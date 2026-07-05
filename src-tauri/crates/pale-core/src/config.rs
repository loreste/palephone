use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{PaleError, PaleResult};
use crate::types::Transport;

/// Persisted application configuration (passwords are NOT stored here — use OS keychain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub account: Option<AccountPersist>,
    #[serde(default)]
    pub audio: AudioPersist,
    #[serde(default)]
    pub network: NetworkPersist,
    #[serde(default)]
    pub matrix: MatrixPersist,
    #[serde(default)]
    pub server: ServerPersist,
    #[serde(default)]
    pub notifications: NotificationPersist,
    #[serde(default)]
    pub ui: UiPersist,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            account: None,
            audio: AudioPersist::default(),
            network: NetworkPersist::default(),
            matrix: MatrixPersist::default(),
            server: ServerPersist::default(),
            notifications: NotificationPersist::default(),
            ui: UiPersist::default(),
        }
    }
}

/// Matrix homeserver configuration (password stored in OS keychain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixPersist {
    pub homeserver: String,
    pub username: String,
    pub user_id: Option<String>,
}

impl Default for MatrixPersist {
    fn default() -> Self {
        Self {
            homeserver: String::new(),
            username: String::new(),
            user_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPersist {
    pub display_name: String,
    pub sip_uri: String,
    pub registrar_uri: String,
    pub auth_username: String,
    pub transport: Transport,
    pub reg_expiry: u32,
    // NOTE: password is stored in OS keychain, NOT here
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioPersist {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub echo_cancel: bool,
    pub noise_suppression: bool,
    pub auto_gain: bool,
    pub codec_priority: Vec<String>,
}

impl Default for AudioPersist {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            echo_cancel: true,
            noise_suppression: true,
            auto_gain: false,
            codec_priority: vec!["opus".into(), "g722".into(), "pcmu".into(), "pcma".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPersist {
    pub stun_server: String,
    pub turn_server: String,
    pub turn_username: String,
    #[serde(default)]
    pub turn_password: String,
    pub enable_ice: bool,
    #[serde(default)]
    pub srtp_mode: SrtpMode,
    pub sip_port: u16,
    pub rtp_port_min: u16,
    pub rtp_port_max: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SrtpMode {
    Disabled,
    Optional,
    Required,
}

impl Default for SrtpMode {
    fn default() -> Self {
        Self::Optional
    }
}

impl Default for NetworkPersist {
    fn default() -> Self {
        Self {
            stun_server: "stun:stun.l.google.com:19302".into(),
            turn_server: String::new(),
            turn_username: String::new(),
            turn_password: String::new(),
            enable_ice: true,
            srtp_mode: SrtpMode::Optional,
            sip_port: 5060,
            rtp_port_min: 10000,
            rtp_port_max: 20000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPersist {
    pub theme: String,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for UiPersist {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            window_width: 380,
            window_height: 640,
        }
    }
}

/// Notification preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPersist {
    pub enabled: bool,
    pub sound_enabled: bool,
    pub dnd_enabled: bool,
    pub dnd_start: String, // HH:MM format, e.g. "22:00"
    pub dnd_end: String,   // HH:MM format, e.g. "07:00"
    pub muted_rooms: Vec<String>,
}

impl Default for NotificationPersist {
    fn default() -> Self {
        Self {
            enabled: true,
            sound_enabled: true,
            dnd_enabled: false,
            dnd_start: "22:00".into(),
            dnd_end: "07:00".into(),
            muted_rooms: Vec::new(),
        }
    }
}

/// Pale server connection configuration (password stored in OS keychain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPersist {
    pub url: String,
    pub username: String,
    pub auto_connect: bool,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

impl Default for ServerPersist {
    fn default() -> Self {
        Self {
            url: String::new(),
            username: "admin".into(),
            auto_connect: false,
            role: None,
            display_name: None,
        }
    }
}

/// Load config from disk, returning defaults if file doesn't exist
pub fn load_config(config_path: &Path) -> AppConfig {
    match fs::read_to_string(config_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

/// Save config to disk atomically (write to tmp file then rename)
pub fn save_config(config_path: &Path, config: &AppConfig) -> PaleResult<()> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| PaleError::InvalidConfig(e.to_string()))?;

    let tmp_path = config_path.with_extension("json.tmp");
    fs::write(&tmp_path, &json)
        .map_err(|e| PaleError::InvalidConfig(format!("Failed to write config: {}", e)))?;
    fs::rename(&tmp_path, config_path)
        .map_err(|e| PaleError::InvalidConfig(format!("Failed to rename config: {}", e)))?;

    Ok(())
}
