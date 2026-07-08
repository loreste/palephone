use serde::{Deserialize, Serialize};

use crate::types::{CallDirection, CallState, RegState};

/// Events emitted by the PJSIP engine to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaleEvent {
    RegistrationState {
        account_id: i32,
        state: RegState,
        reason: String,
    },
    IncomingCall {
        call_id: i32,
        account_id: i32,
        caller_name: String,
        caller_uri: String,
    },
    CallState {
        call_id: i32,
        state: CallState,
        direction: CallDirection,
        remote_uri: String,
        remote_name: String,
    },
    AudioLevel {
        input: f32,
        output: f32,
    },
    RecordingState {
        call_id: i32,
        recording: bool,
        file_path: String,
    },
    VideoStreamState {
        call_id: i32,
        active: bool,
        has_incoming: bool,
        has_outgoing: bool,
    },
    AudioDevicesChanged,
    Error {
        message: String,
    },
}
