use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use pale_core::{
    load_config, save_config, AccountPersist, AppConfig, CallHistoryDb, CallRecord, EngineCommand,
    PaleEvent, PjsipEngine, RegState, SipAccountConfig, Transport,
};
use pale_matrix::{MatrixClient, MatrixEvent, RoomSummary};
use serde::Deserialize;
#[cfg(desktop)]
use tauri::menu::{MenuBuilder, MenuItemBuilder};
#[cfg(desktop)]
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(desktop)]
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Shared engine state accessible from Tauri commands
struct EngineState {
    engine: Option<Arc<PjsipEngine>>,
    init_error: String,
}

impl EngineState {
    fn get(&self) -> Result<&Arc<PjsipEngine>, String> {
        self.engine
            .as_ref()
            .ok_or_else(|| format!("SIP engine unavailable: {}", self.init_error))
    }
}

/// Shared call history database
struct HistoryState(Arc<Mutex<CallHistoryDb>>);

/// Shared Matrix client
struct MatrixState(Arc<tokio::sync::Mutex<MatrixClient>>);

/// Shared config state
struct ConfigState {
    config: Mutex<AppConfig>,
    path: PathBuf,
}

/// Runtime metadata that is learned from PJSIP callbacks.
struct SipRuntimeState {
    registered_account_id: Mutex<Option<i32>>,
}

#[derive(Debug, Clone)]
struct TrackedCall {
    direction: String,
    remote_uri: String,
    remote_name: String,
    start_time: String,
    connected_at_ms: Option<u128>,
}

type CallTracker = Arc<Mutex<HashMap<i32, TrackedCall>>>;

// ─── Tauri Commands ───

#[derive(Deserialize)]
struct AccountConfig {
    display_name: String,
    sip_uri: String,
    registrar_uri: String,
    auth_username: String,
    auth_password: String,
    transport: String,
}

fn normalize_sip_transport(value: &str) -> Result<Transport, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "tls" => Ok(Transport::Tls),
        "tcp" => Ok(Transport::Tcp),
        "udp" => Ok(Transport::Udp),
        other => Err(format!("unsupported SIP transport '{other}'")),
    }
}

fn registrar_has_port(authority: &str) -> bool {
    let host_port = authority.split(';').next().unwrap_or(authority);
    if let Some(rest) = host_port.strip_prefix('[') {
        return rest
            .split_once(']')
            .map(|(_, suffix)| suffix.starts_with(':'))
            .unwrap_or(false);
    }
    host_port
        .rsplit_once(':')
        .map(|(_, port)| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

fn normalize_registrar_uri(registrar_uri: &str, transport: Transport) -> String {
    let trimmed = registrar_uri.trim();
    let bare = trimmed
        .strip_prefix("sips:")
        .or_else(|| trimmed.strip_prefix("sip:"))
        .unwrap_or(trimmed);

    match transport {
        Transport::Tls => {
            let with_port = if registrar_has_port(bare) {
                bare.to_string()
            } else {
                let mut parts = bare.split(';');
                let host = parts.next().unwrap_or_default();
                let params: Vec<_> = parts.collect();
                if params.is_empty() {
                    format!("{host}:5061")
                } else {
                    format!("{host}:5061;{}", params.join(";"))
                }
            };
            format!("sips:{with_port}")
        }
        Transport::Tcp => format!("sip:{bare}"),
        Transport::Udp => format!("sip:{bare}"),
    }
}

#[tauri::command]
fn register_account(
    engine: State<EngineState>,
    config_state: State<ConfigState>,
    config: AccountConfig,
) -> Result<(), String> {
    validate_no_nul("display_name", &config.display_name)?;
    validate_no_nul("sip_uri", &config.sip_uri)?;
    validate_no_nul("registrar_uri", &config.registrar_uri)?;
    validate_no_nul("auth_username", &config.auth_username)?;
    validate_no_nul("auth_password", &config.auth_password)?;

    let transport = normalize_sip_transport(&config.transport)?;
    let registrar_uri = normalize_registrar_uri(&config.registrar_uri, transport);

    let account = SipAccountConfig {
        display_name: config.display_name,
        sip_uri: config.sip_uri,
        registrar_uri,
        auth_username: config.auth_username,
        auth_password: config.auth_password,
        transport,
        reg_expiry: 3600,
    };

    engine
        .get()?
        .send_command(EngineCommand::AddAccount(account.clone()))
        .map_err(|e| e.to_string())?;

    let mut current = config_state.config.lock().map_err(|e| e.to_string())?;
    current.account = Some(AccountPersist {
        display_name: account.display_name,
        sip_uri: account.sip_uri,
        registrar_uri: account.registrar_uri,
        auth_username: account.auth_username,
        transport: account.transport,
        reg_expiry: account.reg_expiry,
    });
    save_config(&config_state.path, &current).map_err(|e| e.to_string())
}

#[cfg(mobile)]
#[tauri::command]
fn open_popout_window(
    _app: AppHandle,
    _kind: String,
    _target_id: Option<String>,
    _title: Option<String>,
) -> Result<String, String> {
    Err("Pop-out windows are not available on mobile".to_string())
}

#[cfg(desktop)]
#[tauri::command]
fn open_popout_window(
    app: AppHandle,
    kind: String,
    target_id: Option<String>,
    title: Option<String>,
) -> Result<String, String> {
    let normalized_kind = match kind.trim().to_ascii_lowercase().as_str() {
        "chat" | "meeting" | "call" | "files" | "calendar" => kind.trim().to_ascii_lowercase(),
        _ => return Err("unsupported pop-out window kind".to_string()),
    };
    let safe_target = target_id
        .as_deref()
        .unwrap_or("main")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    let label = format!(
        "popout-{}-{}",
        normalized_kind,
        if safe_target.is_empty() {
            "main"
        } else {
            &safe_target
        }
    );
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        #[cfg(desktop)]
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(label);
    }
    let mut path = format!("index.html?popout={normalized_kind}");
    if !safe_target.is_empty() {
        path.push_str("&target=");
        path.push_str(&safe_target);
    }
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(path.into()))
        .title(title.unwrap_or_else(|| format!("Pale {}", normalized_kind)))
        .inner_size(980.0, 720.0)
        .min_inner_size(420.0, 360.0)
        .resizable(true)
        .build()
        .map_err(|err| err.to_string())?;
    Ok(label)
}

