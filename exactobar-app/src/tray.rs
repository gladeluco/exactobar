//! Native macOS system tray implementation.
//!
//! Handles the menu bar icon(s) with dynamic usage meters using native NSStatusItem APIs.
//! Uses an Objective-C delegate to handle status item clicks and show GPUI popup windows.

#[cfg(target_os = "macos")]
use cocoa::appkit::NSSquareStatusItemLength;
#[cfg(target_os = "macos")]
use cocoa::base::{YES, id, nil};
#[cfg(target_os = "macos")]
use cocoa::foundation::{NSSize, NSString};
#[cfg(target_os = "macos")]
use objc::declare::ClassDecl;
#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use std::sync::Once;

use exactobar_core::{ProviderKind, StatusIndicator};
use gpui::*;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use tracing::{debug, info, warn};

// Linux-specific imports
#[cfg(target_os = "linux")]
use ksni::Icon as KsniIcon;
#[cfg(target_os = "linux")]
use ksni::blocking::TrayMethods as KsniTrayMethods;

use crate::icon::{IconAnimationState, IconRenderer, RenderMode, RenderedIcon};
use crate::menu::TrayMenu;
use crate::state::AppState;

// ============================================================================
// Objective-C Delegate for Status Item Clicks
// ============================================================================

#[cfg(target_os = "macos")]
static REGISTER_DELEGATE: Once = Once::new();

/// Registers our Objective-C delegate class for handling status item clicks.
///
/// This creates a class called "ExactoBarDelegate" with a `statusItemClicked:` method
/// that sends a message through a channel back to Rust/GPUI.
///
/// # Panics
///
/// Panics if the Objective-C class cannot be registered. This is intentionally
/// unrecoverable because:
/// 1. Class registration only fails if NSObject is unavailable (impossible on macOS)
///    or if "ExactoBarDelegate" already exists (a programming bug)
/// 2. Without the delegate class, status bar clicks cannot be handled
/// 3. Uses `Once` so this can only panic once at app startup, not during runtime
///
/// If this panics, it indicates either a broken macOS installation or a bug
/// in our code (e.g., registering the class twice).
#[cfg(target_os = "macos")]
fn register_delegate_class() -> &'static Class {
    REGISTER_DELEGATE.call_once(|| {
        let superclass = class!(NSObject);
        // SAFETY: This expect is acceptable because:
        // - It only runs once (guarded by Once)
        // - Failure means NSObject doesn't exist (impossible) or class name collision (bug)
        // - The app cannot function without click handling
        let mut decl = ClassDecl::new("ExactoBarDelegate", superclass)
            .expect("Failed to create ExactoBarDelegate class - this is a bug");

        // Add instance variable to store the channel sender pointer
        decl.add_ivar::<*mut std::ffi::c_void>("sender_ptr");
        // Add instance variable to store the provider (as u8 for simplicity)
        decl.add_ivar::<u8>("provider_index");

        // Add the action method that handles clicks
        unsafe {
            extern "C" fn status_item_clicked(this: &Object, _sel: Sel, _sender: id) {
                unsafe {
                    let sender_ptr: *mut std::ffi::c_void = *this.get_ivar("sender_ptr");
                    let provider_index: u8 = *this.get_ivar("provider_index");

                    if !sender_ptr.is_null() {
                        // Cast back to our sender type
                        let sender: &Sender<StatusItemClickEvent> =
                            &*(sender_ptr as *const Sender<StatusItemClickEvent>);

                        // Decode provider from index (255 = merged/none)
                        let provider = if provider_index == 255 {
                            None
                        } else {
                            ProviderKind::from_index(provider_index as usize)
                        };

                        // Send the click event
                        let _ = sender.send(StatusItemClickEvent { provider });
                        debug!(provider = ?provider, "Status item clicked");
                    }
                }
            }

            decl.add_method(
                sel!(statusItemClicked:),
                status_item_clicked as extern "C" fn(&Object, Sel, id),
            );
        }

        decl.register();
    });

    // SAFETY: This expect is acceptable because we just registered the class above.
    // If it's not found, something is catastrophically wrong with the Objective-C runtime.
    Class::get("ExactoBarDelegate")
        .expect("ExactoBarDelegate class not found after registration - this is a bug")
}

/// Event sent when a status item is clicked.
#[derive(Debug, Clone)]
struct StatusItemClickEvent {
    provider: Option<ProviderKind>,
}

/// Creates a delegate instance configured to send click events to the given channel.
#[cfg(target_os = "macos")]
fn create_delegate(sender: &Sender<StatusItemClickEvent>, provider: Option<ProviderKind>) -> id {
    let class = register_delegate_class();
    unsafe {
        let delegate: id = msg_send![class, new];

        // Store the sender pointer (we store a raw pointer to the sender)
        // The sender lives in SystemTray which outlives the delegates
        let sender_ptr = sender as *const Sender<StatusItemClickEvent> as *mut std::ffi::c_void;
        (*delegate).set_ivar("sender_ptr", sender_ptr);

        // Store the provider index (255 = none/merged)
        let provider_index: u8 = provider.map(|p| p.to_index() as u8).unwrap_or(255);
        (*delegate).set_ivar("provider_index", provider_index);

        delegate
    }
}

// ============================================================================
// Linux SNI (StatusNotifierItem) Implementation
// ============================================================================

/// Event sent when a Linux tray action is triggered.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
enum LinuxTrayEvent {
    /// Tray icon was clicked (left click).
    Activate { x: i32, y: i32 },
    /// "Open Menu" menu item was clicked.
    OpenMenu,
    /// "Refresh" menu item was clicked.
    Refresh,
    /// "Settings" menu item was clicked.
    Settings,
    /// "Quit" menu item was clicked.
    Quit,
}

/// Linux tray struct implementing ksni::Tray trait.
///
/// This provides the StatusNotifierItem implementation for Linux desktop
/// environments that support the SNI specification (KDE, GNOME with extensions, etc.).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct LinuxTray {
    /// Channel sender for communicating events back to GPUI.
    event_sender: Sender<LinuxTrayEvent>,
    /// The tray icon (ARGB format).
    icon: KsniIcon,
}

