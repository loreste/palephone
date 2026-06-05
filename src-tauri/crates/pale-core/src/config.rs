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
    pub ui: UiPersist,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            account: None,
            audio: AudioPersist::default(),
            network: NetworkPersist::default(),
            matrix: MatrixPersist::default(),
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
            codec_priority: vec![
                "opus".into(),
                "g722".into(),
                "pcmu".into(),
                "pcma".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPersist {
    pub stun_server: String,
    pub turn_server: String,
    pub turn_username: String,
    pub enable_ice: bool,
    pub sip_port: u16,
    pub rtp_port_min: u16,
    pub rtp_port_max: u16,
}

impl Default for NetworkPersist {
    fn default() -> Self {
        Self {
            stun_server: "stun:stun.l.google.com:19302".into(),
            turn_server: String::new(),
            turn_username: String::new(),
            enable_ice: true,
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
