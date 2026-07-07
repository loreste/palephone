use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use tokio::sync::broadcast;

use crate::config::{NetworkPersist, SrtpMode};
use crate::error::{PaleError, PaleResult};
use crate::events::PaleEvent;
use crate::types::*;

/// Global event sender — used by PJSIP C callbacks to emit events.
/// PJSIP callbacks are `extern "C"` and cannot capture state,
/// so we use a global sender.
static EVENT_TX: OnceLock<broadcast::Sender<PaleEvent>> = OnceLock::new();

/// Active recorders: call_id → (recorder_id, file_path)
static ACTIVE_RECORDERS: std::sync::LazyLock<
    Mutex<std::collections::HashMap<CallId, (pjsip_sys::pjsua_recorder_id, String)>>,
> = std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Send an event from PJSIP callbacks
fn emit_event(event: PaleEvent) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.send(event);
    }
}

/// Commands sent to the PJSIP worker thread
#[derive(Debug)]
pub enum EngineCommand {
    AddAccount(SipAccountConfig),
    RemoveAccount(AccountId),
    MakeCall {
        account_id: AccountId,
        uri: String,
    },
    AnswerCall {
        call_id: CallId,
        code: u16,
    },
    HangupCall(CallId),
    HoldCall(CallId),
    UnholdCall(CallId),
    SetMute {
        call_id: CallId,
        muted: bool,
    },
    SendDtmf {
        call_id: CallId,
        digits: String,
    },
    BlindTransfer {
        call_id: CallId,
        target: String,
    },
    AttendedTransfer {
        call_id: CallId,
        target_call_id: CallId,
    },
    MakeVideoCall {
        account_id: AccountId,
        uri: String,
    },
    ToggleVideo {
        call_id: CallId,
        enabled: bool,
    },
    StartRecording {
        call_id: CallId,
        file_path: String,
    },
    StopRecording {
        call_id: CallId,
    },
    ListAudioDevices,
    Shutdown,
}

/// The PJSIP engine manages PJSIP on a dedicated OS thread.
pub struct PjsipEngine {
    cmd_tx: std::sync::mpsc::Sender<EngineCommand>,
    event_tx: broadcast::Sender<PaleEvent>,
    running: Arc<AtomicBool>,
    thread_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl PjsipEngine {
    /// Create and start the PJSIP engine.
    pub fn new(network: NetworkPersist) -> PaleResult<Self> {
        let (event_tx, _) = broadcast::channel(256);
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));

        // Store event sender globally for callbacks
        EVENT_TX
            .set(event_tx.clone())
            .map_err(|_| PaleError::AlreadyRunning)?;

        let running_clone = running.clone();

        let handle = thread::Builder::new()
            .name("pjsip-worker".into())
            .spawn(move || {
                Self::worker_thread(cmd_rx, running_clone, network);
            })
            .map_err(|e| PaleError::Thread(e.to_string()))?;