#[cfg(target_os = "linux")]
impl LinuxTray {
    /// Creates a new Linux tray with the given event sender and icon.
    fn new(event_sender: Sender<LinuxTrayEvent>, icon: KsniIcon) -> Self {
        Self { event_sender, icon }
    }
}

#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "exactobar".into()
    }

    fn title(&self) -> String {
        "ExactoBar".into()
    }

    fn icon_pixmap(&self) -> Vec<KsniIcon> {
        vec![self.icon.clone()]
    }

    fn activate(&mut self, x: i32, y: i32) {
        let _ = self.event_sender.send(LinuxTrayEvent::Activate { x, y });
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Refresh".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.event_sender.send(LinuxTrayEvent::Refresh);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Settings".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.event_sender.send(LinuxTrayEvent::Settings);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.event_sender.send(LinuxTrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

// ============================================================================
// System Tray
// ============================================================================

/// Native system tray manager (macOS NSStatusItem / Linux SNI).
///
/// On macOS: Creates real NSStatusItem objects in the macOS menu bar.
/// Uses an Objective-C delegate to handle clicks and show GPUI popup windows.
///
/// On Linux: Uses the StatusNotifierItem (SNI) specification via ksni crate
/// for desktop environments that support it (KDE, GNOME with extensions).
pub struct SystemTray {
    // ========================================================================
    // macOS-specific fields
    // ========================================================================
    /// Native status items by provider (macOS).
    #[cfg(target_os = "macos")]
    status_items: HashMap<ProviderKind, id>,

    /// Merged status item (when merge mode is enabled) (macOS).
    #[cfg(target_os = "macos")]
    merged_status_item: Option<id>,

    /// Delegate objects for handling clicks (must be kept alive) (macOS).
    #[cfg(target_os = "macos")]
    delegates: Vec<id>,

    /// Channel sender for click events (macOS).
    /// IMPORTANT: Must be Box to maintain stable pointer address when SystemTray
    /// is moved into global storage. Delegates hold raw pointers to this.
    #[cfg(target_os = "macos")]
    click_sender: Box<Sender<StatusItemClickEvent>>,

    /// Channel receiver for click events (macOS).
    #[cfg(target_os = "macos")]
    click_receiver: Option<Receiver<StatusItemClickEvent>>,

    // ========================================================================
    // Linux-specific fields
    // ========================================================================
    /// Handle to the ksni tray service (Linux).
    #[cfg(target_os = "linux")]
    sni_handle: Option<ksni::blocking::Handle<LinuxTray>>,

    /// Channel sender for Linux tray events.
    #[cfg(target_os = "linux")]
    linux_event_sender: Sender<LinuxTrayEvent>,

    /// Channel receiver for Linux tray events.
    #[cfg(target_os = "linux")]
    linux_event_receiver: Option<Receiver<LinuxTrayEvent>>,

    // ========================================================================
    // Common fields (all platforms)
    // ========================================================================
    /// Icon renderer.
    renderer: IconRenderer,

    /// Whether icons are merged.
    merge_mode: bool,

    /// Currently displayed menu (if any).
    menu_window: Option<AnyWindowHandle>,

    /// Loading animation phase.
    loading_phase: f64,

    /// Animation states per provider.
    animation_states: HashMap<ProviderKind, IconAnimationState>,

    /// Whether "surprise me" mode (random animations) is enabled.
    surprise_me_enabled: bool,

    /// Time since last random animation event.
    last_random_event: std::time::Instant,
}

impl Global for SystemTray {}

#[cfg(target_os = "macos")]
impl SystemTray {
    /// Creates a new system tray with native macOS status items.
    ///
    /// Sets up an Objective-C delegate to handle status item clicks, which
    /// sends events through a channel that we poll to show GPUI popup windows.
    pub fn new(cx: &mut App) -> Self {
        let state = cx.global::<AppState>();
        let merge_mode = state.settings.read(cx).merge_icons();
        let surprise_me_enabled = state.settings.read(cx).random_blink_enabled();
        let providers = state.enabled_providers(cx);

        // Use template mode for macOS menu bar (automatic dark/light mode)
        let renderer = IconRenderer::new().with_mode(RenderMode::Template);

        // Create channel for click events from Objective-C delegate
        // Box the sender so it has a stable heap address (survives struct moves)
        let (click_sender, click_receiver) = mpsc::channel();
        let click_sender = Box::new(click_sender);

        // Initialize animation states for all providers
        let mut animation_states = HashMap::new();
        for provider in &providers {
            animation_states.insert(*provider, IconAnimationState::default());
        }

        let mut tray = Self {
            status_items: HashMap::new(),
            merged_status_item: None,
            delegates: Vec::new(),
            click_sender,
            click_receiver: Some(click_receiver),
            renderer,
            merge_mode,
            menu_window: None,
            loading_phase: 0.0,
            animation_states,
            surprise_me_enabled,
            last_random_event: std::time::Instant::now(),
        };

        // Create native status items
        if merge_mode {
            tray.create_merged_status_item(cx);
        } else {
            for provider in providers {
                tray.create_status_item(provider, cx);
            }
        }

        info!(merge_mode = merge_mode, "Native system tray initialized");
        tray
    }

    /// Starts the click event listener.
    ///
    /// This should be called after the SystemTray is set as a global.
    /// It spawns a background task that polls the click channel and
    /// calls `toggle_menu()` when a status item is clicked.
    pub fn start_click_listener(&mut self, cx: &mut App) {
        // Take ownership of the receiver
        let Some(receiver) = self.click_receiver.take() else {
            warn!("Click listener already started");
            return;
        };

        // Spawn a background task to poll for click events
        // We use a timer to periodically check the channel
        cx.spawn(async move |cx| {
            loop {
                // Check for click events (non-blocking)
                while let Ok(event) = receiver.try_recv() {
                    debug!(provider = ?event.provider, "Processing status item click");
                    let _ = cx.update_global::<SystemTray, _>(|tray, cx| {
                        tray.toggle_menu(event.provider, cx);
                    });
                }

                // Sleep briefly to avoid busy-waiting
                // 16ms ≈ 60fps, responsive enough for UI
                smol::Timer::after(std::time::Duration::from_millis(16)).await;
            }
        })
        .detach();

        info!("Status item click listener started");
    }

    /// Creates a native NSStatusItem for a provider.
    ///
    /// Instead of attaching an NSMenu, we set up the button's target/action
    /// to call our Objective-C delegate, which sends a message through the
    /// channel to show the GPUI popup window.
    fn create_status_item(&mut self, provider: ProviderKind, cx: &mut App) {
        unsafe {
            // Get the system status bar
            let status_bar: id = msg_send![class!(NSStatusBar), systemStatusBar];

            // Create a status item with square length (fits icon nicely)
            let status_item: id =
                msg_send![status_bar, statusItemWithLength: NSSquareStatusItemLength];

            // Retain the status item so it doesn't get deallocated
            let _: () = msg_send![status_item, retain];

            // Render the initial icon
            let state = cx.global::<AppState>();
            let snapshot = state.get_snapshot(provider, cx);
            let status = state.get_status(provider, cx);
            let status_indicator = status.map(|s| s.indicator).unwrap_or(StatusIndicator::None);

            let rendered = self.renderer.render(
                provider,
                snapshot.as_ref(),
                false,
                Some(status_indicator),
                None,
            );

            // Set the icon image
            self.set_status_item_image(status_item, &rendered);

            // Create delegate for handling clicks (instead of NSMenu)
            let delegate = create_delegate(&self.click_sender, Some(provider));
            let _: () = msg_send![delegate, retain];
            self.delegates.push(delegate);

            // Get the button and set up click handling
            let button: id = msg_send![status_item, button];
            if button != nil {
                // Set target and action for the button
                let _: () = msg_send![button, setTarget: delegate];
                let _: () = msg_send![button, setAction: sel!(statusItemClicked:)];
                debug!("Set up click handler for status item button");
            } else {
                warn!("Status item button is nil, cannot set up click handler");
            }

            // Store the status item
            self.status_items.insert(provider, status_item);

            debug!(provider = ?provider, "Created native status item with click handler");
        }
    }

    /// Creates a merged status item (for merge mode).
    ///
    /// Uses the same delegate pattern as individual status items.
    fn create_merged_status_item(&mut self, cx: &mut App) {
        let state = cx.global::<AppState>();
        let providers = state.enabled_providers(cx);

        if let Some(first) = providers.first() {
            unsafe {
                let status_bar: id = msg_send![class!(NSStatusBar), systemStatusBar];
                let status_item: id =
                    msg_send![status_bar, statusItemWithLength: NSSquareStatusItemLength];
                let _: () = msg_send![status_item, retain];

                let snapshot = state.get_snapshot(*first, cx);
                let rendered = self
                    .renderer
                    .render(*first, snapshot.as_ref(), false, None, None);
                self.set_status_item_image(status_item, &rendered);

                // Create delegate for handling clicks (provider=None for merged)
                let delegate = create_delegate(&self.click_sender, None);
                let _: () = msg_send![delegate, retain];
                self.delegates.push(delegate);

                // Get the button and set up click handling
                let button: id = msg_send![status_item, button];
                if button != nil {
                    let _: () = msg_send![button, setTarget: delegate];
                    let _: () = msg_send![button, setAction: sel!(statusItemClicked:)];
                }

                self.merged_status_item = Some(status_item);
                debug!("Created merged status item with click handler");
            }
        }
    }

    /// Sets the image for a status item from a RenderedIcon.
    fn set_status_item_image(&self, status_item: id, rendered: &RenderedIcon) {
        unsafe {
            // Convert to PNG bytes
            let png_data = rendered.to_png();

            // Create NSData from PNG bytes
            let ns_data: id = msg_send![
                class!(NSData),
                dataWithBytes: png_data.as_ptr()
                length: png_data.len()
            ];

            // Create NSImage from data
            let ns_image: id = msg_send![class!(NSImage), alloc];
            let ns_image: id = msg_send![ns_image, initWithData: ns_data];

            if ns_image != nil {
                // Set as template image for proper dark/light mode support
                // Template images are rendered by macOS in the appropriate color
                let _: () = msg_send![ns_image, setTemplate: YES];

                // Set size (18x11 points for our icon dimensions)
                // macOS handles retina scaling automatically
                let size = NSSize::new(18.0, 11.0);
                let _: () = msg_send![ns_image, setSize: size];

                // Get the status item's button and set the image
                let button: id = msg_send![status_item, button];
                if button != nil {
                    let _: () = msg_send![button, setImage: ns_image];
                } else {
                    warn!("Status item button is nil");
                }

                // Release the image (button retains it)
                let _: () = msg_send![ns_image, release];
            } else {
                warn!("Failed to create NSImage from PNG data");
            }
        }
    }

    /// Updates the icon for a specific provider.
    pub fn update_icon(&mut self, provider: ProviderKind, cx: &mut App) {
        let state = cx.global::<AppState>();
        let snapshot = state.get_snapshot(provider, cx);
        let is_refreshing = state.is_provider_refreshing(provider, cx);
        let has_error = state.get_error(provider, cx).is_some();
        let status = state.get_status(provider, cx);

        // Check if snapshot is stale (older than 10 minutes)
        let stale = snapshot.as_ref().is_some_and(|s| {
            let threshold = chrono::Duration::minutes(10);
            chrono::Utc::now() - s.updated_at > threshold
        });

        // Get animation state for this provider
        let animation = self.animation_states.get(&provider);

        let rendered = if is_refreshing {
            self.loading_phase += 0.1;
            self.renderer.render_loading(provider, self.loading_phase)
        } else if has_error {
            self.renderer.render_error(provider)
        } else {
            let status_indicator = status.map(|s| s.indicator).unwrap_or(StatusIndicator::None);

            self.renderer.render(
                provider,
                snapshot.as_ref(),
                stale,
                Some(status_indicator),
                animation,
            )
        };

        if self.merge_mode {
            if let Some(status_item) = self.merged_status_item {
                self.set_status_item_image(status_item, &rendered);
            }
        } else if let Some(&status_item) = self.status_items.get(&provider) {
            self.set_status_item_image(status_item, &rendered);
        }

        debug!(provider = ?provider, stale = stale, "Icon updated");
    }

    /// Updates all icons based on current state.
    pub fn update_all(&mut self, cx: &mut App) {
        let state = cx.global::<AppState>();
        let providers = state.enabled_providers(cx);

        for provider in providers {
            self.update_icon(provider, cx);
        }
    }

    // ========================================================================
    // Animation Methods
    // ========================================================================

    /// Triggers a blink animation for a provider.
    ///
    /// The blink starts with the eye closed (blink_phase = 1.0) and
    /// gradually opens as tick_animations decays the phase.
    pub fn trigger_blink(&mut self, provider: ProviderKind, cx: &mut App) {
        if let Some(state) = self.animation_states.get_mut(&provider) {
            state.blink_phase = 1.0; // Start closed
        }
        self.update_icon(provider, cx);
    }

    /// Updates animation states (called each frame by the animation timer).
    ///
    /// Decays blink phase so the eye opens back up, and decays wiggle/tilt
    /// for "surprise me" animations.
    fn tick_animations(&mut self, delta_seconds: f32, cx: &mut App) {
        let mut needs_update = Vec::new();

        for (provider, state) in &mut self.animation_states {
            let mut changed = false;

            // Decay blink phase (eye opens back up)
            // Speed: 3.0 means full blink cycle takes ~0.33 seconds
            if state.blink_phase > 0.0 {
                state.blink_phase = (state.blink_phase - delta_seconds * 3.0).max(0.0);
                changed = true;
            }

            // Decay wiggle offset (damped oscillation)
            if state.wiggle_offset.abs() > 0.01 {
                state.wiggle_offset *= 0.9_f32.powf(delta_seconds * 60.0); // Frame-rate independent
                changed = true;
            } else {
                state.wiggle_offset = 0.0;
            }

            // Decay tilt (damped oscillation)
            if state.tilt_degrees.abs() > 0.1 {
                state.tilt_degrees *= 0.9_f32.powf(delta_seconds * 60.0);
                changed = true;
            } else {
                state.tilt_degrees = 0.0;
            }

            if changed {
                needs_update.push(*provider);
            }
        }

        // Only update icons that have active animations
        for provider in needs_update {
            self.update_icon(provider, cx);
        }
    }

    /// Maybe trigger a random animation if "surprise me" is enabled.
    ///
    /// Called periodically by the animation timer. Has a chance to trigger
    /// a random blink, wiggle, or tilt on a random provider.
    fn maybe_random_animation(&mut self, cx: &mut App) {
        if !self.surprise_me_enabled {
            return;
        }

        // Only check every ~30 seconds
        if self.last_random_event.elapsed() < std::time::Duration::from_secs(30) {
            return;
        }

        // 30% chance to trigger when the cooldown expires
        if rand::random::<f32>() >= 0.3 {
            self.last_random_event = std::time::Instant::now();
            return;
        }

        // Pick a random enabled provider
        let providers: Vec<_> = self.animation_states.keys().copied().collect();
        if providers.is_empty() {
            self.last_random_event = std::time::Instant::now();
            return;
        }

        let provider = providers[rand::random::<usize>() % providers.len()];

        // Random animation type
        match rand::random::<u8>() % 3 {
            0 => {
                // Blink
                debug!(provider = ?provider, "Random blink triggered");
                self.trigger_blink(provider, cx);
            }
            1 => {
                // Wiggle
                if let Some(state) = self.animation_states.get_mut(&provider) {
                    state.wiggle_offset = (rand::random::<f32>() - 0.5) * 4.0;
                    debug!(provider = ?provider, wiggle = state.wiggle_offset, "Random wiggle triggered");
                }
            }
            _ => {
                // Tilt
                if let Some(state) = self.animation_states.get_mut(&provider) {
                    state.tilt_degrees = (rand::random::<f32>() - 0.5) * 10.0;
                    debug!(provider = ?provider, tilt = state.tilt_degrees, "Random tilt triggered");
                }
            }
        }

        self.last_random_event = std::time::Instant::now();
    }

    /// Starts the animation tick timer.
    ///
    /// Spawns a background task that runs at ~30fps and updates animation
    /// states. Only performs work when animations are actually active.
    pub fn start_animation_timer(&mut self, cx: &mut App) {
        cx.spawn(async move |mut cx| {
            let mut last_tick = std::time::Instant::now();

            loop {
                // ~30fps is smooth enough for blink animations
                smol::Timer::after(std::time::Duration::from_millis(33)).await;

                let now = std::time::Instant::now();
                let delta = (now - last_tick).as_secs_f32();
                last_tick = now;

                // Update animations in the global SystemTray
                let _ = cx.update_global::<SystemTray, _>(|tray, cx| {
                    tray.tick_animations(delta, cx);
                    tray.maybe_random_animation(cx);
                });
            }
        })
        .detach();

        info!("Animation timer started (~30fps)");
    }

    /// Updates the "surprise me" (random animation) setting.
    pub fn set_surprise_me_enabled(&mut self, enabled: bool) {
        self.surprise_me_enabled = enabled;
        info!(surprise_me = enabled, "Surprise me mode changed");
    }

    /// Ensures a provider has an animation state entry.
    ///
    /// Called when a new provider is added.
    fn ensure_animation_state(&mut self, provider: ProviderKind) {
        self.animation_states.entry(provider).or_default();
    }

    // ========================================================================
    // Mode Switching
    // ========================================================================

    /// Toggles merge mode.
    pub fn set_merge_mode(&mut self, merge: bool, cx: &mut App) {
        if self.merge_mode == merge {
            return;
        }

        // Remove existing status items
        self.remove_all_status_items();

        self.merge_mode = merge;

        // Create new status items in the appropriate mode
        if merge {
            self.create_merged_status_item(cx);
        } else {
            let state = cx.global::<AppState>();
            let providers = state.enabled_providers(cx);
            for provider in providers {
                self.create_status_item(provider, cx);
            }
        }

        info!(merge_mode = merge, "Merge mode changed");
    }

    /// Adds a provider to the tray.
    pub fn add_provider(&mut self, provider: ProviderKind, cx: &mut App) {
        // Ensure animation state exists for this provider
        self.ensure_animation_state(provider);

        if !self.merge_mode && !self.status_items.contains_key(&provider) {
            self.create_status_item(provider, cx);
        }
    }

    /// Removes a provider from the tray.
    pub fn remove_provider(&mut self, provider: ProviderKind) {
        // Clean up animation state
        self.animation_states.remove(&provider);

        if let Some(status_item) = self.status_items.remove(&provider) {
            unsafe {
                let status_bar: id = msg_send![class!(NSStatusBar), systemStatusBar];
                let _: () = msg_send![status_bar, removeStatusItem: status_item];
                let _: () = msg_send![status_item, release];
            }
        }
    }

    /// Removes all status items from the menu bar.
    fn remove_all_status_items(&mut self) {
        unsafe {
            let status_bar: id = msg_send![class!(NSStatusBar), systemStatusBar];

            // Remove individual provider items
            for (_, status_item) in self.status_items.drain() {
                let _: () = msg_send![status_bar, removeStatusItem: status_item];
                let _: () = msg_send![status_item, release];
            }

            // Remove merged item if present
            if let Some(status_item) = self.merged_status_item.take() {
                let _: () = msg_send![status_bar, removeStatusItem: status_item];
                let _: () = msg_send![status_item, release];
            }

            // Release all delegates
            for delegate in self.delegates.drain(..) {
                let _: () = msg_send![delegate, release];
            }
        }
    }

    /// Toggles the tray menu.
    pub fn toggle_menu(&mut self, provider: Option<ProviderKind>, cx: &mut App) {
        if self.menu_window.is_some() {
            self.close_menu(cx);
        } else {
            self.open_menu(provider, cx);
        }
    }

    /// Opens the tray menu as a GPUI popup window with native macOS panel styling.
    ///
    /// Positions the popup directly below the clicked status item, right-aligned.
    /// Uses blurred background for native macOS vibrancy effect.
    ///
    /// COORDINATE SYSTEM NOTES:
    /// - macOS NSScreen uses bottom-left origin (Y increases upward)
    /// - GPUI uses top-left origin (Y increases downward)
    /// - We must convert: gpui_y = screen_height - macos_y - rect_height
    fn open_menu(&mut self, provider: Option<ProviderKind>, cx: &mut App) {
        info!(provider = ?provider, "Opening GPUI popup menu...");
        self.close_menu(cx);

        let menu = TrayMenu::new(provider);

        let menu_width = 340.0_f32; // Match MenuPanel width
        let menu_height = 600.0_f32; // Match max_h in MenuPanel

        // Get screen dimensions for coordinate conversion (macOS -> GPUI)
        let (screen_width, screen_height) = unsafe {
            let screen: id = msg_send![class!(NSScreen), mainScreen];
            let frame: cocoa::foundation::NSRect = msg_send![screen, frame];
            (frame.size.width as f32, frame.size.height as f32)
        };

        // Get status item position (macOS coordinates - origin at bottom-left)
        let frame_info = self.get_status_item_frame(provider);
        debug!(frame = ?frame_info, screen_height = screen_height, "Status item frame (macOS coords)");

        let (origin_x, origin_y) = if let Some((mac_x, mac_y, item_w, item_h)) = frame_info {
            // macOS origin is bottom-left, GPUI origin is top-left
            // Status item's mac_y is the BOTTOM of the icon
            // So its TOP is at: mac_y + item_h
            // The TOP of screen in GPUI is y=0
            // So GPUI Y for the bottom of status item = screen_height - (mac_y + item_h)
            // We want menu just below that, so add a small gap

            let status_item_top_gpui = screen_height - (mac_y + item_h);
            let status_item_bottom_gpui = status_item_top_gpui + item_h;

            // Position menu just below the status item
            let menu_y = status_item_bottom_gpui + 2.0; // 2px gap below icon

            // Right-align menu with status item's right edge
            let menu_x = (mac_x + item_w - menu_width).max(10.0);

            info!(
                mac_coords = ?(mac_x, mac_y, item_w, item_h),
                gpui_coords = ?(menu_x, menu_y),
                "Coordinate conversion"
            );

            (menu_x, menu_y)
        } else {
            // Fallback: Position at RIGHT edge of screen, just below menu bar
            let menu_x = screen_width - menu_width - 10.0; // 10px from right edge
            let menu_y = 30.0; // Just below menu bar
            info!(
                fallback = true,
                x = menu_x,
                y = menu_y,
                "Using fallback position"
            );
            (menu_x, menu_y)
        };

        let bounds = Bounds::new(
            point(px(origin_x), px(origin_y)),
            size(px(menu_width), px(menu_height)),
        );

        let window_options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            display_id: None,
            window_background: WindowBackgroundAppearance::Blurred,
            app_id: None,
            window_min_size: None,
            window_decorations: Some(WindowDecorations::Client),
            is_minimizable: false,
            is_resizable: false,
            tabbing_identifier: None,
        };

        match cx.open_window(window_options, |_window, cx| cx.new(|_| menu)) {
            Ok(handle) => {
                self.menu_window = Some(handle.into());
                info!(x = origin_x, y = origin_y, "✅ Menu opened at position");
            }
            Err(e) => {
                warn!(error = ?e, "❌ Failed to open menu");
            }
        }
    }

    /// Gets the frame (x, y, width, height) of a status item's button.
    ///
    /// Returns the screen coordinates where the status item is displayed,
    /// useful for positioning popup windows.
    fn get_status_item_frame(
        &self,
        provider: Option<ProviderKind>,
    ) -> Option<(f32, f32, f32, f32)> {
        unsafe {
            let status_item = if self.merge_mode {
                self.merged_status_item?
            } else {
                let p = provider?;
                *self.status_items.get(&p)?
            };

            let button: id = msg_send![status_item, button];
            if button == nil {
                return None;
            }

            let window: id = msg_send![button, window];
            if window == nil {
                return None;
            }

            // Get the button's frame in window coordinates
            let frame: cocoa::foundation::NSRect = msg_send![button, frame];

            // Convert to screen coordinates
            let screen_rect: cocoa::foundation::NSRect =
                msg_send![window, convertRectToScreen: frame];

            Some((
                screen_rect.origin.x as f32,
                screen_rect.origin.y as f32,
                screen_rect.size.width as f32,
                screen_rect.size.height as f32,
            ))
        }
    }

    /// Closes the tray menu.
    fn close_menu(&mut self, cx: &mut App) {
        if let Some(handle) = self.menu_window.take() {
            // Actually close the GPUI window, not just drop the handle
            let _ = cx.update_window(handle, |_, window, _| {
                window.remove_window();
            });
        }
    }

    /// Gets the icon PNG for a provider.
    pub fn get_icon_png(&self, provider: ProviderKind, cx: &App) -> Option<Vec<u8>> {
        let state = cx.global::<AppState>();
        let snapshot = state.get_snapshot(provider, cx);
        let rendered = self
            .renderer
            .render(provider, snapshot.as_ref(), false, None, None);
        Some(rendered.to_png())
    }
}

#[cfg(target_os = "macos")]
impl Drop for SystemTray {
    fn drop(&mut self) {
        // Clean up all native status items
        self.remove_all_status_items();
        info!("System tray cleaned up");
    }
}

// ============================================================================
// Linux SNI Implementation
// ============================================================================

#[cfg(target_os = "linux")]
impl SystemTray {
    /// Creates a new system tray with Linux SNI (StatusNotifierItem) support.
    ///
    /// Sets up a ksni tray service that communicates with the desktop environment's
    /// system tray implementation (KDE, GNOME with AppIndicator extension, etc.).
    pub fn new(cx: &mut App) -> Self {
        let state = cx.global::<AppState>();
        let merge_mode = state.settings.read(cx).merge_icons();
        let surprise_me_enabled = state.settings.read(cx).random_blink_enabled();
        let providers = state.enabled_providers(cx);

        // Use Colored mode for Linux (we'll convert RGBA to ARGB for ksni)
        let renderer = IconRenderer::new().with_mode(RenderMode::Colored);

        // Create channel for Linux tray events
        let (linux_event_sender, linux_event_receiver) = mpsc::channel();

        // Initialize animation states for all providers
        let mut animation_states = HashMap::new();
        for provider in &providers {
            animation_states.insert(*provider, IconAnimationState::default());
        }

        let mut tray = Self {
            sni_handle: None,
            linux_event_sender,
            linux_event_receiver: Some(linux_event_receiver),
            renderer,
            merge_mode,
            menu_window: None,
            loading_phase: 0.0,
            animation_states,
            surprise_me_enabled,
            last_random_event: std::time::Instant::now(),
        };

        // Create the SNI tray
        tray.create_sni_tray(cx);

        info!(merge_mode = merge_mode, "Linux SNI system tray initialized");
        tray
    }

    /// Creates the ksni tray service.
    fn create_sni_tray(&mut self, cx: &mut App) {
        let state = cx.global::<AppState>();
        let providers = state.enabled_providers(cx);

        // Get the first provider for the initial icon
        let first_provider = providers.first().copied();
        let icon = self.render_linux_icon(first_provider, cx);

        // Create the Linux tray
        let linux_tray = LinuxTray::new(self.linux_event_sender.clone(), icon);

        // Spawn the tray service
        match linux_tray.spawn() {
            Ok(handle) => {
                self.sni_handle = Some(handle);
                info!("Linux SNI tray service started");
            }
            Err(e) => {
                warn!(error = ?e, "Failed to start Linux SNI tray service");
            }
        }
    }

    /// Renders an icon for Linux in ARGB format (as required by ksni).
    fn render_linux_icon(&self, provider: Option<ProviderKind>, cx: &App) -> KsniIcon {
        let state = cx.global::<AppState>();

        // Get snapshot and status for rendering
        let (snapshot, status_indicator) = if let Some(p) = provider {
            let snapshot = state.get_snapshot(p, cx);
            let status = state.get_status(p, cx);
            let indicator = status.map(|s| s.indicator).unwrap_or(StatusIndicator::None);
            (snapshot, indicator)
        } else {
            (None, StatusIndicator::None)
        };

        // Render the icon
        let rendered = if let Some(p) = provider {
            self.renderer
                .render(p, snapshot.as_ref(), false, Some(status_indicator), None)
        } else {
            // Fallback: render a default icon
            self.renderer.render(
                ProviderKind::Codex,
                None,
                false,
                Some(StatusIndicator::None),
                None,
            )
        };

        // Get raw RGBA pixels
        let (width, height, mut pixels) = rendered.to_rgba_pixels();

        // Convert RGBA to ARGB (Linux SNI expects ARGB in network byte order)
        for pixel in pixels.chunks_exact_mut(4) {
            // RGBA -> ARGB: [R, G, B, A] -> [A, R, G, B]
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            pixel[0] = a;
            pixel[1] = r;
            pixel[2] = g;
            pixel[3] = b;
        }

        KsniIcon {
            width: width as i32,
            height: height as i32,
            data: pixels,
        }
    }

    /// Starts the event listener for Linux tray events.
    ///
    /// Spawns a background task that polls the event channel and
    /// handles tray actions (clicks, menu items).
    pub fn start_click_listener(&mut self, cx: &mut App) {
        let Some(receiver) = self.linux_event_receiver.take() else {
            warn!("Linux event listener already started");
            return;
        };

        cx.spawn(async move |cx| {
            loop {
                // Check for events (non-blocking)
                while let Ok(event) = receiver.try_recv() {
                    debug!(event = ?event, "Processing Linux tray event");
                    match event {
                        LinuxTrayEvent::Activate { x, y } => {
                            info!(x = x, y = y, "Tray icon activated at position");
                            let _ = cx.update_global::<SystemTray, _>(|tray, cx| {
                                tray.toggle_menu_at(None, Some((x, y)), cx);
                            });
                        }
                        LinuxTrayEvent::OpenMenu => {
                            let _ = cx.update_global::<SystemTray, _>(|tray, cx| {
                                tray.toggle_menu_at(None, None, cx);
                            });
                        }
                        LinuxTrayEvent::Refresh => {
                            info!("Refresh requested from tray menu");
                            let _ = cx.update_global::<AppState, _>(|state, cx| {
                                state.refresh_all(cx);
                            });
                        }
                        LinuxTrayEvent::Settings => {
                            info!("Settings requested from tray menu");
                            let _ = cx.update(|cx| {
                                crate::actions::open_settings(cx);
                            });
                        }
                        LinuxTrayEvent::Quit => {
                            info!("Quit requested from tray menu");
                            let _ = cx.update(|cx| {
                                cx.quit();
                            });
                        }
                    }
                }

                // Sleep briefly to avoid busy-waiting
                smol::Timer::after(std::time::Duration::from_millis(16)).await;
            }
        })
        .detach();

        info!("Linux tray event listener started");
    }

    /// Updates the icon for a specific provider.
    pub fn update_icon(&mut self, provider: ProviderKind, cx: &mut App) {
        let state = cx.global::<AppState>();
        let snapshot = state.get_snapshot(provider, cx);
        let is_refreshing = state.is_provider_refreshing(provider, cx);
        let has_error = state.get_error(provider, cx).is_some();
        let status = state.get_status(provider, cx);

        // Check if snapshot is stale (older than 10 minutes)
        let stale = snapshot.as_ref().is_some_and(|s| {
            let threshold = chrono::Duration::minutes(10);
            chrono::Utc::now() - s.updated_at > threshold
        });

        // Get animation state for this provider
        let animation = self.animation_states.get(&provider);

        let rendered = if is_refreshing {
            self.loading_phase += 0.1;
            self.renderer.render_loading(provider, self.loading_phase)
        } else if has_error {
            self.renderer.render_error(provider)
        } else {
            let status_indicator = status.map(|s| s.indicator).unwrap_or(StatusIndicator::None);

            self.renderer.render(
                provider,
                snapshot.as_ref(),
                stale,
                Some(status_indicator),
                animation,
            )
        };

        // Convert to ARGB for ksni
        let (width, height, mut pixels) = rendered.to_rgba_pixels();
        for pixel in pixels.chunks_exact_mut(4) {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            pixel[0] = a;
            pixel[1] = r;
            pixel[2] = g;
            pixel[3] = b;
        }

        let icon = KsniIcon {
            width: width as i32,
            height: height as i32,
            data: pixels,
        };

        // Update the tray icon
        if let Some(handle) = &self.sni_handle {
            handle.update(|tray| {
                tray.icon = icon;
            });
        }

        debug!(provider = ?provider, stale = stale, "Icon updated (Linux)");
    }

    /// Updates all icons based on current state.
    pub fn update_all(&mut self, cx: &mut App) {
        let state = cx.global::<AppState>();
        let providers = state.enabled_providers(cx);

        // On Linux, we only have one icon, so just update with the first provider
        if let Some(&provider) = providers.first() {
            self.update_icon(provider, cx);
        }
    }

    // ========================================================================
    // Animation Methods
    // ========================================================================

    /// Triggers a blink animation for a provider.
    pub fn trigger_blink(&mut self, provider: ProviderKind, cx: &mut App) {
        if let Some(state) = self.animation_states.get_mut(&provider) {
            state.blink_phase = 1.0;
        }
        self.update_icon(provider, cx);
    }

    /// Updates animation states (called each frame by the animation timer).
    fn tick_animations(&mut self, delta_seconds: f32, cx: &mut App) {
        let mut needs_update = Vec::new();

        for (provider, state) in &mut self.animation_states {
            let mut changed = false;

            if state.blink_phase > 0.0 {
                state.blink_phase = (state.blink_phase - delta_seconds * 3.0).max(0.0);
                changed = true;
            }

            if state.wiggle_offset.abs() > 0.01 {
                state.wiggle_offset *= 0.9_f32.powf(delta_seconds * 60.0);
                changed = true;
            } else {
                state.wiggle_offset = 0.0;
            }

            if state.tilt_degrees.abs() > 0.1 {
                state.tilt_degrees *= 0.9_f32.powf(delta_seconds * 60.0);
                changed = true;
            } else {
                state.tilt_degrees = 0.0;
            }

            if changed {
                needs_update.push(*provider);
            }
        }

        for provider in needs_update {
            self.update_icon(provider, cx);
        }
    }

    /// Maybe trigger a random animation if "surprise me" is enabled.
    fn maybe_random_animation(&mut self, cx: &mut App) {
        if !self.surprise_me_enabled {
            return;
        }

        if self.last_random_event.elapsed() < std::time::Duration::from_secs(30) {
            return;
        }

        if rand::random::<f32>() >= 0.3 {
            self.last_random_event = std::time::Instant::now();
            return;
        }

        let providers: Vec<_> = self.animation_states.keys().copied().collect();
        if providers.is_empty() {
            self.last_random_event = std::time::Instant::now();
            return;
        }

        let provider = providers[rand::random::<usize>() % providers.len()];

        match rand::random::<u8>() % 3 {
            0 => {
                debug!(provider = ?provider, "Random blink triggered");
                self.trigger_blink(provider, cx);
            }
            1 => {
                if let Some(state) = self.animation_states.get_mut(&provider) {
                    state.wiggle_offset = (rand::random::<f32>() - 0.5) * 4.0;
                    debug!(provider = ?provider, wiggle = state.wiggle_offset, "Random wiggle triggered");
                }
            }
            _ => {
                if let Some(state) = self.animation_states.get_mut(&provider) {
                    state.tilt_degrees = (rand::random::<f32>() - 0.5) * 10.0;
                    debug!(provider = ?provider, tilt = state.tilt_degrees, "Random tilt triggered");
                }
            }
        }

        self.last_random_event = std::time::Instant::now();
    }

    /// Starts the animation tick timer.
    pub fn start_animation_timer(&mut self, cx: &mut App) {
        cx.spawn(async move |mut cx| {
            let mut last_tick = std::time::Instant::now();

            loop {
                smol::Timer::after(std::time::Duration::from_millis(33)).await;

                let now = std::time::Instant::now();
                let delta = (now - last_tick).as_secs_f32();
                last_tick = now;

                let _ = cx.update_global::<SystemTray, _>(|tray, cx| {
                    tray.tick_animations(delta, cx);
                    tray.maybe_random_animation(cx);
                });
            }
        })
        .detach();

        info!("Animation timer started (~30fps)");
    }

    /// Updates the "surprise me" (random animation) setting.
    pub fn set_surprise_me_enabled(&mut self, enabled: bool) {
        self.surprise_me_enabled = enabled;
        info!(surprise_me = enabled, "Surprise me mode changed");
    }

    /// Ensures a provider has an animation state entry.
    fn ensure_animation_state(&mut self, provider: ProviderKind) {
        self.animation_states.entry(provider).or_default();
    }

    // ========================================================================
    // Mode Switching
    // ========================================================================

    /// Toggles merge mode (no-op on Linux since we always have one icon).
    pub fn set_merge_mode(&mut self, merge: bool, _cx: &mut App) {
        self.merge_mode = merge;
        // Linux only supports one icon anyway, so this is a no-op
        debug!(merge_mode = merge, "Merge mode changed (Linux - no-op)");
    }

    /// Adds a provider to the tray.
    pub fn add_provider(&mut self, provider: ProviderKind, _cx: &mut App) {
        self.ensure_animation_state(provider);
        // Linux only has one icon, so we don't create additional items
    }

    /// Removes a provider from the tray.
    pub fn remove_provider(&mut self, provider: ProviderKind) {
        self.animation_states.remove(&provider);
        // Linux only has one icon, so nothing else to do
    }

    /// Toggles the tray menu (legacy, no position).
    pub fn toggle_menu(&mut self, provider: Option<ProviderKind>, cx: &mut App) {
        self.toggle_menu_at(provider, None, cx);
    }

    /// Toggles the tray menu with optional click position.
    pub fn toggle_menu_at(
        &mut self,
        provider: Option<ProviderKind>,
        click_pos: Option<(i32, i32)>,
        cx: &mut App,
    ) {
        if self.menu_window.is_some() {
            self.close_menu(cx);
        } else {
            self.open_menu_at(provider, click_pos, cx);
        }
    }

    /// Opens the tray menu as a GPUI popup window.
    ///
    /// On Linux, we position the window near the click position (which is near the tray icon).
    /// The menu appears below and to the left of the click point so it doesn't obscure the icon.
    fn open_menu_at(
        &mut self,
        provider: Option<ProviderKind>,
        click_pos: Option<(i32, i32)>,
        cx: &mut App,
    ) {
        info!(provider = ?provider, click_pos = ?click_pos, "Opening GPUI popup menu (Linux)...");
        self.close_menu(cx);

        let menu = TrayMenu::new(provider);

        let menu_width = 340.0_f32;
        let menu_height = 600.0_f32;

        // Position menu near the click (tray icon location)
        let (origin_x, origin_y) = if let Some((click_x, click_y)) = click_pos {
            // Get screen dimensions
            let (screen_width, screen_height) = cx
                .primary_display()
                .map(|d| {
                    let b = d.bounds();
                    (f32::from(b.size.width), f32::from(b.size.height))
                })
                .unwrap_or((1920.0, 1080.0));

            // Position menu to the left of click point, keeping on screen
            let x = (click_x as f32 - menu_width).clamp(10.0, screen_width - menu_width - 10.0);

            // Position menu so its bottom edge aligns with the click point
            // This puts the menu directly above where the user clicked (the tray icon)
            let y = (click_y as f32 - menu_height).max(10.0);

            info!(
                click_x = click_x,
                click_y = click_y,
                screen_w = screen_width,
                screen_h = screen_height,
                menu_x = x,
                menu_y = y,
                "Positioning menu above bottom panel"
            );
            (x, y)
        } else if let Some(display) = cx.primary_display() {
            // Fallback: top-right of screen
            let screen_bounds = display.bounds();
            let screen_width: f32 = screen_bounds.size.width.into();
            let x = screen_width - menu_width - 10.0;
            let y = 30.0_f32;
            info!(
                screen_width = screen_width,
                x = x,
                y = y,
                "Positioning menu at top-right (fallback)"
            );
            (x, y)
        } else {
            // Last resort fallback
            warn!("Could not get click position or display info, using hardcoded fallback");
            (100.0_f32, 30.0_f32)
        };

        let bounds = Bounds::new(
            point(px(origin_x), px(origin_y)),
            size(px(menu_width), px(menu_height)),
        );

        let window_options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            display_id: None,
            // Linux doesn't support blur, so we use opaque background
            window_background: WindowBackgroundAppearance::Opaque,
            app_id: Some("exactobar".into()),
            window_min_size: None,
            window_decorations: Some(WindowDecorations::Client),
            is_minimizable: false,
            is_resizable: false,
            tabbing_identifier: None,
        };

        match cx.open_window(window_options, |_window, cx| cx.new(|_| menu)) {
            Ok(handle) => {
                self.menu_window = Some(handle.into());
                info!(
                    x = origin_x,
                    y = origin_y,
                    "✅ Menu opened at position (Linux)"
                );
            }
            Err(e) => {
                warn!(error = ?e, "❌ Failed to open menu");
            }
        }
    }

    /// Closes the tray menu.
    fn close_menu(&mut self, cx: &mut App) {
        if let Some(handle) = self.menu_window.take() {
            let _ = cx.update_window(handle, |_, window, _| {
                window.remove_window();
            });
        }
    }

    /// Gets the icon PNG for a provider.
    pub fn get_icon_png(&self, provider: ProviderKind, cx: &App) -> Option<Vec<u8>> {
        let state = cx.global::<AppState>();
        let snapshot = state.get_snapshot(provider, cx);
        let rendered = self
            .renderer
            .render(provider, snapshot.as_ref(), false, None, None);
        Some(rendered.to_png())
    }
}

#[cfg(target_os = "linux")]
impl Drop for SystemTray {
    fn drop(&mut self) {
        // The ksni handle will be dropped automatically, which stops the service
        info!("Linux system tray cleaned up");
    }
}