#[tauri::command]
fn make_call(
    state: State<EngineState>,
    runtime: State<Arc<SipRuntimeState>>,
    uri: String,
) -> Result<(), String> {
    validate_no_nul("uri", &uri)?;
    let account_id = runtime
        .registered_account_id
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "SIP account is not registered yet".to_string())?;

    state
        .get()?
        .send_command(EngineCommand::MakeCall { account_id, uri })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn answer_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::AnswerCall { call_id, code: 200 })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hangup_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::HangupCall(call_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hold_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::HoldCall(call_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn unhold_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::UnholdCall(call_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_mute(state: State<EngineState>, call_id: i32, muted: bool) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::SetMute { call_id, muted })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn send_dtmf(state: State<EngineState>, call_id: i32, digits: String) -> Result<(), String> {
    validate_no_nul("digits", &digits)?;
    state
        .get()?
        .send_command(EngineCommand::SendDtmf { call_id, digits })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn blind_transfer(state: State<EngineState>, call_id: i32, target: String) -> Result<(), String> {
    validate_no_nul("target", &target)?;
    state
        .get()?
        .send_command(EngineCommand::BlindTransfer { call_id, target })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn attended_transfer(
    state: State<EngineState>,
    call_id: i32,
    target_call_id: i32,
) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::AttendedTransfer {
            call_id,
            target_call_id,
        })
        .map_err(|e| e.to_string())
}

// ─── Call Recording ───

#[tauri::command]
fn start_recording(
    state: State<EngineState>,
    app_handle: tauri::AppHandle,
    call_id: i32,
) -> Result<String, String> {
    // Create recording file in app data dir
    let recordings_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");
    std::fs::create_dir_all(&recordings_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("call_{}_{}.wav", call_id, timestamp);
    let file_path = recordings_dir.join(&filename);
    let file_path_str = file_path.to_string_lossy().to_string();

    state
        .get()?
        .send_command(EngineCommand::StartRecording {
            call_id,
            file_path: file_path_str.clone(),
        })
        .map_err(|e| e.to_string())?;

    Ok(file_path_str)
}

#[tauri::command]
fn stop_recording(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::StopRecording { call_id })
        .map_err(|e| e.to_string())
}

// ─── Pale Server Login (HTTP from Rust to bypass webview fetch restrictions) ───

#[derive(serde::Deserialize)]
struct PaleLoginRequest {
    base_url: String,
    sip_uri: String,
    password: String,
}

#[tauri::command]
async fn pale_server_login(input: PaleLoginRequest) -> Result<serde_json::Value, String> {
    let url = format!("{}/v1/auth/login", input.base_url.trim_end_matches('/'));
    log::info!("pale_server_login -> {}", url);
    let body = serde_json::json!({
        "sip_uri": input.sip_uri,
        "password": input.password,
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", format!("Pale/{}", env!("CARGO_PKG_VERSION")))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    log::info!("pale_server_login <- {}", response.status());

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Login failed ({}): {}", status, text));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Invalid response: {}", e))
}

#[derive(serde::Deserialize)]
struct PaleServerRequest {
    base_url: String,
    method: String,
    path: String,
    token: Option<String>,
    body: Option<serde_json::Value>,
}

#[tauri::command]
async fn pale_server_request(input: PaleServerRequest) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", input.base_url.trim_end_matches('/'), input.path);
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut req = match input.method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };

    req = req
        .header("Content-Type", "application/json")
        .header("User-Agent", format!("Pale/{}", env!("CARGO_PKG_VERSION")));

    if let Some(token) = &input.token {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    if let Some(body) = &input.body {
        req = req.body(body.to_string());
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    let status = response.status();

    if status.as_u16() == 204 {
        return Ok(serde_json::json!({"ok": true}));
    }

    let text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("{}: {}", status, text));
    }

    serde_json::from_str(&text).map_err(|_| text)
}