        Ok(Self {
            cmd_tx,
            event_tx,
            running,
            thread_handle: Mutex::new(Some(handle)),
        })
    }

    /// Subscribe to engine events
    pub fn subscribe(&self) -> broadcast::Receiver<PaleEvent> {
        self.event_tx.subscribe()
    }

    /// Send a command to the PJSIP worker thread
    pub fn send_command(&self, cmd: EngineCommand) -> PaleResult<()> {
        self.cmd_tx
            .send(cmd)
            .map_err(|e| PaleError::ChannelSend(e.to_string()))
    }

    /// Check if the engine is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Shut down the engine
    pub fn shutdown(&self) -> PaleResult<()> {
        self.running.store(false, Ordering::SeqCst);
        self.send_command(EngineCommand::Shutdown)?;
        if let Ok(mut handle) = self.thread_handle.lock() {
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
        Ok(())
    }

    /// The PJSIP worker thread function
    fn worker_thread(
        cmd_rx: std::sync::mpsc::Receiver<EngineCommand>,
        running: Arc<AtomicBool>,
        network: NetworkPersist,
    ) {
        log::info!("PJSIP worker thread starting...");

        // Initialize PJSIP
        unsafe {
            let status = pjsip_sys::pjsua_create();
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("pjsua_create failed with status {}", status),
                });
                return;
            }

            // Configure pjsua with our callbacks
            let mut cfg: pjsip_sys::pjsua_config = std::mem::zeroed();
            pjsip_sys::pjsua_config_default(&mut cfg);
            let stun_strings = match apply_stun_config(&mut cfg, &network) {
                Ok(strings) => strings,
                Err(message) => {
                    emit_event(PaleEvent::Error { message });
                    pjsip_sys::pjsua_destroy();
                    return;
                }
            };
            cfg.cb.on_reg_state2 = Some(on_reg_state2);
            cfg.cb.on_incoming_call = Some(on_incoming_call);
            cfg.cb.on_call_state = Some(on_call_state);
            cfg.cb.on_call_media_state = Some(on_call_media_state);
            let (use_srtp, secure_signaling) = srtp_policy(network.srtp_mode, false);
            cfg.use_srtp = use_srtp;
            cfg.srtp_secure_signaling = secure_signaling;

            // Logging config — reduce verbosity
            let mut log_cfg: pjsip_sys::pjsua_logging_config = std::mem::zeroed();
            pjsip_sys::pjsua_logging_config_default(&mut log_cfg);
            log_cfg.console_level = 3;
            log_cfg.level = 4;

            // Media config
            let mut media_cfg: pjsip_sys::pjsua_media_config = std::mem::zeroed();
            pjsip_sys::pjsua_media_config_default(&mut media_cfg);
            media_cfg.ec_tail_len = 256;
            media_cfg.no_vad = 1;
            let turn_strings = match apply_media_config(&mut media_cfg, &network) {
                Ok(strings) => strings,
                Err(message) => {
                    emit_event(PaleEvent::Error { message });
                    pjsip_sys::pjsua_destroy();
                    return;
                }
            };

            let status = pjsip_sys::pjsua_init(&cfg, &log_cfg, &media_cfg);
            drop(stun_strings);
            drop(turn_strings);
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("pjsua_init failed with status {}", status),
                });
                pjsip_sys::pjsua_destroy();
                return;
            }

            // Add UDP transport (port 5060)
            let mut tp_cfg: pjsip_sys::pjsua_transport_config = std::mem::zeroed();
            pjsip_sys::pjsua_transport_config_default(&mut tp_cfg);
            tp_cfg.port = network.sip_port as u32;
            let status = pjsip_sys::pjsua_transport_create(
                pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_UDP,
                &tp_cfg,
                std::ptr::null_mut(),
            );
            if status == 0 {
                log::info!("UDP transport created on port {}", network.sip_port);
            }

            // Add TCP transport
            pjsip_sys::pjsua_transport_config_default(&mut tp_cfg);
            tp_cfg.port = network.sip_port as u32;
            let status = pjsip_sys::pjsua_transport_create(
                pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_TCP,
                &tp_cfg,
                std::ptr::null_mut(),
            );
            if status == 0 {
                log::info!("TCP transport created on port {}", network.sip_port);
            }

            // Add TLS transport (port 5061) for encrypted signaling
            pjsip_sys::pjsua_transport_config_default(&mut tp_cfg);
            tp_cfg.port = network.sip_port.saturating_add(1) as u32;
            let status = pjsip_sys::pjsua_transport_create(
                pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_TLS,
                &tp_cfg,
                std::ptr::null_mut(),
            );
            if status == 0 {
                log::info!("TLS transport created on port 5061");
            } else {
                log::warn!("TLS transport creation failed (status={}). SIP signaling encryption unavailable.", status);
            }

            // Start pjsua
            let status = pjsip_sys::pjsua_start();
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("pjsua_start failed with status {}", status),
                });
                pjsip_sys::pjsua_destroy();
                return;
            }

            log::info!("PJSIP initialized and started successfully.");
        }

        // Command loop
        while running.load(Ordering::Relaxed) {
            match cmd_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(cmd) => match cmd {
                    EngineCommand::Shutdown => break,
                    EngineCommand::AddAccount(config) => {
                        Self::handle_add_account(config, &network);
                    }
                    EngineCommand::RemoveAccount(id) => {
                        Self::handle_remove_account(id);
                    }
                    EngineCommand::MakeCall { account_id, uri } => {
                        Self::handle_make_call(account_id, &uri);
                    }
                    EngineCommand::AnswerCall { call_id, code } => {
                        Self::handle_answer_call(call_id, code);
                    }
                    EngineCommand::HangupCall(call_id) => {
                        Self::handle_hangup(call_id);
                    }
                    EngineCommand::HoldCall(call_id) => {
                        Self::handle_hold(call_id, true);
                    }
                    EngineCommand::UnholdCall(call_id) => {
                        Self::handle_hold(call_id, false);
                    }
                    EngineCommand::SetMute { call_id, muted } => {
                        Self::handle_mute(call_id, muted);
                    }
                    EngineCommand::SendDtmf { call_id, digits } => {
                        Self::handle_dtmf(call_id, &digits);
                    }
                    EngineCommand::BlindTransfer { call_id, target } => {
                        Self::handle_blind_transfer(call_id, &target);
                    }
                    EngineCommand::AttendedTransfer {
                        call_id,
                        target_call_id,
                    } => {
                        Self::handle_attended_transfer(call_id, target_call_id);
                    }
                    EngineCommand::MakeVideoCall { account_id, uri } => {
                        Self::handle_make_video_call(account_id, &uri);
                    }
                    EngineCommand::ToggleVideo { call_id, enabled } => {
                        Self::handle_toggle_video(call_id, enabled);
                    }
                    EngineCommand::StartRecording { call_id, file_path } => {
                        Self::handle_start_recording(call_id, &file_path);
                    }
                    EngineCommand::StopRecording { call_id } => {
                        Self::handle_stop_recording(call_id);
                    }
                    EngineCommand::ListAudioDevices => {
                        Self::handle_list_audio_devices();
                    }
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // No command — continue polling
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        // Shutdown PJSIP
        log::info!("Shutting down PJSIP...");
        unsafe {
            pjsip_sys::pjsua_destroy();
        }
        log::info!("PJSIP shut down.");
    }

    // ─── Command Handlers ───

    fn handle_add_account(config: SipAccountConfig, network: &NetworkPersist) {
        unsafe {
            let mut acc_cfg: pjsip_sys::pjsua_acc_config = std::mem::zeroed();
            pjsip_sys::pjsua_acc_config_default(&mut acc_cfg);

            let sip_uri =
                if config.sip_uri.starts_with("sip:") || config.sip_uri.starts_with("sips:") {
                    config.sip_uri.clone()
                } else {
                    format!("sip:{}", config.sip_uri)
                };
            let id_str =
                CString::new(format!("\"{}\" <{}>", config.display_name, sip_uri)).unwrap();
            acc_cfg.id = pj_str_from_cstring(&id_str);

            acc_cfg.cred_count = 1;
            let realm = CString::new("*").unwrap();
            acc_cfg.cred_info[0].realm = pj_str_from_cstring(&realm);
            let scheme = CString::new("digest").unwrap();
            acc_cfg.cred_info[0].scheme = pj_str_from_cstring(&scheme);
            let username = CString::new(config.auth_username.as_str()).unwrap();
            acc_cfg.cred_info[0].username = pj_str_from_cstring(&username);
            let data_type = 0; // plaintext
            acc_cfg.cred_info[0].data_type = data_type;
            let password = CString::new(config.auth_password.as_str()).unwrap();
            acc_cfg.cred_info[0].data = pj_str_from_cstring(&password);

            acc_cfg.reg_timeout = config.reg_expiry;

            // Set transport based on config. Keep the CString alive until pjsua_acc_add.
            let bare_registrar = config
                .registrar_uri
                .strip_prefix("sips:")
                .or_else(|| config.registrar_uri.strip_prefix("sip:"))
                .unwrap_or(&config.registrar_uri);
            let reg_uri_str = match config.transport {
                Transport::Tls => format!("sips:{}", bare_registrar),
                Transport::Tcp => format!("sip:{};transport=tcp", bare_registrar),
                Transport::Udp => format!("sip:{}", bare_registrar),
            };
            let reg_uri = CString::new(reg_uri_str).unwrap();
            acc_cfg.reg_uri = pj_str_from_cstring(&reg_uri);

            let (rtp_port, rtp_port_range) =
                normalize_rtp_port_range(network.rtp_port_min, network.rtp_port_max);
            acc_cfg.rtp_cfg.port = rtp_port as u32;
            acc_cfg.rtp_cfg.port_range = rtp_port_range as u32;
            acc_cfg.rtp_cfg.randomize_port = 1;

            let (use_srtp, secure_signaling) = srtp_policy(
                network.srtp_mode,
                matches!(config.transport, Transport::Tls),
            );
            acc_cfg.use_srtp = use_srtp;
            acc_cfg.srtp_secure_signaling = secure_signaling;

            let mut acc_id: pjsip_sys::pjsua_acc_id = -1;
            let status = pjsip_sys::pjsua_acc_add(&acc_cfg, 1, &mut acc_id);

            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Failed to add account: status={}", status),
                });
            } else {
                log::info!("Account added with id={}", acc_id);
            }
        }
    }

    fn handle_remove_account(acc_id: AccountId) {
        unsafe {
            let status = pjsip_sys::pjsua_acc_del(acc_id);
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Failed to remove account {}: status={}", acc_id, status),
                });
            }
        }
    }

    fn handle_make_call(acc_id: AccountId, uri: &str) {
        unsafe {
            let dest = CString::new(uri).unwrap();
            let mut dest_pj = pj_str_from_cstring(&dest);
            let mut call_id: pjsip_sys::pjsua_call_id = -1;

            let status = pjsip_sys::pjsua_call_make_call(
                acc_id,
                &mut dest_pj,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                &mut call_id,
            );

            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Failed to make call to {}: status={}", uri, status),
                });
            } else {
                log::info!("Call initiated: call_id={}", call_id);
            }
        }
    }

    fn handle_answer_call(call_id: CallId, code: u16) {
        unsafe {
            let status = pjsip_sys::pjsua_call_answer(
                call_id,
                code as u32,
                std::ptr::null(),
                std::ptr::null(),
            );
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Failed to answer call {}: status={}", call_id, status),
                });
            }
        }
    }

    fn handle_hangup(call_id: CallId) {
        unsafe {
            let status =
                pjsip_sys::pjsua_call_hangup(call_id, 0, std::ptr::null(), std::ptr::null());
            if status != 0 {
                log::warn!("Failed to hangup call {}: status={}", call_id, status);
            }
        }
    }

    fn handle_hold(call_id: CallId, hold: bool) {
        unsafe {
            let status = if hold {
                pjsip_sys::pjsua_call_set_hold(call_id, std::ptr::null())
            } else {
                pjsip_sys::pjsua_call_reinvite(call_id, 1, std::ptr::null())
            };
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Failed to {} call {}: status={}",
                        if hold { "hold" } else { "unhold" },
                        call_id,
                        status
                    ),
                });
            }
        }
    }

    fn handle_mute(_call_id: CallId, muted: bool) {
        unsafe {
            // Connect/disconnect the mic port from the conference bridge
            // Port 0 is the sound device
            let conf_port = 0;
            if muted {
                pjsip_sys::pjsua_conf_disconnect(conf_port, 0);
            } else {
                pjsip_sys::pjsua_conf_connect(conf_port, 0);
            }
        }
    }

    fn handle_dtmf(call_id: CallId, digits: &str) {
        unsafe {
            let digits_c = CString::new(digits).unwrap();
            let mut digits_pj = pj_str_from_cstring(&digits_c);
            let status = pjsip_sys::pjsua_call_dial_dtmf(call_id, &mut digits_pj);
            if status != 0 {
                log::warn!("DTMF send failed for call {}: status={}", call_id, status);
            }
        }
    }

    fn handle_blind_transfer(call_id: CallId, target: &str) {
        unsafe {
            let target_c = CString::new(target).unwrap();
            let mut target_pj = pj_str_from_cstring(&target_c);
            let status = pjsip_sys::pjsua_call_xfer(call_id, &mut target_pj, std::ptr::null());
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Transfer failed for call {}: status={}", call_id, status),
                });
            }
        }
    }

    fn handle_attended_transfer(call_id: CallId, target_call_id: CallId) {
        unsafe {
            // pjsua_call_xfer_replaces sends a REFER with Replaces header
            // This connects the two remote parties (call_id's remote ↔ target_call_id's remote)
            // and disconnects the local endpoint from both calls
            let options = 0_u32; // PJSUA_XFER_NO_REQUIRE_REPLACES = 0
            let status = pjsip_sys::pjsua_call_xfer_replaces(
                call_id,
                target_call_id,
                options,
                std::ptr::null(),
            );
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Attended transfer failed for calls {}->{}: status={}",
                        call_id, target_call_id, status
                    ),
                });
            }
        }
    }

    fn handle_make_video_call(acc_id: AccountId, uri: &str) {
        unsafe {
            let dest = CString::new(uri).unwrap();
            let mut dest_pj = pj_str_from_cstring(&dest);

            // Create call setting with video enabled
            let mut opt: pjsip_sys::pjsua_call_setting = std::mem::zeroed();
            pjsip_sys::pjsua_call_setting_default(&mut opt);
            opt.vid_cnt = 1; // Enable 1 video stream
            opt.aud_cnt = 1;

            let mut call_id: pjsip_sys::pjsua_call_id = -1;
            let status = pjsip_sys::pjsua_call_make_call(
                acc_id,
                &mut dest_pj,
                &opt,
                std::ptr::null_mut(),
                std::ptr::null(),
                &mut call_id,
            );

            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Failed to make video call to {}: status={}", uri, status),
                });
            } else {
                log::info!("Video call initiated: call_id={}", call_id);
            }
        }
    }

    fn handle_toggle_video(call_id: CallId, enabled: bool) {
        unsafe {
            let mut opt: pjsip_sys::pjsua_call_setting = std::mem::zeroed();
            pjsip_sys::pjsua_call_setting_default(&mut opt);
            opt.vid_cnt = if enabled { 1 } else { 0 };
            opt.aud_cnt = 1;

            let status = pjsip_sys::pjsua_call_reinvite2(call_id, &opt, std::ptr::null());

            if status != 0 {
                log::warn!(
                    "Failed to toggle video for call {}: status={}",
                    call_id,
                    status
                );
            }
        }
    }

    fn handle_start_recording(call_id: CallId, file_path: &str) {
        unsafe {
            // Check if call exists and is active
            if pjsip_sys::pjsua_call_is_active(call_id) == 0 {
                emit_event(PaleEvent::Error {
                    message: format!("Cannot record: call {} is not active", call_id),
                });
                return;
            }

            // Stop any existing recording for this call
            Self::handle_stop_recording(call_id);

            // Create WAV recorder
            let file_c = CString::new(file_path).unwrap();
            let mut file_pj = pj_str_from_cstring(&file_c);
            let mut recorder_id: pjsip_sys::pjsua_recorder_id = -1;

            let status = pjsip_sys::pjsua_recorder_create(
                &mut file_pj,
                0,                    // enc_type: default (WAV)
                std::ptr::null_mut(), // enc_param
                -1,                   // max_size: unlimited
                0,                    // options
                &mut recorder_id,
            );

            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Failed to create recorder for call {}: status={}",
                        call_id, status
                    ),
                });
                return;
            }

            // Get the conference port for this call and the recorder
            let call_conf_port = pjsip_sys::pjsua_call_get_conf_port(call_id);
            let rec_conf_port = pjsip_sys::pjsua_recorder_get_conf_port(recorder_id);

            if call_conf_port < 0 || rec_conf_port < 0 {
                pjsip_sys::pjsua_recorder_destroy(recorder_id);
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Failed to get conference ports for recording call {}",
                        call_id
                    ),
                });
                return;
            }

            // Connect both directions to recorder:
            // 1. Remote party's audio → recorder (what the remote says)
            let status1 = pjsip_sys::pjsua_conf_connect(call_conf_port, rec_conf_port);
            // 2. Local microphone → recorder (what we say) — port 0 is the sound device
            let status2 = pjsip_sys::pjsua_conf_connect(0, rec_conf_port);

            if status1 != 0 || status2 != 0 {
                pjsip_sys::pjsua_recorder_destroy(recorder_id);
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Failed to connect audio to recorder: status1={}, status2={}",
                        status1, status2
                    ),
                });
                return;
            }

            // Store the recorder mapping
            if let Ok(mut recorders) = ACTIVE_RECORDERS.lock() {
                recorders.insert(call_id, (recorder_id, file_path.to_string()));
            }

            log::info!("Recording started for call {} → {}", call_id, file_path);
            emit_event(PaleEvent::RecordingState {
                call_id,
                recording: true,
                file_path: file_path.to_string(),
            });
        }
    }

    fn handle_stop_recording(call_id: CallId) {
        let entry = ACTIVE_RECORDERS
            .lock()
            .ok()
            .and_then(|mut r| r.remove(&call_id));
        if let Some((recorder_id, file_path)) = entry {
            unsafe {
                pjsip_sys::pjsua_recorder_destroy(recorder_id);
            }
            log::info!("Recording stopped for call {} — {}", call_id, file_path);
            emit_event(PaleEvent::RecordingState {
                call_id,
                recording: false,
                file_path,
            });
        }
    }

    fn handle_list_audio_devices() {
        unsafe {
            let count = pjsip_sys::pjmedia_aud_dev_count() as usize;
            for i in 0..count {
                let mut info: pjsip_sys::pjmedia_aud_dev_info = std::mem::zeroed();
                let status = pjsip_sys::pjmedia_aud_dev_get_info(i as i32, &mut info);
                if status == 0 {
                    let name = std::ffi::CStr::from_ptr(info.name.as_ptr())
                        .to_string_lossy()
                        .to_string();
                    log::info!(
                        "Audio device {}: {} (in={}, out={})",
                        i,
                        name,
                        info.input_count,
                        info.output_count
                    );
                }
            }
        }
    }
}

