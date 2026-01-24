//! Synthetic.new API client.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, instrument};

use super::error::SyntheticError;

// ============================================================================
// Constants
// ============================================================================

/// Synthetic.new API base URL.
pub const API_BASE_URL: &str = "https://api.synthetic.new";

/// Quota endpoint.
pub const QUOTA_ENDPOINT: &str = "/v2/quotas";

// ============================================================================
// API Response Types
// ============================================================================

/// Response from Synthetic.new quota API.
#[derive(Debug, Clone, Deserialize)]
pub struct SyntheticQuotaResponse {
    /// Subscription info (optional for flexibility).
    pub subscription: Option<SubscriptionInfo>,
    // Search info for Pro tier accounts.
    pub search: Option<SearchInfo>,
}

/// Subscription details from the API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    /// Request limit per period.
    pub limit: i64,

    /// Requests used in current period. (measured in partial requests)
    pub requests: f64,

    /// When the quota resets (ISO 8601 format).
    #[serde(rename = "renewsAt")]
    pub renews_at: Option<String>,
}

/// Search details from the API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInfo {
    pub hourly: SubscriptionInfo,
}

impl SyntheticQuotaResponse {
    /// Convert to UsageSnapshot.
    pub fn to_snapshot(&self) -> exactobar_core::UsageSnapshot {
        use exactobar_core::{FetchSource, LoginMethod, ProviderIdentity, ProviderKind};

        let mut snapshot = exactobar_core::UsageSnapshot::new();
        snapshot.fetch_source = FetchSource::Api;

        if let Some(ref sub) = self.subscription {
            // Calculate usage percentage
            let used_percent = if sub.limit > 0 {
                (sub.requests / sub.limit as f64) * 100.0
            } else {
                0.0
            };

            // Parse renewal time
            let resets_at = sub.renews_at.as_ref().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            });

            snapshot.primary = Some(exactobar_core::UsageWindow {
                used_percent,
                window_minutes: Some(43200), // ~30 days in minutes
                resets_at,
                reset_description: None,
            });

            // Add identity with plan info
            let mut identity = ProviderIdentity::new(ProviderKind::Synthetic);
            identity.plan_name = Some(format!("{} requests/period", sub.limit));
            identity.login_method = Some(LoginMethod::ApiKey);
            snapshot.identity = Some(identity);
        }

        snapshot
    }
}

// ============================================================================
// API Client
// ============================================================================

/// Synthetic.new API client.
#[derive(Debug, Clone)]
pub struct SyntheticApiClient {
    base_url: String,
}

impl Default for SyntheticApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntheticApiClient {
    /// Creates a new client.
    pub fn new() -> Self {
        Self {
            base_url: API_BASE_URL.to_string(),
        }
    }

    /// Get API key from Keychain first, then environment variable.
    ///
    /// The lookup order is:
    /// 1. System keychain (stored via Settings UI)
    /// 2. Environment variable `SYNTHETIC_API_KEY`
    pub fn get_api_key() -> Result<String, SyntheticError> {
        // Try Keychain first
        if let Some(key) = exactobar_store::get_api_key("synthetic") {
            return Ok(key);
        }

        // Fall back to environment variable
        std::env::var("SYNTHETIC_API_KEY").map_err(|_| SyntheticError::ApiKeyNotFound)
    }

    /// Fetch quota from the API.
    #[instrument(skip(self, api_key))]
    pub async fn fetch_quota(
        &self,
        api_key: &str,
    ) -> Result<SyntheticQuotaResponse, SyntheticError> {
        let url = format!("{}{}", self.base_url, QUOTA_ENDPOINT);

        debug!(url = %url, "Fetching Synthetic.new quota");

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| SyntheticError::HttpError(e.to_string()))?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyntheticError::AuthenticationFailed(
                "API key rejected".to_string(),
            ));
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(SyntheticError::ApiError(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SyntheticError::ParseError(e.to_string()))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = SyntheticApiClient::new();
        assert_eq!(client.base_url, API_BASE_URL);
    }

    #[test]
    fn test_parse_quota_response() {
        let json = r#"{
          "subscription": {
            "limit": 135,
            "requests": 50.0,
            "renewsAt": "2026-01-16T19:52:56.048Z"
          },
          "search": {
            "hourly": {
              "limit": 250,
              "requests": 0,
              "renewsAt": "2026-01-16T17:17:14.049Z"
            }
          }
        }"#;

        let response: SyntheticQuotaResponse = serde_json::from_str(json).unwrap();
        let sub = response.subscription.unwrap();
        assert_eq!(sub.limit, 135);
        assert_eq!(sub.requests, 50.0);
        assert!(sub.renews_at.is_some());
    }

    #[test]
    fn test_to_snapshot() {
        let response = SyntheticQuotaResponse {
            subscription: Some(SubscriptionInfo {
                limit: 100,
                requests: 50.0,
                renews_at: Some("2025-09-21T14:36:14.288Z".to_string()),
            }),

            search: Some(SearchInfo {
                hourly: SubscriptionInfo {
                    limit: 100,
                    requests: 50.0,
                    renews_at: Some("2025-09-21T14:36:14.288Z".to_string()),
                },
            }),
        };

        let snapshot = response.to_snapshot();
        assert!(snapshot.primary.is_some());
        let primary = snapshot.primary.unwrap();
        assert_eq!(primary.used_percent, 50.0);
        assert!(primary.resets_at.is_some());
    }

    #[test]
    fn test_to_snapshot_zero_limit() {
        let response = SyntheticQuotaResponse {
            subscription: Some(SubscriptionInfo {
                limit: 0,
                requests: 0.0,
                renews_at: None,
            }),

            search: Some(SearchInfo {
                hourly: SubscriptionInfo {
                    limit: 100,
                    requests: 50.0,
                    renews_at: Some("2025-09-21T14:36:14.288Z".to_string()),
                },
            }),
        };

        let snapshot = response.to_snapshot();
        assert!(snapshot.primary.is_some());
        let primary = snapshot.primary.unwrap();
        assert_eq!(primary.used_percent, 0.0);
    }
}