// ─── Config Commands ───

#[tauri::command]
fn get_config(state: State<ConfigState>) -> Result<AppConfig, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
fn save_settings(state: State<ConfigState>, config: AppConfig) -> Result<(), String> {
    let mut current = state.config.lock().map_err(|e| e.to_string())?;
    *current = config.clone();
    save_config(&state.path, &config).map_err(|e| e.to_string())
}

// ─── Keychain Commands ───

#[tauri::command]
fn store_sip_password(account_id: String, password: String) -> Result<(), String> {
    pale_core::store_password(&account_id, &password)
}

#[tauri::command]
fn get_sip_password(account_id: String) -> Result<Option<String>, String> {
    pale_core::get_password(&account_id)
}

#[tauri::command]
fn delete_sip_password(account_id: String) -> Result<(), String> {
    pale_core::delete_password(&account_id)
}

// ─── Matrix Commands ───

#[tauri::command]
async fn matrix_login(
    state: State<'_, MatrixState>,
    homeserver: String,
    username: String,
    password: String,
) -> Result<String, String> {
    let mut client = state.0.lock().await;
    let user_id = client.login(&homeserver, &username, &password).await?;
    client.start_sync().await?;
    Ok(user_id)
}

#[tauri::command]
async fn matrix_logout(state: State<'_, MatrixState>) -> Result<(), String> {
    let mut client = state.0.lock().await;
    client.logout().await
}

#[tauri::command]
async fn matrix_get_rooms(state: State<'_, MatrixState>) -> Result<Vec<RoomSummary>, String> {
    let client = state.0.lock().await;
    client.get_rooms().await
}

#[tauri::command]
async fn matrix_send_message(
    state: State<'_, MatrixState>,
    room_id: String,
    body: String,
) -> Result<String, String> {
    let client = state.0.lock().await;
    client.send_message(&room_id, &body).await
}

#[tauri::command]
async fn matrix_set_typing(
    state: State<'_, MatrixState>,
    room_id: String,
    typing: bool,
) -> Result<(), String> {
    let client = state.0.lock().await;
    client.set_typing(&room_id, typing).await
}

#[tauri::command]
async fn matrix_send_file(
    state: State<'_, MatrixState>,
    room_id: String,
    file_path: String,
) -> Result<String, String> {
    let client = state.0.lock().await;
    client.send_file(&room_id, &file_path).await
}

#[tauri::command]
async fn matrix_create_dm(
    state: State<'_, MatrixState>,
    user_id: String,
) -> Result<String, String> {
    let client = state.0.lock().await;
    client.create_dm(&user_id).await
}

#[tauri::command]
async fn matrix_is_logged_in(state: State<'_, MatrixState>) -> Result<bool, String> {
    let client = state.0.lock().await;
    Ok(client.is_logged_in())
}

// ─── Video Commands ───

