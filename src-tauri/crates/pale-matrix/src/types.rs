use serde::{Deserialize, Serialize};

/// Matrix authentication state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixAuthState {
    LoggedOut,
    LoggingIn,
    LoggedIn,
    SyncError,
}

/// Matrix sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    Idle,
    Syncing,
    Error { message: String },
}

/// A Matrix room summary for the conversation list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub name: String,
    pub is_direct: bool,
    pub is_encrypted: bool,
    pub last_message: Option<String>,
    pub last_message_sender: Option<String>,
    pub last_message_ts: Option<u64>,
    pub unread_count: u32,
    pub avatar_url: Option<String>,
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub sender_name: Option<String>,
    pub body: String,
    pub msg_type: MessageType,
    pub timestamp: u64,
    pub is_encrypted: bool,
    pub is_own: bool,
}

/// Message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Text,
    Image { url: String, thumbnail_url: Option<String>, width: Option<u32>, height: Option<u32> },
    File { url: String, filename: String, size: Option<u64>, mimetype: Option<String> },
    Audio { url: String, duration_ms: Option<u64> },
    Video { url: String, duration_ms: Option<u64>, width: Option<u32>, height: Option<u32> },
    Emote,
    Notice,
}

/// File transfer progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub filename: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub direction: TransferDirection,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    InProgress,
    Complete,
    Failed { error: String },
}

/// Encryption verification state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Unverified,
    Verified,
    Blocked,
}

/// Matrix login credentials
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixLoginRequest {
    pub homeserver: String,
    pub username: String,
    pub password: String,
}
