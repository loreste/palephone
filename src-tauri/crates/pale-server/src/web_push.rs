//! Web Push notifications via the VAPID protocol.
//!
//! Instead of pulling in a heavy `web-push` crate, this module implements the
//! Web Push protocol directly: VAPID JWT signing with ES256 (P-256) and the
//! actual POST to the push endpoint using `reqwest`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// VAPID configuration parsed from environment variables.
#[derive(Debug, Clone)]
pub struct VapidConfig {
    pub public_key: String,
    pub private_key: String,
    pub subject: String,
}

impl VapidConfig {
    /// Returns `None` when `PALE_VAPID_PUBLIC_KEY` is not set.
    pub fn from_env() -> Option<Self> {
        let public_key = std::env::var("PALE_VAPID_PUBLIC_KEY")
            .ok()
            .filter(|v| !v.is_empty())?;
        let private_key = std::env::var("PALE_VAPID_PRIVATE_KEY")
            .ok()
            .filter(|v| !v.is_empty())?;
        let subject = std::env::var("PALE_VAPID_SUBJECT")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "mailto:admin@pale.local".to_string());
        Some(Self {
            public_key,
            private_key,
            subject,
        })
    }
}

/// A push subscription registered by a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscription {
    pub id: Uuid,
    pub user_uri: String,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub created_at: DateTime<Utc>,
}

/// The payload POSTed by clients when subscribing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscribeRequest {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// The payload POSTed by clients when unsubscribing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushUnsubscribeRequest {
    pub endpoint: String,
}

/// A push notification payload.
#[derive(Debug, Clone, Serialize)]
pub struct PushPayload {
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Build a VAPID Authorization header for the given push endpoint.
fn build_vapid_header(config: &VapidConfig, audience: &str) -> Result<String, PushError> {
    let header = serde_json::json!({"typ": "JWT", "alg": "ES256"});
    let now = Utc::now().timestamp();
    let claims = serde_json::json!({
        "aud": audience,
        "exp": now + 12 * 3600,
        "sub": config.subject,
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    let signing_input = format!("{}.{}", header_b64, claims_b64);

    let key_bytes = URL_SAFE_NO_PAD
        .decode(&config.private_key)
        .map_err(|e| PushError::Config(format!("invalid VAPID private key: {e}")))?;
    let signing_key = SigningKey::from_bytes(key_bytes.as_slice().into())
        .map_err(|e| PushError::Config(format!("invalid P-256 key: {e}")))?;
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    let jwt = format!("{}.{}", signing_input, sig_b64);
    Ok(format!("vapid t={},k={}", jwt, config.public_key))
}

/// Extract the origin (scheme + host) from a push endpoint URL.
fn audience_from_endpoint(endpoint: &str) -> Result<String, PushError> {
    let url = url::Url::parse(endpoint)
        .map_err(|e| PushError::Config(format!("invalid endpoint URL: {e}")))?;
    Ok(format!(
        "{}://{}",
        url.scheme(),
        url.host_str().unwrap_or("localhost")
    ))
}

/// Send a push notification to a single subscription.
pub async fn send_push_notification(
    http_client: &reqwest::Client,
    config: &VapidConfig,
    subscription: &PushSubscription,
    payload: &PushPayload,
) -> Result<(), PushError> {
    let audience = audience_from_endpoint(&subscription.endpoint)?;
    let auth_header = build_vapid_header(config, &audience)?;
    let body = serde_json::to_vec(payload)
        .map_err(|e| PushError::Internal(format!("serialize payload: {e}")))?;

    let response = http_client
        .post(&subscription.endpoint)
        .header("Authorization", &auth_header)
        .header("Content-Type", "application/json")
        .header("TTL", "86400")
        .body(body)
        .send()
        .await
        .map_err(|e| PushError::Network(format!("POST to push endpoint failed: {e}")))?;

    let status = response.status();
    if status.is_success() || status.as_u16() == 201 {
        Ok(())
    } else if status.as_u16() == 410 {
        Err(PushError::Gone)
    } else {
        let text = response.text().await.unwrap_or_default();
        Err(PushError::Upstream(format!("{} {}", status, text)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("config: {0}")]
    Config(String),
    #[error("network: {0}")]
    Network(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error("subscription gone (410)")]
    Gone,
    #[error("internal: {0}")]
    Internal(String),
}
