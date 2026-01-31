//! Menu card components for displaying provider information.
//!
//! The MenuCard shows provider identity, status, usage metrics,
//! and action buttons in a cohesive card layout.

use exactobar_core::{ProviderKind, UsageSnapshot};
use exactobar_providers::ProviderRegistry;
use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::components::{ProviderIcon, Spinner};
use crate::state::AppState;
use crate::theme;

use super::actions::ActionButtonsSection;
use super::error::{EnhancedErrorSection, InstallHint, get_install_hint};
use super::usage::UsageMetricsSection;

// ============================================================================
// Menu Card Data
// ============================================================================

pub struct MenuCardData {
    pub provider: ProviderKind,
    pub provider_name: String,
    pub email: String,
    pub plan: Option<String>,
    pub snapshot: Option<UsageSnapshot>,
    pub is_refreshing: bool,
    pub error: Option<String>,
    /// Install hint when CLI is missing
    pub install_hint: Option<InstallHint>,
    pub session_label: &'static str,
    pub weekly_label: &'static str,
    /// Whether to show "X% used" instead of "X% remaining"
    pub show_used: bool,
    /// Whether to show "Resets at 3:00 PM" instead of "Resets in 2h 30m"
    pub show_absolute: bool,
}

impl MenuCardData {
    pub fn new<V: 'static>(provider: ProviderKind, cx: &Context<V>) -> Self {
        let state = cx.global::<AppState>();
        let snapshot = state.get_snapshot(provider, cx);
        let is_refreshing = state.is_provider_refreshing(provider, cx);
        let error = state.get_error(provider, cx);
        let descriptor = ProviderRegistry::get(provider);

        // Read display settings
        let settings = state.settings.read(cx).settings();
        let show_used = settings.usage_bars_show_used;
        let show_absolute = settings.reset_times_show_absolute;

        let provider_name = descriptor
            .map(|d| d.display_name().to_string())
            .unwrap_or_else(|| format!("{:?}", provider));

        let session_label = descriptor
            .map(|d| d.metadata.session_label.as_str())
            .unwrap_or("Session");

        let weekly_label = descriptor
            .map(|d| d.metadata.weekly_label.as_str())
            .unwrap_or("Weekly");

        // Extract identity info from snapshot
        let identity = snapshot.as_ref().and_then(|s| s.identity.as_ref());
        let email = identity
            .and_then(|i| i.account_email.as_deref())
            .unwrap_or("")
            .to_string();
        let plan = identity.and_then(|i| i.plan_name.clone());

        // Detect install hints for missing CLIs
        let install_hint = error.as_ref().and_then(|e| get_install_hint(provider, e));

        Self {
            provider,
            provider_name,
            email,
            plan,
            snapshot,
            is_refreshing,
            error,
            install_hint,
            session_label,
            weekly_label,
            show_used,
            show_absolute,
        }
    }
}

// ============================================================================
// Menu Card
// ============================================================================

pub struct MenuCard {
    data: MenuCardData,
}

impl MenuCard {
    pub fn new(data: MenuCardData) -> Self {
        Self { data }
    }
}

impl IntoElement for MenuCard {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let provider = self.data.provider;
        tracing::info!(
            provider = ?provider,
            has_snapshot = self.data.snapshot.is_some(),
            has_error = self.data.error.is_some(),
            is_refreshing = self.data.is_refreshing,
            "MenuCard rendering"
        );
        let mut card = div().flex().flex_col();

        // Header section
        card = card.child(CardHeader {
            provider,
            provider_name: self.data.provider_name.clone(),
            email: self.data.email.clone(),
            plan: self.data.plan.clone(),
            is_refreshing: self.data.is_refreshing,
            has_error: self.data.error.is_some(),
        });

        // Error display with install hints
        if let Some(ref err) = self.data.error {
            card = card.child(EnhancedErrorSection {
                summary: err.clone(),
                details: None,
                install_hint: self.data.install_hint.clone(),
            });
        } else if let Some(ref snap) = self.data.snapshot {
            // Usage metrics
            card = card.child(UsageMetricsSection::new(
                snap,
                self.data.session_label,
                self.data.weekly_label,
                Some("Search"),
                self.data.show_used,
                self.data.show_absolute,
            ));
        } else if !self.data.is_refreshing {
            card = card.child(PlaceholderSection);
        }

        // Action buttons section (Dashboard, Status, Buy Credits)
        card = card.child(ActionButtonsSection::new(provider));

        card
    }
}

// ============================================================================
// Card Header
// ============================================================================

struct CardHeader {
    provider: ProviderKind,
    provider_name: String,
    email: String,
    plan: Option<String>,
    is_refreshing: bool,
    has_error: bool,
}

impl IntoElement for CardHeader {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let status_text = if self.is_refreshing {
            "Refreshing...".to_string()
        } else if self.has_error {
            "Error".to_string()
        } else {
            "Updated just now".to_string()
        };

        let status_color = if self.has_error {
            theme::error()
        } else {
            theme::muted()
        };

        // Build top row with optional email
        let mut top_row = div().flex().items_center().justify_between().child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(ProviderIcon::new(self.provider).size(px(18.)))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text_primary())
                        .child(self.provider_name),
                ),
        );

        if !self.email.is_empty() {
            top_row = top_row.child(div().text_xs().text_color(theme::muted()).child(self.email));
        }

        // Build status row with optional spinner
        let mut status_row = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().text_xs().text_color(status_color).child(status_text));

        if self.is_refreshing {
            status_row = status_row.child(Spinner::new());
        }

        // Build bottom row with optional plan
        let mut bottom_row = div()
            .flex()
            .items_center()
            .justify_between()
            .child(status_row);

        if let Some(plan) = self.plan {
            bottom_row = bottom_row.child(div().text_xs().text_color(theme::muted()).child(plan));
        }

        div()
            .px(px(14.))
            .py(px(10.))
            .bg(theme::card_background())
            .border_b_1()
            .border_color(theme::glass_separator())
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(top_row)
            .child(bottom_row)
    }
}

// ============================================================================
// Placeholder Section
// ============================================================================

struct PlaceholderSection;

impl IntoElement for PlaceholderSection {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .px(px(14.))
            .py(px(10.))
            .bg(theme::card_background())
            .border_b_1()
            .border_color(theme::glass_separator())
            .child(
                div()
                    .text_sm()
                    .text_color(theme::muted())
                    .child("No data yet"),
            )
    }
}
