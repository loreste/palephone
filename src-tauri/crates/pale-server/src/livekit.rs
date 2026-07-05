//! LiveKit integration for multi-party audio/video conferencing.
//!
//! When `PALE_LIVEKIT_URL`, `PALE_LIVEKIT_API_KEY`, and `PALE_LIVEKIT_API_SECRET`
//! are set, conferences use LiveKit as the SFU.  Otherwise the server falls back
//! to the existing signaling-only mode.

use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Configuration ────────────────────────────────────────────────────

/// LiveKit server connection details, read from environment variables.
#[derive(Debug, Clone)]
pub struct LiveKitConfig {
    /// WebSocket URL of the LiveKit server, e.g. `ws://localhost:7880`.
    pub url: String,
    /// API key issued by the LiveKit deployment.
    pub api_key: String,
    /// API secret used to sign access tokens.
    pub api_secret: String,
}

impl LiveKitConfig {
    /// Try to build a config from environment variables.  Returns `None` when
    /// the required variables are not set (graceful fallback).
    pub fn from_env() -> Option<Self> {
        let url = non_empty_env("PALE_LIVEKIT_URL")?;
        let api_key = non_empty_env("PALE_LIVEKIT_API_KEY")?;
        let api_secret = non_empty_env("PALE_LIVEKIT_API_SECRET")?;
        Some(Self {
            url,
            api_key,
            api_secret,
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
}

// ── Access-token generation (LiveKit JWT) ────────────────────────────

/// VideoGrant controls what a participant is allowed to do inside a room.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoGrant {
    /// The room the participant may join.
    pub room: String,
    /// Whether the participant is allowed to join this room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_join: Option<bool>,
    /// Whether the participant can publish audio/video tracks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_publish: Option<bool>,
    /// Whether the participant can subscribe to other tracks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_subscribe: Option<bool>,
    /// Whether the participant can publish data messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_publish_data: Option<bool>,
    /// Whether the participant can update their own metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_update_own_metadata: Option<bool>,
    /// Whether the participant is hidden (e.g. recorder bots).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Whether this token can create rooms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_create: Option<bool>,
    /// Whether this token can list rooms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_list: Option<bool>,
    /// Whether the room should be recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_record: Option<bool>,
}

impl VideoGrant {
    /// Standard grant for a meeting participant.
    pub fn participant(room: &str) -> Self {
        Self {
            room: room.to_string(),
            room_join: Some(true),
            can_publish: Some(true),
            can_subscribe: Some(true),
            can_publish_data: Some(true),
            can_update_own_metadata: Some(true),
            hidden: None,
            room_create: None,
            room_list: None,
            room_record: None,
        }
    }

    /// Grant for a recorder/egress bot that is hidden and cannot publish.
    pub fn recorder(room: &str) -> Self {
        Self {
            room: room.to_string(),
            room_join: Some(true),
            can_publish: Some(false),
            can_subscribe: Some(true),
            can_publish_data: Some(false),
            can_update_own_metadata: None,
            hidden: Some(true),
            room_create: None,
            room_list: None,
            room_record: Some(true),
        }
    }

    /// Admin-level grant for room management (create/list).
    pub fn admin() -> Self {
        Self {
            room: String::new(),
            room_join: None,
            can_publish: None,
            can_subscribe: None,
            can_publish_data: None,
            can_update_own_metadata: None,
            hidden: None,
            room_create: Some(true),
            room_list: Some(true),
            room_record: None,
        }
    }
}

/// JWT claims for a LiveKit access token.
#[derive(Debug, Serialize, Deserialize)]
pub struct LiveKitClaims {
    /// Issued-at timestamp (Unix seconds).
    pub iat: i64,
    /// Expiration timestamp (Unix seconds).
    pub exp: i64,
    /// Not-before timestamp (Unix seconds).
    pub nbf: i64,
    /// Issuer – the LiveKit API key.
    pub iss: String,
    /// Subject – the participant identity.
    pub sub: String,
    /// Unique token identifier.
    pub jti: String,
    /// The video grant embedded in the token.
    pub video: VideoGrant,
    /// Optional metadata (JSON string) attached to the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    /// Optional display name for the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Generate a signed LiveKit access token.
///
/// * `config`   – LiveKit server credentials
/// * `identity` – unique participant identity (typically the SIP URI)
/// * `name`     – human-readable display name
/// * `grant`    – the `VideoGrant` controlling permissions
/// * `ttl_secs` – token lifetime in seconds (default: 6 hours)
pub fn generate_token(
    config: &LiveKitConfig,
    identity: &str,
    name: &str,
    grant: VideoGrant,
    ttl_secs: i64,
) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let claims = LiveKitClaims {
        iat: now,
        nbf: now,
        exp: now + ttl_secs,
        iss: config.api_key.clone(),
        sub: identity.to_string(),
        jti: Uuid::new_v4().to_string(),
        video: grant,
        metadata: None,
        name: Some(name.to_string()),
    };

    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(config.api_secret.as_bytes());
    encode(&header, &claims, &key).map_err(|e| format!("failed to sign LiveKit token: {e}"))
}

// ── Room management via LiveKit HTTP API ─────────────────────────────

/// Create a LiveKit room by calling the LiveKit server's `POST /twirp/livekit.RoomService/CreateRoom`.
///
/// LiveKit uses Twirp (Protobuf-over-HTTP), but also accepts JSON bodies.
pub async fn create_room(config: &LiveKitConfig, room_name: &str) -> Result<(), String> {
    let url = livekit_http_url(config, "/twirp/livekit.RoomService/CreateRoom");
    let token = generate_token(
        config,
        "pale-server",
        "Pale Server",
        VideoGrant::admin(),
        60,
    )?;

    let body = serde_json::json!({
        "name": room_name,
        "empty_timeout": 300,
        "max_participants": 250,
    });

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap_or_else(|_| reqwest::Client::new());
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LiveKit CreateRoom request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "LiveKit CreateRoom returned {status}: {text}"
        ));
    }
    Ok(())
}

