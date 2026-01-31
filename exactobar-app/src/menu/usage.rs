//! Usage metrics display components.
//!
//! Provides progress bars and usage metric rows for displaying
//! session, weekly, and premium usage limits.

use chrono::{DateTime, Local, Utc};
use exactobar_core::UsageSnapshot;
use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme;

// ============================================================================
// Usage Metrics Section
// ============================================================================

pub struct UsageMetricsSection {
    metrics: Vec<UsageMetric>,
}

struct UsageMetric {
    title: String,
    used_percent: f64,
    resets_at: Option<DateTime<Utc>>,
    reset_description: Option<String>,
    /// When true, show "X% used" instead of "X% remaining"
    show_used: bool,
    /// When true, show "Resets at 3:00 PM" instead of "Resets in 2h 30m"
    show_absolute: bool,
}

impl UsageMetricsSection {
    pub fn new(
        snapshot: &UsageSnapshot,
        session_label: &str,
        weekly_label: &str,
        search_label: Option<&str>,
        show_used: bool,
        show_absolute: bool,
    ) -> Self {
        let mut metrics = Vec::new();

        if let Some(primary) = &snapshot.primary {
            metrics.push(UsageMetric {
                title: session_label.to_string(),
                used_percent: primary.used_percent,
                resets_at: primary.resets_at,
                reset_description: primary.reset_description.clone(),
                show_used,
                show_absolute,
            });
        }

        if let Some(secondary) = &snapshot.secondary {
            metrics.push(UsageMetric {
                title: weekly_label.to_string(),
                used_percent: secondary.used_percent,
                resets_at: secondary.resets_at,
                reset_description: secondary.reset_description.clone(),
                show_used,
                show_absolute,
            });
        }

        if let Some(tertiary) = &snapshot.tertiary {
            metrics.push(UsageMetric {
                title: "Premium".to_string(),
                used_percent: tertiary.used_percent,
                resets_at: tertiary.resets_at,
                reset_description: tertiary.reset_description.clone(),
                show_used,
                show_absolute,
            });
        }

        if let Some(search) = &snapshot.search {
            metrics.push(UsageMetric {
                title: search_label.unwrap_or("Search").to_string(),
                used_percent: search.used_percent,
                resets_at: search.resets_at,
                reset_description: search.reset_description.clone(),
                show_used,
                show_absolute,
            });
        }

        Self { metrics }
    }
}

impl IntoElement for UsageMetricsSection {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        if self.metrics.is_empty() {
            return div();
        }

        div()
            .px(px(14.))
            .py(px(10.))
            .bg(theme::card_background())
            .border_b_1()
            .border_color(theme::glass_separator())
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(self.metrics.into_iter().map(UsageMetricRow::new))
    }
}

// ============================================================================
// Usage Metric Row
// ============================================================================

struct UsageMetricRow {
    metric: UsageMetric,
}

impl UsageMetricRow {
    fn new(metric: UsageMetric) -> Self {
        Self { metric }
    }

    /// Format reset time based on settings.
    /// Returns "Resets at 3:00 PM" or "Resets in 2h 30m" depending on `show_absolute`.
    fn format_reset_time(&self) -> Option<String> {
        if self.metric.show_absolute {
            // Absolute time format: "Resets at 3:00 PM"
            self.metric.resets_at.map(|reset_at| {
                let local_time: DateTime<Local> = reset_at.into();
                format!(
                    "Resets at {}",
                    local_time.format("%l:%M %p").to_string().trim()
                )
            })
        } else {
            // Relative time format: "Resets in 2h 30m" or use provider's description
            if let Some(reset_at) = self.metric.resets_at {
                let now = Utc::now();
                if reset_at > now {
                    let duration = reset_at - now;
                    let total_minutes = duration.num_minutes();
                    let hours = total_minutes / 60;
                    let minutes = total_minutes % 60;

                    let time_str = if hours > 0 {
                        format!("{}h {}m", hours, minutes)
                    } else {
                        format!("{}m", minutes)
                    };
                    Some(format!("Resets in {}", time_str))
                } else {
                    Some("Resets soon".to_string())
                }
            } else {
                // Fall back to provider's description if no timestamp
                self.metric
                    .reset_description
                    .as_ref()
                    .map(|d| format!("Resets {}", d))
            }
        }
    }
}

impl IntoElement for UsageMetricRow {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let used_percent = self.metric.used_percent.clamp(0.0, 100.0);

        // Label always shows "X% used" - it's more intuitive!
        let percent_label = format!("{:.0}% used", used_percent);

        // Color based on USAGE: green (low) → yellow → orange → red (high)
        let color = usage_color(used_percent);

        // Progress bar fill = used percentage (fills left to right as usage increases)
        let bar_fill_percent = used_percent;

        // Format reset time based on settings
        let reset_text = self.format_reset_time();

        // Build footer row with optional reset text
        let mut footer_row = div().flex().items_center().justify_between().child(
            div()
                .text_xs()
                .text_color(theme::text_secondary())
                .child(percent_label),
        );

        if let Some(text) = reset_text {
            footer_row = footer_row.child(div().text_xs().text_color(theme::muted()).child(text));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            // Title
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::text_primary())
                    .child(self.metric.title),
            )
            // Capsule-shaped progress bar
            .child(ProgressBar::new(bar_fill_percent, color))
            // Footer
            .child(footer_row)
    }
}

// ============================================================================
// Progress Bar (Capsule Style like CodexBar)
// ============================================================================

struct ProgressBar {
    percent: f64,
    color: Hsla,
}

impl ProgressBar {
    fn new(percent: f64, color: Hsla) -> Self {
        Self {
            percent: percent.clamp(0.0, 100.0),
            color,
        }
    }
}

impl IntoElement for ProgressBar {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let fraction = (self.percent / 100.0) as f32;

        // Capsule-shaped progress bar: 6px height, fully rounded ends (radius = height/2)
        div()
            .h(px(6.))
            .w_full()
            .bg(theme::track())
            .rounded(px(3.)) // Full capsule shape
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .bg(self.color)
                    .rounded(px(3.)), // Match container rounding
            )
    }
}

// ============================================================================
// Color Utilities
// ============================================================================

/// Returns a color based on usage percentage (USED, not remaining).
/// Smooth gradient: Green (0%) → Yellow (50%) → Orange (80%) → Red (100%)
///
/// This makes intuitive sense: low usage = green (good), high usage = red (warning)
fn usage_color(used_percent: f64) -> Hsla {
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
