use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use tokio::sync::broadcast;

use crate::error::{PaleError, PaleResult};
use crate::events::PaleEvent;
use crate::types::*;

/// Global event sender — used by PJSIP C callbacks to emit events.
/// PJSIP callbacks are `extern "C"` and cannot capture state,
/// so we use a global sender.
static EVENT_TX: OnceLock<broadcast::Sender<PaleEvent>> = OnceLock::new();

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
    pub fn new() -> PaleResult<Self> {
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
                Self::worker_thread(cmd_rx, running_clone);
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
            cfg.cb.on_reg_state2 = Some(on_reg_state2);
            cfg.cb.on_incoming_call = Some(on_incoming_call);
            cfg.cb.on_call_state = Some(on_call_state);
            cfg.cb.on_call_media_state = Some(on_call_media_state);

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

            let status = pjsip_sys::pjsua_init(&cfg, &log_cfg, &media_cfg);
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
            tp_cfg.port = 5060;
            let status = pjsip_sys::pjsua_transport_create(
                pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_UDP,
                &tp_cfg,
                std::ptr::null_mut(),
            );
            if status == 0 {
                log::info!("UDP transport created on port 5060");
            }

            // Add TCP transport (port 5060)
            pjsip_sys::pjsua_transport_config_default(&mut tp_cfg);
            tp_cfg.port = 5060;
            let status = pjsip_sys::pjsua_transport_create(
                pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_TCP,
                &tp_cfg,
                std::ptr::null_mut(),
            );
            if status == 0 {
                log::info!("TCP transport created on port 5060");
            }

            // Add TLS transport (port 5061) for encrypted signaling
            pjsip_sys::pjsua_transport_config_default(&mut tp_cfg);
            tp_cfg.port = 5061;
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
                Ok(cmd) => {
                    match cmd {
                        EngineCommand::Shutdown => break,
                        EngineCommand::AddAccount(config) => {
                            Self::handle_add_account(config);
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
                        EngineCommand::AttendedTransfer { call_id, target_call_id } => {
                            Self::handle_attended_transfer(call_id, target_call_id);
                        }
                        EngineCommand::MakeVideoCall { account_id, uri } => {
                            Self::handle_make_video_call(account_id, &uri);
                        }
                        EngineCommand::ToggleVideo { call_id, enabled } => {
                            Self::handle_toggle_video(call_id, enabled);
                        }
                        EngineCommand::ListAudioDevices => {
                            Self::handle_list_audio_devices();
                        }
                    }
                }
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

    fn handle_add_account(config: SipAccountConfig) {
        unsafe {
            let mut acc_cfg: pjsip_sys::pjsua_acc_config = std::mem::zeroed();
            pjsip_sys::pjsua_acc_config_default(&mut acc_cfg);

            let id_str = CString::new(format!(
                "\"{}\" <sip:{}>",
                config.display_name, config.sip_uri
            ))
            .unwrap();
            acc_cfg.id = pj_str_from_cstring(&id_str);

            let reg_uri = CString::new(format!("sip:{}", config.registrar_uri)).unwrap();
            acc_cfg.reg_uri = pj_str_from_cstring(&reg_uri);

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

            // Set transport based on config
            match config.transport {
                Transport::Tls => {
                    // Use sips: URI for TLS registration
                    let reg_uri_tls =
                        CString::new(format!("sips:{}", config.registrar_uri)).unwrap();
                    acc_cfg.reg_uri = pj_str_from_cstring(&reg_uri_tls);
                }
                Transport::Tcp => {
                    // Append ;transport=tcp to the registrar URI
                    let reg_uri_tcp = CString::new(format!(
                        "sip:{};transport=tcp",
                        config.registrar_uri
                    ))
                    .unwrap();
                    acc_cfg.reg_uri = pj_str_from_cstring(&reg_uri_tcp);
                }
                Transport::Udp => {
                    // Default — already set above
                }
            }

            // Enable SRTP for media encryption
            // PJSUA_DEFAULT_SRTP_SECURE_SIGNALING = 0 means SRTP is optional
            // Set to 1 to require secure signaling (TLS) for SRTP
            acc_cfg.use_srtp = pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_OPTIONAL;
            acc_cfg.srtp_secure_signaling = match config.transport {
                Transport::Tls => 1, // Require TLS for SRTP when using TLS
                _ => 0,              // SRTP optional without TLS signaling
            };

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
            let status =
                pjsip_sys::pjsua_call_answer(call_id, code as u32, std::ptr::null(), std::ptr::null());
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Failed to answer call {}: status={}",
                        call_id, status
                    ),
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
            let status =
                pjsip_sys::pjsua_call_xfer(call_id, &mut target_pj, std::ptr::null());
            if status != 0 {
                emit_event(PaleEvent::Error {
                    message: format!(
                        "Transfer failed for call {}: status={}",
                        call_id, status
                    ),
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

            let status = pjsip_sys::pjsua_call_reinvite2(
                call_id,
                &opt,
                std::ptr::null(),
            );

            if status != 0 {
                log::warn!("Failed to toggle video for call {}: status={}", call_id, status);
            }
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

// ─── PJSIP C Callbacks ───

/// Helper: create a pj_str_t from a CString (borrows the CString's data)
unsafe fn pj_str_from_cstring(s: &CString) -> pjsip_sys::pj_str_t {
    pjsip_sys::pj_str_t {
        ptr: s.as_ptr() as *mut i8,
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
                let slice = std::slice::from_raw_parts(
                    reason_pj.ptr as *const u8,
                    reason_pj.slen as usize,
                );
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
            identity[start..].find('>').map(|end| {
                identity[start + 1..start + end].to_string()
            })
        })
        .unwrap_or_else(|| identity.to_string());

    (name, uri)
}

#[cfg(test)]
mod tests {
    use super::*;

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