/// Delete (destroy) a LiveKit room.
pub async fn delete_room(config: &LiveKitConfig, room_name: &str) -> Result<(), String> {
    let url = livekit_http_url(config, "/twirp/livekit.RoomService/DeleteRoom");
    let token = generate_token(
        config,
        "pale-server",
        "Pale Server",
        VideoGrant::admin(),
        60,
    )?;

    let body = serde_json::json!({ "room": room_name });

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap_or_else(|_| reqwest::Client::new());
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LiveKit DeleteRoom request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        log::warn!("LiveKit DeleteRoom returned {status}: {text}");
        // Non-fatal: room may already have been cleaned up.
    }
    Ok(())
}

// ── Egress (recording) ──────────────────────────────────────────────

/// Response from LiveKit Egress API.
#[derive(Debug, Clone, Deserialize)]
pub struct EgressInfo {
    #[serde(default)]
    pub egress_id: String,
}

/// Start a room composite recording via LiveKit Egress.
///
/// Records all tracks into a single file.
pub async fn start_room_composite_egress(
    config: &LiveKitConfig,
    room_name: &str,
    output_file_path: &str,
) -> Result<String, String> {
    let url = livekit_http_url(
        config,
        "/twirp/livekit.Egress/StartRoomCompositeEgress",
    );
    let token = generate_token(
        config,
        "pale-server",
        "Pale Server",
        VideoGrant::admin(),
        60,
    )?;

    let body = serde_json::json!({
        "room_name": room_name,
        "file": {
            "file_type": "MP4",
            "filepath": output_file_path,
        },
        "audio_only": false,
    });

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap_or_else(|_| reqwest::Client::new());
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LiveKit StartRoomCompositeEgress failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "LiveKit Egress returned {status}: {text}"
        ));
    }

    let info: EgressInfo = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Egress response: {e}"))?;
    Ok(info.egress_id)
}

/// Stop a running egress by ID.
pub async fn stop_egress(config: &LiveKitConfig, egress_id: &str) -> Result<(), String> {
    let url = livekit_http_url(config, "/twirp/livekit.Egress/StopEgress");
    let token = generate_token(
        config,
        "pale-server",
        "Pale Server",
        VideoGrant::admin(),
        60,
    )?;

    let body = serde_json::json!({ "egress_id": egress_id });

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap_or_else(|_| reqwest::Client::new());
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LiveKit StopEgress failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        log::warn!("LiveKit StopEgress returned {status}: {text}");
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Convert a LiveKit WebSocket URL to an HTTP URL for the Twirp API.
///
/// `ws://host:7880`  -> `http://host:7880`
/// `wss://host:7443` -> `https://host:7443`
fn livekit_http_url(config: &LiveKitConfig, path: &str) -> String {
    let base = config
        .url
        .replace("ws://", "http://")
        .replace("wss://", "https://");
    let base = base.trim_end_matches('/');
    format!("{base}{path}")
}

/// Build a deterministic LiveKit room name from a conference ID.
pub fn room_name_for_conference(conference_id: Uuid) -> String {
    format!("pale-conf-{conference_id}")
}

/// Default token TTL: 6 hours.
pub const DEFAULT_TOKEN_TTL_SECS: i64 = 6 * 3600;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let config = LiveKitConfig {
            url: "ws://localhost:7880".to_string(),
            api_key: "test-key".to_string(),
            api_secret: "test-secret-that-is-long-enough".to_string(),
        };
        let token = generate_token(
            &config,
            "sip:alice@example.com",
            "Alice",
            VideoGrant::participant("test-room"),
            3600,
        )
        .expect("token generation should succeed");

        // Should be a valid JWT with 3 dot-separated parts
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn test_room_name() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            room_name_for_conference(id),
            "pale-conf-550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_livekit_http_url() {
        let config = LiveKitConfig {
            url: "ws://localhost:7880".to_string(),
            api_key: String::new(),
            api_secret: String::new(),
        };
        assert_eq!(
            livekit_http_url(&config, "/twirp/livekit.RoomService/CreateRoom"),
            "http://localhost:7880/twirp/livekit.RoomService/CreateRoom"
        );

        let config_tls = LiveKitConfig {
            url: "wss://lk.example.com:7443".to_string(),
            api_key: String::new(),
            api_secret: String::new(),
        };
        assert_eq!(
            livekit_http_url(&config_tls, "/twirp/livekit.Egress/StopEgress"),
            "https://lk.example.com:7443/twirp/livekit.Egress/StopEgress"
        );
    }
}