#[tauri::command]
fn make_video_call(
    state: State<EngineState>,
    runtime: State<Arc<SipRuntimeState>>,
    uri: String,
) -> Result<(), String> {
    validate_no_nul("uri", &uri)?;
    let account_id = runtime
        .registered_account_id
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "SIP account is not registered yet".to_string())?;

    state
        .get()?
        .send_command(EngineCommand::MakeVideoCall { account_id, uri })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_video(state: State<EngineState>, call_id: i32, enabled: bool) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::ToggleVideo { call_id, enabled })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn start_screen_share(
    state: State<EngineState>,
    call_id: i32,
    enabled: bool,
) -> Result<(), String> {
    state
        .get()?
        .send_command(EngineCommand::ScreenShare { call_id, enabled })
        .map_err(|e| e.to_string())
}

// ─── Audio Device Commands ───

#[derive(serde::Serialize)]
struct AudioDeviceInfo {
    id: i32,
    name: String,
    input_count: u32,
    output_count: u32,
}

#[tauri::command]
fn list_audio_devices(state: State<EngineState>) -> Result<Vec<AudioDeviceInfo>, String> {
    // Don't touch PJSIP FFI if the engine never initialized.
    state.get()?;
    unsafe {
        let count = pjsip_sys::pjmedia_aud_dev_count() as i32;
        let mut devices = Vec::new();
        for i in 0..count {
            let mut info: pjsip_sys::pjmedia_aud_dev_info = std::mem::zeroed();
            let status = pjsip_sys::pjmedia_aud_dev_get_info(i, &mut info);
            if status == 0 {
                let name = std::ffi::CStr::from_ptr(info.name.as_ptr())
                    .to_string_lossy()
                    .to_string();
                devices.push(AudioDeviceInfo {
                    id: i,
                    name,
                    input_count: info.input_count,
                    output_count: info.output_count,
                });
            }
        }
        Ok(devices)
    }
}

#[derive(serde::Serialize)]
struct HidAudioDevice {
    name: String,
    device_type: String,
    connected: bool,
}

#[tauri::command]
fn detect_hid_devices(state: State<EngineState>) -> Result<Vec<HidAudioDevice>, String> {
    // Reuse existing audio device enumeration to detect connected headsets
    state.get()?;
    unsafe {
        let count = pjsip_sys::pjmedia_aud_dev_count() as i32;
        let mut devices = Vec::new();
        for i in 0..count {
            let mut info: pjsip_sys::pjmedia_aud_dev_info = std::mem::zeroed();
            let status = pjsip_sys::pjmedia_aud_dev_get_info(i, &mut info);
            if status == 0 {
                let name = std::ffi::CStr::from_ptr(info.name.as_ptr())
                    .to_string_lossy()
                    .to_string();
                let device_type = if info.input_count > 0 && info.output_count > 0 {
                    "headset"
                } else if info.output_count > 0 {
                    "speaker"
                } else {
                    "microphone"
                };
                devices.push(HidAudioDevice {
                    name,
                    device_type: device_type.to_string(),
                    connected: true,
                });
            }
        }
        Ok(devices)
    }
}

// ─── Call History Commands ───

#[tauri::command]
fn get_call_history(state: State<HistoryState>) -> Result<Vec<CallRecord>, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .list_recent(100)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_call_record(state: State<HistoryState>, record: CallRecord) -> Result<i64, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .insert(&record)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_call_record(state: State<HistoryState>, id: i64) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .delete(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_call_history(state: State<HistoryState>) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .clear_all()
        .map_err(|e| e.to_string())
}

// ─── Event Bridge ───

fn validate_no_nul(field: &str, value: &str) -> Result<(), String> {
    if value.contains('\0') {
        Err(format!("{} contains an invalid NUL byte", field))
    } else {
        Ok(())
    }
}

