// Lint configuration for this crate
#![allow(unexpected_cfgs)] // objc crate uses deprecated cfg syntax
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::wildcard_imports)] // gpui uses wildcard imports
#![allow(clippy::too_many_lines)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::fn_params_excessive_bools)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unused_self)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::non_std_lazy_statics)]
#![allow(missing_docs)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::unit_arg)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::async_yields_async)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::unnecessary_operation)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::nonminimal_bool)]
#![allow(unused_mut)]
#![allow(clippy::unused_async)]
#![allow(clippy::type_complexity)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_match_else)]
#![allow(clippy::let_and_return)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::manual_midpoint)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::format_push_string)]
#![allow(clippy::useless_format)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(clippy::ref_option)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::if_not_else)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::similar_names)]
#![allow(clippy::ref_as_ptr)]

//! ExactoBar - GPUI Menu Bar Application
//!
//! A macOS menu bar app for monitoring LLM provider usage.

pub mod actions;
pub mod components;
pub mod icon;
pub mod menu;
pub mod notifications;
pub mod refresh;
pub mod state;
pub mod theme;
pub mod tray;
pub mod updater;
pub mod windows;

use gpui::*;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

use crate::state::AppState;
use crate::tray::SystemTray;

/// Application entry point.
fn main() {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).ok();

    info!("ExactoBar starting...");

    // Run the GPUI application
    Application::new().run(|cx: &mut App| {
        // IMPORTANT: Tray apps must not quit when the popup window closes!
        // On Linux, the default is to quit when last window closes.
        cx.set_quit_mode(QuitMode::Explicit);

        // Register actions
        actions::register_actions(cx);

        // Initialize global state
        let state = AppState::init(cx);
        cx.set_global(state);

        // Initialize system tray
        let tray = SystemTray::new(cx);
        cx.set_global(tray);

        // Start the click listener for status item interactions
        cx.update_global::<SystemTray, _>(|tray, cx| {
            tray.start_click_listener(cx);
        });

        // Start the animation timer for eye blink and "surprise me" animations
        cx.update_global::<SystemTray, _>(|tray, cx| {
            tray.start_animation_timer(cx);
        });

        // Debug: write icon PNG to temp file for verification
        #[cfg(debug_assertions)]
        {
            let tray = cx.global::<SystemTray>();
            let state = cx.global::<AppState>();
            let providers = state.enabled_providers(cx);
            if let Some(provider) = providers.first() {
                if let Some(png) = tray.get_icon_png(*provider, cx) {
                    if std::fs::write("/tmp/exactobar-icon.png", &png).is_ok() {
                        info!(provider = ?provider, "Wrote debug icon to /tmp/exactobar-icon.png ({} bytes)", png.len());
                    }
                }
            }
        }

        // Start background refresh task
        refresh::spawn_refresh_task(cx);

        // Check for updates after a short delay (don't block startup)
        spawn_update_check(cx);

        // Check for onboarding - open settings if no providers
        if should_show_onboarding(cx) {
            windows::open_settings(cx);
        }

        info!("ExactoBar initialized");
    });
}

/// Checks if we should show onboarding (first run or no providers).
fn should_show_onboarding(cx: &App) -> bool {
    let state = cx.global::<AppState>();
    state.enabled_providers(cx).is_empty()
}

/// Spawns a background task to check for updates after a delay.
///
/// This runs 5 seconds after startup to avoid blocking the initial load.
fn spawn_update_check(cx: &mut App) {
    cx.spawn(async move |mut cx| {
        // Wait 5 seconds before checking for updates
        smol::Timer::after(std::time::Duration::from_secs(5)).await;

        info!("Starting background update check...");

        let result = crate::updater::check_for_updates().await;

        if let crate::updater::UpdateCheckResult::UpdateAvailable {
            ref current,
            ref latest,
            ..
        } = result
        {
            // Show system notification about the update
            crate::updater::show_update_notification(current, latest);

            // Show the update dialog
            let _ = cx.update(|cx| {
                crate::windows::show_update_dialog(&result, cx);
            });
        }
    })
    .detach();
}
