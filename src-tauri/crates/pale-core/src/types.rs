use serde::{Deserialize, Serialize};

/// SIP registration state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegState {
    Registered,
    Registering,
    Unregistered,
    None,
}

/// Call direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallDirection {
    Inbound,
    Outbound,
}

/// Call state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    Idle,
    Dialing,
    Ringing,
    EarlyMedia,
    Connected,
    OnHold,
    Transferring,
    Terminated,
}

/// SIP account configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipAccountConfig {
    pub display_name: String,
    pub sip_uri: String,
    pub registrar_uri: String,
    pub auth_username: String,
    pub auth_password: String,
    pub transport: Transport,
    pub reg_expiry: u32,
}

impl Default for SipAccountConfig {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            sip_uri: String::new(),
            registrar_uri: String::new(),
            auth_username: String::new(),
            auth_password: String::new(),
            transport: Transport::Tls,
            reg_expiry: 3600,
        }
    }
}

/// SIP transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Udp,
    Tcp,
    Tls,
}

/// Audio device info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub id: i32,
    pub name: String,
    pub input_count: u32,
    pub output_count: u32,
}

/// PJSIP account and call identifiers
pub type AccountId = i32;
pub type CallId = i32;