fn current_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn current_iso_time() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn track_call_event(
    event: &PaleEvent,
    history: &Arc<Mutex<CallHistoryDb>>,
    tracker: &CallTracker,
    runtime: &SipRuntimeState,
) {
    match event {
        PaleEvent::RegistrationState {
            account_id, state, ..
        } => {
            if let Ok(mut registered_account_id) = runtime.registered_account_id.lock() {
                if matches!(state, RegState::Registered) {
                    *registered_account_id = Some(*account_id);
                } else if matches!(*registered_account_id, Some(current) if current == *account_id)
                {
                    *registered_account_id = None;
                }
            }
        }
        PaleEvent::IncomingCall {
            call_id,
            caller_name,
            caller_uri,
            ..
        } => {
            if let Ok(mut calls) = tracker.lock() {
                calls.entry(*call_id).or_insert_with(|| TrackedCall {
                    direction: "inbound".to_string(),
                    remote_uri: caller_uri.clone(),
                    remote_name: caller_name.clone(),
                    start_time: current_iso_time(),
                    connected_at_ms: None,
                });
            }
        }
        PaleEvent::CallState {
            call_id,
            state,
            direction,
            remote_uri,
            remote_name,
        } => {
            let now = current_epoch_ms();
            let direction = match direction {
                pale_core::CallDirection::Inbound => "inbound",
                pale_core::CallDirection::Outbound => "outbound",
            };

            let mut completed = None;
            if let Ok(mut calls) = tracker.lock() {
                let tracked = calls.entry(*call_id).or_insert_with(|| TrackedCall {
                    direction: direction.to_string(),
                    remote_uri: remote_uri.clone(),
                    remote_name: remote_name.clone(),
                    start_time: current_iso_time(),
                    connected_at_ms: None,
                });

                if tracked.remote_uri.is_empty() && !remote_uri.is_empty() {
                    tracked.remote_uri = remote_uri.clone();
                }
                if tracked.remote_name.is_empty() && !remote_name.is_empty() {
                    tracked.remote_name = remote_name.clone();
                }

                match state {
                    pale_core::CallState::Connected => {
                        if tracked.connected_at_ms.is_none() {
                            tracked.connected_at_ms = Some(now);
                        }
                    }
                    pale_core::CallState::Terminated => {
                        completed = calls.remove(call_id);
                    }
                    _ => {}
                }
            }

            if let Some(tracked) = completed {
                let duration_secs = tracked
                    .connected_at_ms
                    .map(|connected| now.saturating_sub(connected) / 1000)
                    .unwrap_or(0) as i64;
                let record = CallRecord {
                    id: 0,
                    direction: tracked.direction,
                    remote_uri: tracked.remote_uri,
                    remote_name: tracked.remote_name,
                    start_time: tracked.start_time,
                    duration_secs,
                    answered: tracked.connected_at_ms.is_some(),
                };
                if let Ok(db) = history.lock() {
                    if let Err(e) = db.insert(&record) {
                        log::warn!("Failed to persist call history from backend event: {}", e);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Forward MatrixEvents to the Tauri frontend
fn start_matrix_event_bridge(app: AppHandle, matrix: Arc<tokio::sync::Mutex<MatrixClient>>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            let mut rx = matrix.lock().await.subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let event_name = match &event {
                            MatrixEvent::AuthStateChanged { .. } => "matrix://auth-state",
                            MatrixEvent::RoomListUpdated { .. } => "matrix://rooms",
                            MatrixEvent::Message(_) => "matrix://message",
                            MatrixEvent::Typing { .. } => "matrix://typing",
                            MatrixEvent::TransferProgress(_) => "matrix://transfer-progress",
                            MatrixEvent::SyncError { .. } => "matrix://sync-error",
                            MatrixEvent::VerificationRequest { .. } => "matrix://verification",
                        };
                        let _ = app.emit(event_name, &event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("Matrix event bridge lagged by {} messages", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    });
}

/// Forward PaleEvents from the Rust engine to the Tauri frontend
fn start_event_bridge(
    app: AppHandle,
    engine: Arc<PjsipEngine>,
    history: Arc<Mutex<CallHistoryDb>>,
    tracker: CallTracker,
    runtime: Arc<SipRuntimeState>,
) {
    let mut rx = engine.subscribe();

    // Spawn on a separate thread since we don't have a tokio runtime in the main app
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        track_call_event(&event, &history, &tracker, &runtime);

                        let event_name = match &event {
                            PaleEvent::RegistrationState { .. } => "sip://reg-state",
                            PaleEvent::IncomingCall { .. } => "sip://incoming-call",
                            PaleEvent::CallState { .. } => "sip://call-state",
                            PaleEvent::AudioLevel { .. } => "audio://level",
                            PaleEvent::AudioDevicesChanged => "audio://devices-changed",
                            PaleEvent::RecordingState { .. } => "sip://recording-state",
                            PaleEvent::Error { .. } => "pale://error",
                        };

                        // Send native OS notification for incoming calls
                        // (critical for Android background + desktop tray)
                        if let PaleEvent::IncomingCall {
                            ref caller_name,
                            ref caller_uri,
                            ..
                        } = event
                        {
                            let title = "Incoming Call";
                            let body = if caller_name.is_empty() {
                                caller_uri.clone()
                            } else {
                                caller_name.clone()
                            };
                            use tauri_plugin_notification::NotificationExt;
                            let _ = app.notification().builder().title(title).body(&body).show();
                        }

                        let _ = app.emit(event_name, &event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("Event bridge lagged by {} messages", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        log::info!("Event bridge channel closed");
                        break;
                    }
                }
            }
        });
    });
}

// ─── System Tray (desktop only) ───

#[cfg(desktop)]
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(desktop)]
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "Show Pale").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("Pale Softphone")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => {
                show_main_window(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

// ─── App Entry ───

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Android may already have a logger; env_logger only works on desktop (stdout/stderr)
    #[cfg(not(target_os = "android"))]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    #[cfg(target_os = "android")]
    {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
    }

    let runtime = Arc::new(SipRuntimeState {
        registered_account_id: Mutex::new(None),
    });
    let runtime_for_bridge = runtime.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            // SIP commands
            register_account,
            open_popout_window,
            make_call,
            answer_call,
            hangup_call,
            hold_call,
            unhold_call,
            set_mute,
            send_dtmf,
            blind_transfer,
            attended_transfer,
            // Recording
            start_recording,
            stop_recording,
            // Server API
            pale_server_login,
            pale_server_request,
            // Call history
            get_call_history,
            add_call_record,
            delete_call_record,
            clear_call_history,
            // Config + keychain
            get_config,
            save_settings,
            store_sip_password,
            get_sip_password,
            delete_sip_password,
            // Audio devices
            list_audio_devices,
            detect_hid_devices,
            // Video
            make_video_call,
            toggle_video,
            start_screen_share,
            // Matrix
            matrix_login,
            matrix_logout,
            matrix_get_rooms,
            matrix_send_message,
            matrix_set_typing,
            matrix_send_file,
            matrix_create_dm,
            matrix_is_logged_in,
        ])
        .setup(move |app| {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("failed to resolve the app data directory: {e}"))?;
            std::fs::create_dir_all(&app_data).ok();

            // Load persisted config
            let config_path = app_data.join("config.json");
            let config = load_config(&config_path);
            log::info!("Config loaded from {:?}", config_path);
            let network_config = config.network.clone();
            app.manage(ConfigState {
                config: Mutex::new(config),
                path: config_path,
            });

            // Initialize PJSIP after loading persisted media settings. A failure
            // must not crash the app; SIP commands report the init error to the UI.
            let (engine, engine_init_error) = match PjsipEngine::new(network_config) {
                Ok(engine) => (Some(Arc::new(engine)), String::new()),
                Err(e) => {
                    log::error!("Failed to initialize PJSIP engine: {e}");
                    (None, e.to_string())
                }
            };
            let engine_for_bridge = engine.clone();
            app.manage(EngineState {
                engine,
                init_error: engine_init_error,
            });

            // Initialize call history database
            let db_path = app_data.join("call_history.db");
            let history_db = match CallHistoryDb::open(&db_path) {
                Ok(db) => Arc::new(Mutex::new(db)),
                Err(e) => {
                    log::error!(
                        "Failed to open call history at {}: {e}; using in-memory fallback",
                        db_path.display()
                    );
                    Arc::new(Mutex::new(
                        CallHistoryDb::open(std::path::Path::new(":memory:"))
                            .expect("in-memory SQLite should never fail"),
                    ))
                }
            };
            app.manage(HistoryState(history_db.clone()));
            log::info!("Call history DB opened at {:?}", db_path);
            let call_tracker = Arc::new(Mutex::new(HashMap::new()));

            // Initialize Matrix client
            let matrix_client = Arc::new(tokio::sync::Mutex::new(MatrixClient::new(&app_data)));
            app.manage(MatrixState(matrix_client.clone()));
            log::info!("Matrix client initialized");

            // Start Matrix event bridge
            start_matrix_event_bridge(app.handle().clone(), matrix_client);

            // Set up system tray (desktop only)
            #[cfg(desktop)]
            setup_tray(app)?;

            // Close-to-tray: hide window instead of quitting (desktop only)
            #[cfg(desktop)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let win = window.clone();
                    window.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = win.hide();
                        }
                    });
                }
            }

            #[cfg(desktop)]
            show_main_window(app.handle());

            if let Some(engine) = engine_for_bridge {
                start_event_bridge(
                    app.handle().clone(),
                    engine,
                    history_db,
                    call_tracker,
                    runtime_for_bridge,
                );
            } else {
                log::warn!("SIP engine unavailable — event bridge not started");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = _event
            {
                if !has_visible_windows {
                    show_main_window(_app);
                }
            }
        });
}
