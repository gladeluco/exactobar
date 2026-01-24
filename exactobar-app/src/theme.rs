//! ExactoBar theme definitions.
//!
//! Provides color constants and utilities for the menu UI.
//! Extends GPUI's default theme with provider-specific colors.

#![allow(dead_code)]

use exactobar_core::ProviderKind;
use gpui::*;
use std::collections::HashMap;

// ============================================================================
// Theme Mode
// ============================================================================

use exactobar_store::ThemeMode;
use gpui::WindowAppearance;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Gets the current theme based on mode and system appearance.
pub fn current_theme(mode: ThemeMode, appearance: WindowAppearance) -> ExactoBarTheme {
    match mode {
        ThemeMode::Dark => ExactoBarTheme::dark(),
        ThemeMode::Light => ExactoBarTheme::light(),
        ThemeMode::System => {
            // Check system appearance
            let is_dark = matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            );
            if is_dark {
                ExactoBarTheme::dark()
            } else {
                ExactoBarTheme::light()
            }
        }
    }
}

static CURRENT_DARK_MODE: OnceLock<AtomicBool> = OnceLock::new();

fn current_dark_mode() -> bool {
    CURRENT_DARK_MODE
        .get_or_init(|| AtomicBool::new(true))
        .load(Ordering::Relaxed)
}

pub fn set_current_theme_mode(mode: ThemeMode, appearance: WindowAppearance) {
    let is_dark = match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    CURRENT_DARK_MODE
        .get_or_init(|| AtomicBool::new(true))
        .store(is_dark, Ordering::Relaxed);
}

// ============================================================================
// Dark Mode Colors
// ============================================================================

/// Surface/background color for menu panels (dark mode).
pub fn surface_background_dark() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.08) // Dark surface with slight opacity
}

/// Liquid glass panel tint (dark mode).
pub fn liquid_glass_tint_dark() -> Hsla {
    hsla(0.0, 0.0, 0.05, 0.75) // Subtle dark tint
}

/// Primary text color (dark mode).
pub fn text_primary_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.95) // White
}

/// Secondary text color (dark mode).
pub fn text_secondary_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.75) // 75% white for better contrast
}

/// Border color for dividers and outlines (dark mode).
pub fn border_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.1) // Subtle light border
}

/// Muted text color for secondary information (dark mode).
pub fn muted_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.6) // 60% white
}

/// Hover state background color (dark mode).
pub fn hover_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.08) // Subtle white highlight
}

/// Active/pressed state background (dark mode).
pub fn active_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.15) // Stronger white highlight
}

/// Track color for progress bars (dark mode).
pub fn track_dark() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.15) // Subtle white track
}

/// Surface color for buttons/controls (dark mode).
pub fn surface_dark() -> Hsla {
    hsla(0.0, 0.0, 0.15, 0.5) // Semi-transparent dark
}

// ============================================================================
// Light Mode Colors
// ============================================================================

/// Surface/background color for menu panels (light mode).
pub fn surface_background_light() -> Hsla {
    hsla(0.0, 0.0, 0.98, 1.0) // Opaque light background to avoid over-blur
}

/// Text color (light mode).
pub fn text_primary_light() -> Hsla {
    hsla(0.0, 0.0, 0.05, 0.98) // Darker text for better readability
}

/// Secondary text color (light mode).
pub fn text_secondary_light() -> Hsla {
    hsla(0.0, 0.0, 0.25, 0.85) // Darker grey
}

/// Border color (light mode).
pub fn border_light() -> Hsla {
    hsla(0.0, 0.0, 0.85, 0.4) // More visible border
}

/// Muted text color (light mode).
pub fn muted_light() -> Hsla {
    hsla(0.0, 0.0, 0.4, 0.7) // Medium grey
}

/// Hover state (light mode).
pub fn hover_light() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.05) // Subtle dark highlight
}

/// Active state (light mode).
pub fn active_light() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.1) // Stronger dark highlight
}

/// Track color (light mode).
pub fn track_light() -> Hsla {
    hsla(0.0, 0.0, 0.7, 0.2) // Subtle grey track
}

/// Surface color (light mode).
pub fn surface_light() -> Hsla {
    hsla(0.0, 0.0, 0.95, 0.95) // Light grey surface with higher opacity
}

// ============================================================================
// Backwards Compatibility - Default to Dark Mode
// ============================================================================

/// Surface/background color for menu panels (deprecated: use theme-specific versions).
pub fn surface_background() -> Hsla {
    if current_dark_mode() {
        surface_background_dark()
    } else {
        surface_background_light()
    }
}

/// Liquid glass panel tint - ultra-subtle dark tint for definition.
pub fn liquid_glass_tint() -> Hsla {
    if current_dark_mode() {
        liquid_glass_tint_dark()
    } else {
        hsla(0.0, 0.0, 0.98, 0.9)
    }
}

/// Primary text color - white for dark mode.
pub fn text_primary() -> Hsla {
    if current_dark_mode() {
        text_primary_dark()
    } else {
        text_primary_light()
    }
}

