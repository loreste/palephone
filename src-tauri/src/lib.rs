use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use pale_core::{
    load_config, save_config, AccountPersist, AppConfig, CallHistoryDb, CallRecord, EngineCommand,
    PaleEvent, PjsipEngine, SipAccountConfig, Transport,
};
use pale_matrix::{MatrixClient, MatrixEvent, RoomSummary};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(desktop)]
use tauri::menu::{MenuBuilder, MenuItemBuilder};
#[cfg(desktop)]
use tauri::tray::TrayIconBuilder;

/// Shared engine state accessible from Tauri commands
struct EngineState(Arc<PjsipEngine>);

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
    default_account_id: Mutex<Option<i32>>,
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

    let transport = match config.transport.as_str() {
        "tls" => Transport::Tls,
        "tcp" => Transport::Tcp,
        _ => Transport::Udp,
    };

    let account = SipAccountConfig {
        display_name: config.display_name,
        sip_uri: config.sip_uri,
        registrar_uri: config.registrar_uri,
        auth_username: config.auth_username,
        auth_password: config.auth_password,
        transport,
        reg_expiry: 3600,
    };

    engine
        .0
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

#[tauri::command]
fn make_call(
    state: State<EngineState>,
    runtime: State<SipRuntimeState>,
    uri: String,
) -> Result<(), String> {
    validate_no_nul("uri", &uri)?;
    let account_id = runtime
        .default_account_id
        .lock()
        .map_err(|e| e.to_string())?
        .unwrap_or(0);

    state
        .0
        .send_command(EngineCommand::MakeCall { account_id, uri })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn answer_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .0
        .send_command(EngineCommand::AnswerCall { call_id, code: 200 })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hangup_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .0
        .send_command(EngineCommand::HangupCall(call_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hold_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .0
        .send_command(EngineCommand::HoldCall(call_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn unhold_call(state: State<EngineState>, call_id: i32) -> Result<(), String> {
    state
        .0
        .send_command(EngineCommand::UnholdCall(call_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_mute(state: State<EngineState>, call_id: i32, muted: bool) -> Result<(), String> {
    state
        .0
        .send_command(EngineCommand::SetMute { call_id, muted })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn send_dtmf(state: State<EngineState>, call_id: i32, digits: String) -> Result<(), String> {
    validate_no_nul("digits", &digits)?;
    state
        .0
        .send_command(EngineCommand::SendDtmf { call_id, digits })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn blind_transfer(state: State<EngineState>, call_id: i32, target: String) -> Result<(), String> {
    validate_no_nul("target", &target)?;
    state
        .0
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
        .0
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
        .0
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
        .0
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
    let body = serde_json::json!({
        "sip_uri": input.sip_uri,
        "password": input.password,
    });

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

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

    req = req.header("Content-Type", "application/json");

    if let Some(token) = &input.token {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    if let Some(body) = &input.body {
        req = req.body(body.to_string());
    }

    let response = req.send().await.map_err(|e| format!("Network error: {}", e))?;
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
    runtime: State<SipRuntimeState>,
    uri: String,
) -> Result<(), String> {
    validate_no_nul("uri", &uri)?;
    let account_id = runtime
        .default_account_id
        .lock()
        .map_err(|e| e.to_string())?
        .unwrap_or(0);

    state
        .0
        .send_command(EngineCommand::MakeVideoCall { account_id, uri })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_video(state: State<EngineState>, call_id: i32, enabled: bool) -> Result<(), String> {
    state
        .0
        .send_command(EngineCommand::ToggleVideo { call_id, enabled })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn start_screen_share(state: State<EngineState>, call_id: i32, enabled: bool) -> Result<(), String> {
    // Screen sharing uses the same video toggle mechanism in PJSIP
    // When enabled=true, PJSIP switches the video capture device to desktop capture
    // When enabled=false, switches back to camera
    state
        .0
        .send_command(EngineCommand::ToggleVideo { call_id, enabled })
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
fn list_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
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
        PaleEvent::RegistrationState { account_id, .. } => {
            if let Ok(mut default_account_id) = runtime.default_account_id.lock() {
                *default_account_id = Some(*account_id);
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
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
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
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

// ─── App Entry ───

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Initialize PJSIP engine
    let engine = Arc::new(PjsipEngine::new().expect("Failed to initialize PJSIP engine"));

    let engine_for_bridge = engine.clone();
    let runtime = Arc::new(SipRuntimeState {
        default_account_id: Mutex::new(None),
    });
    let runtime_for_bridge = runtime.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(EngineState(engine))
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            // SIP commands
            register_account,
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
                .expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data).ok();

            // Load persisted config
            let config_path = app_data.join("config.json");
            let config = load_config(&config_path);
            log::info!("Config loaded from {:?}", config_path);
            app.manage(ConfigState {
                config: Mutex::new(config),
                path: config_path,
            });

            // Initialize call history database
            let db_path = app_data.join("call_history.db");
            let history_db = Arc::new(Mutex::new(
                CallHistoryDb::open(&db_path).expect("Failed to open call history database"),
            ));
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

            // Close-to-tray: hide window instead of quitting
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            start_event_bridge(
                app.handle().clone(),
                engine_for_bridge,
                history_db,
                call_tracker,
                runtime_for_bridge,
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
