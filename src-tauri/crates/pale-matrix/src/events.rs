use serde::{Deserialize, Serialize};

use crate::types::*;

/// Events emitted by the Matrix client to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MatrixEvent {
    /// Auth state changed
    AuthStateChanged {
        state: MatrixAuthState,
        user_id: Option<String>,
        display_name: Option<String>,
    },
    /// Room list updated
    RoomListUpdated {
        rooms: Vec<RoomSummary>,
    },
    /// New message received
    Message(ChatMessage),
    /// Typing indicator
    Typing {
        room_id: String,
        user_ids: Vec<String>,
    },
    /// File transfer progress
    TransferProgress(TransferProgress),
    /// Sync error
    SyncError {
        message: String,
    },
    /// Verification request
    VerificationRequest {
        sender: String,
        flow_id: String,
    },
}