/// Secondary text color - muted white for dark mode.
pub fn text_secondary() -> Hsla {
    if current_dark_mode() {
        text_secondary_dark()
    } else {
        text_secondary_light()
    }
}

/// Border color for dividers and outlines - subtle white glow.
pub fn border() -> Hsla {
    if current_dark_mode() {
        border_dark()
    } else {
        border_light()
    }
}

/// Liquid glass separator - ultra-subtle divider instead of hard borders.
pub fn glass_separator() -> Hsla {
    if current_dark_mode() {
        hsla(0.0, 0.0, 1.0, 0.04)
    } else {
        hsla(0.0, 0.0, 0.0, 0.06)
    }
}

/// Muted text color for secondary information.
pub fn muted() -> Hsla {
    if current_dark_mode() {
        muted_dark()
    } else {
        muted_light()
    }
}

/// Hover state background color - subtle white highlight.
pub fn hover() -> Hsla {
    if current_dark_mode() {
        hover_dark()
    } else {
        hover_light()
    }
}

/// Active/pressed state background.
pub fn active() -> Hsla {
    if current_dark_mode() {
        active_dark()
    } else {
        active_light()
    }
}

/// Accent color for selected/active states (macOS blue).
pub fn accent() -> Hsla {
    hsla(211.0 / 360.0, 1.0, 0.5, 1.0)
}

/// Success color (good usage levels).
pub fn success() -> Hsla {
    hsla(142.0 / 360.0, 0.71, 0.45, 1.0) // Green
}

/// Warning color (approaching limits).
pub fn warning() -> Hsla {
    hsla(38.0 / 360.0, 0.92, 0.50, 1.0) // Orange/Yellow
}

/// Error color (exceeded limits or errors).
pub fn error() -> Hsla {
    hsla(0.0, 0.72, 0.51, 1.0) // Red
}

/// Surface color for buttons/controls - semi-transparent dark.
pub fn surface() -> Hsla {
    if current_dark_mode() {
        surface_dark()
    } else {
        surface_light()
    }
}

/// Track color for progress bars - subtle on dark background.
pub fn track() -> Hsla {
    if current_dark_mode() {
        track_dark()
    } else {
        track_light()
    }
}

/// Card background - for notification-style cards in dark mode.
pub fn card_background() -> Hsla {
    if current_dark_mode() {
        hsla(0.0, 0.0, 1.0, 0.08)
    } else {
        hsla(0.0, 0.0, 1.0, 0.9)
    }
}

/// Opaque window background for platforms without blur support (Linux).
pub fn window_background() -> Hsla {
    hsla(0.0, 0.0, 0.12, 0.98)
}

/// Liquid glass card background - even MORE subtle for true glass effect.
pub fn liquid_card_background() -> Hsla {
    if current_dark_mode() {
        hsla(0.0, 0.0, 1.0, 0.05)
    } else {
        hsla(0.0, 0.0, 1.0, 0.95)
    }
}

/// Returns the appropriate color for a usage percentage (USED, not remaining).
/// Green = low usage (good), Red = high usage (warning)
/// Smooth gradient: Green (0%) → Yellow (50%) → Orange (80%) → Red (100%)
pub fn color_for_usage(used_percent: f64) -> Hsla {
    let used = used_percent as f32;
    if used < 50.0 {
        // Green to Yellow (0-50%)
        let t = used / 50.0;
        hsla(
            (120.0 - t * 60.0) / 360.0, // Hue: 120 (green) → 60 (yellow)
            0.7,
            0.45,
            1.0,
        )
    } else if used < 80.0 {
        // Yellow to Orange (50-80%)
        let t = (used - 50.0) / 30.0;
        hsla(
            (60.0 - t * 30.0) / 360.0, // Hue: 60 (yellow) → 30 (orange)
            0.8,
            0.5,
            1.0,
        )
    } else {
        // Orange to Red (80-100%)
        let t = (used - 80.0) / 20.0;
        hsla(
            (30.0 - t * 30.0) / 360.0, // Hue: 30 (orange) → 0 (red)
            0.85,
            0.5,
            1.0,
        )
    }
}

/// Deprecated: Use color_for_usage() instead.
/// Kept for backwards compatibility.
#[deprecated(note = "Use color_for_usage() instead - inverted to show used percentage")]
pub fn color_for_percent(percent_remaining: f64) -> Hsla {
    // Convert remaining to used and use the new function
    color_for_usage(100.0 - percent_remaining)
}

// ============================================================================
// ExactoBar Theme
// ============================================================================

/// ExactoBar theme with provider colors.
pub struct ExactoBarTheme {
    /// Provider brand colors.
    pub provider_colors: HashMap<ProviderKind, Hsla>,
    /// Whether dark mode is active.
    pub dark_mode: bool,
}

impl ExactoBarTheme {
    /// Creates a light theme.
    pub fn light() -> Self {
        Self {
            provider_colors: provider_colors(),
            dark_mode: false,
        }
    }