impl Drop for PjsipEngine {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn srtp_policy(mode: SrtpMode, using_tls: bool) -> (pjsip_sys::pjmedia_srtp_use, i32) {
    match mode {
        SrtpMode::Disabled => (pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_DISABLED, 0),
        SrtpMode::Optional => (
            pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_OPTIONAL,
            if using_tls { 1 } else { 0 },
        ),
        SrtpMode::Required => (pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_MANDATORY, 1),
    }
}

fn normalize_rtp_port_range(min: u16, max: u16) -> (u16, u16) {
    if min >= 1024 && max > min {
        (min, max - min)
    } else {
        (10000, 10000)
    }
}

unsafe fn apply_stun_config(
    cfg: &mut pjsip_sys::pjsua_config,
    network: &NetworkPersist,
) -> Result<Vec<CString>, String> {
    let mut stun_servers = Vec::new();
    let stun_server = network.stun_server.trim();
    if !stun_server.is_empty() {
        let value = CString::new(stun_server)
            .map_err(|_| "STUN server contains an interior NUL byte".to_string())?;
        cfg.stun_srv[0] = pj_str_from_cstring(&value);
        stun_servers.push(value);
    }
    cfg.stun_srv_cnt = stun_servers.len() as u32;
    cfg.stun_ignore_failure = 1;
    Ok(stun_servers)
}

unsafe fn apply_media_config(
    media_cfg: &mut pjsip_sys::pjsua_media_config,
    network: &NetworkPersist,
) -> Result<Vec<CString>, String> {
    media_cfg.enable_ice = network.enable_ice as pjsip_sys::pj_bool_t;
    let mut strings = Vec::new();
    let turn_server = network.turn_server.trim();
    if turn_server.is_empty() {
        return Ok(strings);
    }

    media_cfg.enable_turn = 1;
    media_cfg.turn_server = push_pj_string(&mut strings, turn_server, "TURN server")?;
    media_cfg.turn_conn_type = pjsip_sys::pj_turn_tp_type_PJ_TURN_TP_UDP;

    let username = network.turn_username.trim();
    if !username.is_empty() && !network.turn_password.is_empty() {
        let username = push_pj_string(&mut strings, username, "TURN username")?;
        let password = push_pj_string(&mut strings, &network.turn_password, "TURN password")?;
        media_cfg.turn_auth_cred.type_ = pjsip_sys::pj_stun_auth_cred_type_PJ_STUN_AUTH_CRED_STATIC;
        media_cfg.turn_auth_cred.data.static_cred =
            pjsip_sys::pj_stun_auth_cred__bindgen_ty_1__bindgen_ty_1 {
                realm: pjsip_sys::pj_str_t {
                    ptr: std::ptr::null_mut(),
                    slen: 0,
                },
                username,
                data_type: pjsip_sys::pj_stun_passwd_type_PJ_STUN_PASSWD_PLAIN,
                data: password,
                nonce: pjsip_sys::pj_str_t {
                    ptr: std::ptr::null_mut(),
                    slen: 0,
                },
            };
    }

    Ok(strings)
}

fn push_pj_string(
    strings: &mut Vec<CString>,
    value: &str,
    field_name: &str,
) -> Result<pjsip_sys::pj_str_t, String> {
    let value =
        CString::new(value).map_err(|_| format!("{field_name} contains an interior NUL byte"))?;
    strings.push(value);
    Ok(unsafe { pj_str_from_cstring(strings.last().expect("just pushed")) })
}

// ─── PJSIP C Callbacks ───

/// Helper: create a pj_str_t from a CString (borrows the CString's data)
unsafe fn pj_str_from_cstring(s: &CString) -> pjsip_sys::pj_str_t {
    pjsip_sys::pj_str_t {
        ptr: s.as_ptr() as *mut _,
        slen: s.as_bytes().len() as _,
    }
}

/// Registration state callback
unsafe extern "C" fn on_reg_state2(
    acc_id: pjsip_sys::pjsua_acc_id,
    info: *mut pjsip_sys::pjsua_reg_info,
) {
    let mut acc_info: pjsip_sys::pjsua_acc_info = std::mem::zeroed();
    pjsip_sys::pjsua_acc_get_info(acc_id, &mut acc_info);

    let state = if acc_info.status as u32 == 200 {
        RegState::Registered
    } else if acc_info.status as u32 == 0 {
        RegState::Registering
    } else {
        RegState::Unregistered
    };

    let reason = if !info.is_null() {
        let cbparam = (*info).cbparam;
        if !cbparam.is_null() {
            let reason_pj = (*cbparam).reason;
            if !reason_pj.ptr.is_null() && reason_pj.slen > 0 {
                let slice =
                    std::slice::from_raw_parts(reason_pj.ptr as *const u8, reason_pj.slen as usize);
                String::from_utf8_lossy(slice).to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    emit_event(PaleEvent::RegistrationState {
        account_id: acc_id,
        state,
        reason,
    });
}

/// Incoming call callback
unsafe extern "C" fn on_incoming_call(
    acc_id: pjsip_sys::pjsua_acc_id,
    call_id: pjsip_sys::pjsua_call_id,
    _rdata: *mut pjsip_sys::pjsip_rx_data,
) {
    let mut ci: pjsip_sys::pjsua_call_info = std::mem::zeroed();
    pjsip_sys::pjsua_call_get_info(call_id, &mut ci);

    let remote_info = pj_str_to_string(&ci.remote_info);
    // Parse display name and URI from remote_info
    let (name, uri) = parse_sip_identity(&remote_info);

    emit_event(PaleEvent::IncomingCall {
        call_id,
        account_id: acc_id,
        caller_name: name,
        caller_uri: uri,
    });

    // Auto-ring (180 Ringing)
    pjsip_sys::pjsua_call_answer(call_id, 180, std::ptr::null(), std::ptr::null());
}

/// Call state change callback
unsafe extern "C" fn on_call_state(
    call_id: pjsip_sys::pjsua_call_id,
    _e: *mut pjsip_sys::pjsip_event,
) {
    let mut ci: pjsip_sys::pjsua_call_info = std::mem::zeroed();
    pjsip_sys::pjsua_call_get_info(call_id, &mut ci);

    let state = match ci.state {
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_CALLING => CallState::Dialing,
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_INCOMING => CallState::Ringing,
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_EARLY => CallState::EarlyMedia,
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_CONNECTING
        | pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_CONFIRMED => CallState::Connected,
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_DISCONNECTED => CallState::Terminated,
        _ => CallState::Idle,
    };

    let remote = pj_str_to_string(&ci.remote_info);
    let (name, uri) = parse_sip_identity(&remote);

    let direction = if ci.role == pjsip_sys::pjsip_role_e_PJSIP_ROLE_UAS {
        CallDirection::Inbound
    } else {
        CallDirection::Outbound
    };

    emit_event(PaleEvent::CallState {
        call_id,
        state,
        direction,
        remote_uri: uri,
        remote_name: name,
    });
}

/// Call media state callback — connect audio when media is active
unsafe extern "C" fn on_call_media_state(call_id: pjsip_sys::pjsua_call_id) {
    let mut ci: pjsip_sys::pjsua_call_info = std::mem::zeroed();
    pjsip_sys::pjsua_call_get_info(call_id, &mut ci);

    if ci.media_status == pjsip_sys::pjsua_call_media_status_PJSUA_CALL_MEDIA_ACTIVE {
        // Connect call audio to sound device
        pjsip_sys::pjsua_conf_connect(ci.conf_slot, 0);
        pjsip_sys::pjsua_conf_connect(0, ci.conf_slot);
        log::info!("Call {} media active — audio connected", call_id);
    }
}

// ─── Helpers ───

/// Convert pj_str_t to Rust String
unsafe fn pj_str_to_string(pj: &pjsip_sys::pj_str_t) -> String {
    if pj.ptr.is_null() || pj.slen <= 0 {
        return String::new();
    }
    let slice = std::slice::from_raw_parts(pj.ptr as *const u8, pj.slen as usize);
    String::from_utf8_lossy(slice).to_string()
}

/// Parse SIP identity string like `"Alice" <sip:alice@example.com>` into (name, uri)
fn parse_sip_identity(identity: &str) -> (String, String) {
    let identity = identity.trim();

    // Extract display name from quotes
    let name = if identity.starts_with('"') {
        identity
            .find('"')
            .and_then(|start| identity[start + 1..].find('"').map(|end| start + 1 + end))
            .map(|end| identity[1..end].to_string())
            .unwrap_or_default()
    } else {
        // No quotes — name is everything before <
        identity
            .find('<')
            .map(|pos| identity[..pos].trim().to_string())
            .unwrap_or_default()
    };

    // Extract URI from angle brackets
    let uri = identity
        .find('<')
        .and_then(|start| {
            identity[start..]
                .find('>')
                .map(|end| identity[start + 1..start + end].to_string())
        })
        .unwrap_or_else(|| identity.to_string());

    (name, uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_valid_rtp_range() {
        assert_eq!(normalize_rtp_port_range(12000, 14000), (12000, 2000));
    }

    #[test]
    fn falls_back_for_invalid_rtp_range() {
        assert_eq!(normalize_rtp_port_range(20000, 10000), (10000, 10000));
        assert_eq!(normalize_rtp_port_range(80, 10000), (10000, 10000));
    }

    #[test]
    fn maps_srtp_modes() {
        assert_eq!(
            srtp_policy(SrtpMode::Disabled, false),
            (pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_DISABLED, 0)
        );
        assert_eq!(
            srtp_policy(SrtpMode::Optional, true),
            (pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_OPTIONAL, 1)
        );
        assert_eq!(
            srtp_policy(SrtpMode::Required, false),
            (pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_MANDATORY, 1)
        );
    }

    #[test]
    fn test_parse_sip_identity() {
        let (name, uri) = parse_sip_identity("\"Alice Smith\" <sip:alice@example.com>");
        assert_eq!(name, "Alice Smith");
        assert_eq!(uri, "sip:alice@example.com");

        let (name, uri) = parse_sip_identity("<sip:bob@example.com>");
        assert_eq!(name, "");
        assert_eq!(uri, "sip:bob@example.com");

        let (name, uri) = parse_sip_identity("sip:charlie@example.com");
        assert_eq!(name, "");
        assert_eq!(uri, "sip:charlie@example.com");
    }
}
