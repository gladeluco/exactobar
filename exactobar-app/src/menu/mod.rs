//! Rich popup menu matching native macOS panel styling.
//!
//! This module provides the main popup menu shown when clicking the tray icon,
//! featuring provider switcher, rich menu cards with progress bars, and working action buttons.
//!
//! Uses transparent backgrounds to let the window's blur effect show through.
//!
//! # Module Structure
//!
//! - `mod.rs` - MenuPanel, MenuHeader, TrayMenu alias
//! - `card.rs` - MenuCard, MenuCardData, CardHeader
//! - `error.rs` - EnhancedErrorSection, InstallHint, clipboard helpers
//! - `usage.rs` - UsageMetricsSection, ProgressBar
//! - `actions.rs` - ActionButtonsSection, ActionButton, URL opening
//! - `footer.rs` - MenuFooter, FooterActionButton

#![allow(dead_code)]

mod actions;
mod card;
mod error;
mod footer;
mod tabs;
mod usage;

pub use tabs::SelectedTab;

// Re-exports for public API
pub use card::{MenuCard, MenuCardData};
pub use error::{EnhancedErrorSection, InstallHint, copy_to_clipboard, get_install_hint};
pub use footer::MenuFooter;

use exactobar_core::ProviderKind;
use gpui::prelude::FluentBuilder;
use gpui::*;
use tracing::{debug, info};

use crate::state::AppState;
use crate::theme;

// ============================================================================
// Menu Panel
// ============================================================================

/// The main popup panel (replaces TrayMenu).
pub struct MenuPanel {
    /// Currently selected tab (All or a specific provider).
    selected_tab: SelectedTab,
}

impl MenuPanel {
    /// Creates a new menu panel.
    pub fn new(initial_provider: Option<ProviderKind>) -> Self {
        Self {
            // Default to "All" tab, but if a specific provider is requested, use that
            selected_tab: initial_provider
                .map(SelectedTab::Provider)
                .unwrap_or(SelectedTab::All),
        }
    }

    /// Renders the provider switcher with WORKING click handlers.
    /// This must be called from render() where we have access to cx.listener().
    fn render_provider_switcher(
        &self,
        providers: &[ProviderKind],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Build the "All" tab button first
        let is_all_selected = self.selected_tab.is_all();
        let all_btn = div()
            .id("switch-all")
            .px(px(10.))
            .py(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(if is_all_selected {
                gpui::white()
            } else {
                theme::text_primary()
            })
            .when(is_all_selected, |el| el.bg(theme::accent()))
            .when(!is_all_selected, |el| {
                el.hover(|s| s.bg(theme::hover()))
                    .active(|s| s.bg(theme::active()))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _window, cx| {
                    info!("All tab clicked!");
                    this.selected_tab = SelectedTab::All;
                    cx.notify();
                }),
            )
            .child(div().text_sm().child("All"));

        div()
            .px(px(10.))
            .py(px(8.))
            // TRUE LIQUID GLASS: NO background - let window blur shine through!
            .flex()
            .flex_wrap()
            .gap(px(4.))
            // "All" tab first
            .child(all_btn)
            // Then individual provider tabs
            .children(providers.iter().map(|&provider| {
                let is_selected = self.selected_tab == SelectedTab::Provider(provider);
                let name = provider.display_name();

                let mut btn = div()
                    .id(SharedString::from(format!("switch-{:?}", provider)))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(theme::text_primary())
                    // THE MAGIC: cx.listener() gives us access to `this`!
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _window, cx| {
                            info!(provider = ?provider, "Provider switch button clicked!");
                            this.selected_tab = SelectedTab::Provider(provider);

                            // Check if this provider has data, if not trigger refresh
                            let state = cx.global::<AppState>();
                            let has_snapshot = state.get_snapshot(provider, cx).is_some();
                            if !has_snapshot {
                                info!(provider = ?provider, "No snapshot, triggering refresh");
                                cx.update_global::<AppState, _>(|state, cx| {
                                    state.refresh_provider(provider, cx);
                                });
                            }

                            cx.notify(); // Re-render with new selection!
                        }),
                    );

                if is_selected {
                    btn = btn.bg(theme::accent()).text_color(gpui::white());
                } else {
                    btn = btn
                        .hover(|s| s.bg(theme::hover()))
                        .active(|s| s.bg(theme::active()));
                }

                btn.child(div().text_sm().child(name))
            }))
    }
}

impl Render for MenuPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        info!(tab = ?self.selected_tab, "🎨 MenuPanel::render() called!");

        let state = cx.global::<AppState>();
        let settings = state.settings.read(cx);
        let enabled = state.enabled_providers(cx);
        info!(
            enabled_count = enabled.len(),
            providers = ?enabled,
            merge_icons = settings.merge_icons(),
            "Menu state - enabled providers"
        );

        // Build the content based on selected tab
        let content = match self.selected_tab {
            SelectedTab::All => {
                // Render all provider cards in a vertical stack (scrolling handled by wrapper)
                let cards: Vec<_> = enabled
                    .iter()
                    .map(|&p| MenuCard::new(MenuCardData::new(p, cx)))
                    .collect();

                div()
                    .id("all-providers-content")
                    .flex()
                    .flex_col()
                    .children(cards.into_iter().map(|card| {
                        // Wrap each card with a subtle separator
                        div().border_b_1().border_color(theme::border()).child(card)
                    }))
                    .into_any_element()
            }
            SelectedTab::Provider(provider) => {
                // Single provider view (existing behavior)
                let card_data = MenuCardData::new(provider, cx);
                div().child(MenuCard::new(card_data)).into_any_element()
            }
        };

        let root = div()
            .id("menu-panel")
            .w(px(340.)) // Slightly wider like Notification Center
            // TRUE LIQUID GLASS: NO background at all! Window blur does everything.
            // NO BORDERS - true borderless liquid glass design
            .rounded(px(14.)) // Smooth rounded corners
            .overflow_hidden()
            // Deep shadow for floating glass effect
            .shadow_lg()
            // Flex layout for proper space distribution
            .flex()
            .flex_col()
            .max_h(px(600.)) // Max height for entire menu
            // Header (fixed height)
            .child(MenuHeader::new())
            // Provider switcher if multiple providers enabled - rendered here for cx.listener() access!
            .when(enabled.len() > 1, |el| {
                el.child(self.render_provider_switcher(&enabled, cx))
            })
            // Content area - grows to fill available space, scrolls when needed
            .child(
                div()
                    .id("content-scroll-area")
                    .flex_1() // Grow to fill available space
                    .min_h(px(100.)) // Minimum height
                    .overflow_y_scroll()
                    .child(content),
            )
            // Action footer with WORKING buttons (fixed height)
            .child(MenuFooter::new());

        // Apply opaque background on Linux (no blur support)
        #[cfg(target_os = "linux")]
        let root = root.bg(theme::window_background());

        root
    }
}

// ============================================================================
// Menu Header
// ============================================================================

struct MenuHeader;

impl MenuHeader {
    fn new() -> Self {
        Self
    }
}

impl IntoElement for MenuHeader {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .px(px(14.))
            .py(px(10.))
            // TRUE LIQUID GLASS: NO background - let window blur shine through!
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text_primary())
                            .child("ExactoBar"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(env!("CARGO_PKG_VERSION")),
                    ),
            )
    }
}

// ============================================================================
// Legacy TrayMenu Alias
// ============================================================================

/// Alias for backwards compatibility.
pub type TrayMenu = MenuPanel;