    /// Creates a dark theme.
    pub fn dark() -> Self {
        Self {
            provider_colors: provider_colors(),
            dark_mode: true,
        }
    }

    /// Gets the brand color for a provider.
    pub fn provider_color(&self, provider: ProviderKind) -> Hsla {
        self.provider_colors
            .get(&provider)
            .copied()
            .unwrap_or(hsla(0.0, 0.0, 0.5, 1.0))
    }

    /// Gets the usage bar colors.
    pub fn usage_colors(&self) -> UsageColors {
        if self.dark_mode {
            UsageColors {
                good: hsla(142.0 / 360.0, 0.71, 0.45, 1.0),   // Green
                warning: hsla(38.0 / 360.0, 0.92, 0.50, 1.0), // Yellow
                danger: hsla(0.0, 0.84, 0.60, 1.0),           // Red
                background: hsla(0.0, 0.0, 0.25, 1.0),        // Dark gray
            }
        } else {
            UsageColors {
                good: hsla(142.0 / 360.0, 0.71, 0.45, 1.0),   // Green
                warning: hsla(38.0 / 360.0, 0.92, 0.50, 1.0), // Orange
                danger: hsla(0.0, 0.84, 0.50, 1.0),           // Red
                background: hsla(0.0, 0.0, 0.90, 1.0),        // Light gray
            }
        }
    }
}

/// Colors for usage bars.
pub struct UsageColors {
    pub good: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub background: Hsla,
}

impl UsageColors {
    /// Gets the color for a given USAGE percentage (not remaining!).
    /// Green = low usage (good), Red = high usage (warning)
    pub fn for_usage(&self, used_percent: f32) -> Hsla {
        if used_percent < 50.0 {
            self.good
        } else if used_percent < 80.0 {
            self.warning
        } else {
            self.danger
        }
    }

    /// Deprecated: Use for_usage() instead.
    #[deprecated(note = "Use for_usage() instead - inverted to show used percentage")]
    pub fn for_percent(&self, percent_remaining: f32) -> Hsla {
        self.for_usage(100.0 - percent_remaining)
    }
}

// ============================================================================
// Provider Colors
// ============================================================================

/// Provider brand colors.
fn provider_colors() -> HashMap<ProviderKind, Hsla> {
    let mut map = HashMap::new();

    // OpenAI / Codex - Green
    map.insert(ProviderKind::Codex, hsla(160.0 / 360.0, 0.82, 0.35, 1.0));

    // Anthropic / Claude - Orange/Tan
    map.insert(ProviderKind::Claude, hsla(25.0 / 360.0, 0.55, 0.53, 1.0));

    // Cursor - Purple
    map.insert(ProviderKind::Cursor, hsla(265.0 / 360.0, 0.70, 0.60, 1.0));

    // Gemini - Google Blue
    map.insert(ProviderKind::Gemini, hsla(217.0 / 360.0, 0.91, 0.60, 1.0));

    // Copilot - GitHub Dark
    map.insert(ProviderKind::Copilot, hsla(215.0 / 360.0, 0.14, 0.34, 1.0));

    // Factory/Droid - Red
    map.insert(ProviderKind::Factory, hsla(0.0, 0.70, 0.60, 1.0));

    // Vertex AI - Google Blue
    map.insert(ProviderKind::VertexAI, hsla(217.0 / 360.0, 0.91, 0.60, 1.0));

    // z.ai - Gray
    map.insert(ProviderKind::Zai, hsla(0.0, 0.0, 0.40, 1.0));

    // Augment - Indigo
    map.insert(ProviderKind::Augment, hsla(275.0 / 360.0, 1.0, 0.25, 1.0));

    // Kiro - Orange
    map.insert(ProviderKind::Kiro, hsla(39.0 / 360.0, 1.0, 0.50, 1.0));

    // MiniMax - Sky Blue
    map.insert(ProviderKind::MiniMax, hsla(195.0 / 360.0, 1.0, 0.50, 1.0));

    // Antigravity - Violet
    map.insert(
        ProviderKind::Antigravity,
        hsla(282.0 / 360.0, 1.0, 0.41, 1.0),
    );

    map
}

// ============================================================================
// Color Utilities
// ============================================================================

/// Lightens a color by the given amount.
pub fn lighten(color: Hsla, amount: f32) -> Hsla {
    hsla(color.h, color.s, (color.l + amount).min(1.0), color.a)
}

/// Darkens a color by the given amount.
pub fn darken(color: Hsla, amount: f32) -> Hsla {
    hsla(color.h, color.s, (color.l - amount).max(0.0), color.a)
}

/// Creates a transparent version of a color.
pub fn transparent(color: Hsla, alpha: f32) -> Hsla {
    hsla(color.h, color.s, color.l, alpha)
}

// ============================================================================
// Tests are in a separate integration test file to avoid binary recursion issues
// See tests/theme_tests.rs for comprehensive theme system tests
// ============================================================================
