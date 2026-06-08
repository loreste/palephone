use std::ffi::CString;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::Duration;

use crate::{AppState, MediaConfig, SipDialogStatus, TurnConfig, TurnTransport, UpsertSipDialog};

static APP_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct PjsipRuntimeConfig {
    pub sip_addr: SocketAddr,
    pub enable_udp: bool,
    pub enable_tcp: bool,
    pub tls: Option<TlsConfig>,
    pub require_srtp: bool,
    pub media: MediaConfig,
}

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub port: u16,
    pub cert_file: String,
    pub privkey_file: String,
    pub ca_list_file: Option<String>,
    pub ca_list_path: Option<String>,
    pub password: Option<String>,
    pub verify_client: bool,
    pub require_client_cert: bool,
}

pub struct PjsipRuntime {
    running: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl PjsipRuntime {
    pub fn start(config: PjsipRuntimeConfig, state: Arc<AppState>) -> Result<Self, String> {
        let _ = APP_STATE.set(state);

        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let (ready_tx, ready_rx) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("pale-server-pjsip".to_string())
            .spawn(move || {
                let result = unsafe { initialize_pjsip(&config) };
                let startup_ok = result.is_ok();
                let _ = ready_tx.send(result);

                if startup_ok {
                    while worker_running.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(100));
                    }
                }

                unsafe {
                    pjsip_sys::pjsua_destroy();
                }
            })
            .map_err(|err| format!("failed to start PJSIP worker: {err}"))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                running,
                worker: Some(worker),
            }),
            Ok(Err(err)) => {
                running.store(false, Ordering::SeqCst);
                let _ = worker.join();
                Err(err)
            }
            Err(err) => {
                running.store(false, Ordering::SeqCst);
                let _ = worker.join();
                Err(format!("PJSIP worker exited before startup: {err}"))
            }
        }
    }
}

impl Drop for PjsipRuntime {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

unsafe fn initialize_pjsip(config: &PjsipRuntimeConfig) -> Result<(), String> {
    status_to_result(pjsip_sys::pjsua_create(), "pjsua_create")?;

    let mut cfg: pjsip_sys::pjsua_config = std::mem::zeroed();
    pjsip_sys::pjsua_config_default(&mut cfg);
    let stun_servers = apply_stun_config(&mut cfg, &config.media)?;
    cfg.cb.on_incoming_call = Some(on_incoming_call);
    cfg.cb.on_call_state = Some(on_call_state);
    cfg.cb.on_call_media_state = Some(on_call_media_state);
    if config.require_srtp {
        cfg.use_srtp = pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_MANDATORY;
        cfg.srtp_secure_signaling = 2;
    } else {
        cfg.use_srtp = pjsip_sys::pjmedia_srtp_use_PJMEDIA_SRTP_DISABLED;
        cfg.srtp_secure_signaling = 0;
    }

    let mut log_cfg: pjsip_sys::pjsua_logging_config = std::mem::zeroed();
    pjsip_sys::pjsua_logging_config_default(&mut log_cfg);
    log_cfg.console_level = 3;
    log_cfg.level = 4;

    let mut media_cfg: pjsip_sys::pjsua_media_config = std::mem::zeroed();
    pjsip_sys::pjsua_media_config_default(&mut media_cfg);
    media_cfg.no_vad = 1;
    let turn_strings = apply_media_config(&mut media_cfg, &config.media)?;

    status_to_result(pjsip_sys::pjsua_init(&cfg, &log_cfg, &media_cfg), "pjsua_init")?;
    drop(stun_servers);
    drop(turn_strings);

    if config.enable_udp {
        create_transport(
            pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_UDP,
            config.sip_addr.port(),
            "UDP",
        )?;
    }

    if config.enable_tcp {
        create_transport(
            pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_TCP,
            config.sip_addr.port(),
            "TCP",
        )?;
    }

    if let Some(tls) = &config.tls {
        create_tls_transport(
            pjsip_sys::pjsip_transport_type_e_PJSIP_TRANSPORT_TLS,
            tls,
            "TLS",
        )?;
    }

    if !config.enable_udp && !config.enable_tcp && config.tls.is_none() {
        return Err("at least one SIP transport must be enabled".to_string());
    }

    status_to_result(pjsip_sys::pjsua_start(), "pjsua_start")?;
    Ok(())
}

unsafe fn apply_stun_config(
    cfg: &mut pjsip_sys::pjsua_config,
    media: &MediaConfig,
) -> Result<Vec<CString>, String> {
    let mut stun_servers = Vec::new();
    for server in media.stun_servers.iter().take(cfg.stun_srv.len()) {
        let value = CString::new(server.as_str())
            .map_err(|_| "PALE_STUN_SERVERS contains an interior NUL byte".to_string())?;
        cfg.stun_srv[stun_servers.len()] = pj_str_from_cstring(&value);
        stun_servers.push(value);
    }
    cfg.stun_srv_cnt = stun_servers.len() as u32;
    cfg.stun_ignore_failure = media.stun_ignore_failure as pjsip_sys::pj_bool_t;
    Ok(stun_servers)
}

unsafe fn apply_media_config(
    media_cfg: &mut pjsip_sys::pjsua_media_config,
    media: &MediaConfig,
) -> Result<Vec<CString>, String> {
    media_cfg.enable_ice = media.ice_enabled as pjsip_sys::pj_bool_t;
    let mut strings = Vec::new();

    if let Some(turn) = &media.turn {
        media_cfg.enable_turn = 1;
        let turn_server = push_pj_string(&mut strings, &turn.server, "PALE_TURN_SERVER")?;
        media_cfg.turn_server = turn_server;
        media_cfg.turn_conn_type = match turn.transport {
            TurnTransport::Udp => pjsip_sys::pj_turn_tp_type_PJ_TURN_TP_UDP,
            TurnTransport::Tcp => pjsip_sys::pj_turn_tp_type_PJ_TURN_TP_TCP,
            TurnTransport::Tls => pjsip_sys::pj_turn_tp_type_PJ_TURN_TP_TLS,
        };
        apply_turn_credentials(media_cfg, &mut strings, turn)?;
    }

    Ok(strings)
}

unsafe fn apply_turn_credentials(
    media_cfg: &mut pjsip_sys::pjsua_media_config,
    strings: &mut Vec<CString>,
    turn: &TurnConfig,
) -> Result<(), String> {
    let Some(username) = &turn.username else {
        return Ok(());
    };
    let Some(password) = &turn.password else {
        return Ok(());
    };

    let username = push_pj_string(strings, username, "PALE_TURN_USERNAME")?;
    let password = push_pj_string(strings, password, "PALE_TURN_PASSWORD")?;
    let realm = match &turn.realm {
        Some(realm) => push_pj_string(strings, realm, "PALE_TURN_REALM")?,
        None => pjsip_sys::pj_str_t {
            ptr: std::ptr::null_mut(),
            slen: 0,
        },
    };

    media_cfg.turn_auth_cred.type_ = pjsip_sys::pj_stun_auth_cred_type_PJ_STUN_AUTH_CRED_STATIC;
    media_cfg.turn_auth_cred.data.static_cred =
        pjsip_sys::pj_stun_auth_cred__bindgen_ty_1__bindgen_ty_1 {
            realm,
            username,
            data_type: pjsip_sys::pj_stun_passwd_type_PJ_STUN_PASSWD_PLAIN,
            data: password,
            nonce: pjsip_sys::pj_str_t {
                ptr: std::ptr::null_mut(),
                slen: 0,
            },
        };
    Ok(())
}

fn push_pj_string(
    strings: &mut Vec<CString>,
    value: &str,
    env_name: &str,
) -> Result<pjsip_sys::pj_str_t, String> {
    let value = CString::new(value).map_err(|_| format!("{env_name} contains an interior NUL byte"))?;
    strings.push(value);
    Ok(unsafe { pj_str_from_cstring(strings.last().expect("just pushed")) })
}

unsafe fn create_transport(
    transport_type: pjsip_sys::pjsip_transport_type_e,
    port: u16,
    label: &str,
) -> Result<(), String> {
    let mut transport_cfg: pjsip_sys::pjsua_transport_config = std::mem::zeroed();
    pjsip_sys::pjsua_transport_config_default(&mut transport_cfg);
    transport_cfg.port = port as u32;

    status_to_result(
        pjsip_sys::pjsua_transport_create(
            transport_type,
            &transport_cfg,
            std::ptr::null_mut(),
        ),
        &format!("{label} transport on port {port}"),
    )
}

unsafe fn create_tls_transport(
    transport_type: pjsip_sys::pjsip_transport_type_e,
    tls: &TlsConfig,
    label: &str,
) -> Result<(), String> {
    let cert_file = CString::new(tls.cert_file.as_str())
        .map_err(|_| "PALE_SIP_TLS_CERT contains an interior NUL byte".to_string())?;
    let privkey_file = CString::new(tls.privkey_file.as_str())
        .map_err(|_| "PALE_SIP_TLS_KEY contains an interior NUL byte".to_string())?;
    let ca_list_file = tls
        .ca_list_file
        .as_deref()
        .map(CString::new)
        .transpose()
        .map_err(|_| "PALE_SIP_TLS_CA_FILE contains an interior NUL byte".to_string())?;
    let ca_list_path = tls
        .ca_list_path
        .as_deref()
        .map(CString::new)
        .transpose()
        .map_err(|_| "PALE_SIP_TLS_CA_PATH contains an interior NUL byte".to_string())?;
    let password = tls
        .password
        .as_deref()
        .map(CString::new)
        .transpose()
        .map_err(|_| "PALE_SIP_TLS_KEY_PASSWORD contains an interior NUL byte".to_string())?;

    let mut transport_cfg: pjsip_sys::pjsua_transport_config = std::mem::zeroed();
    pjsip_sys::pjsua_transport_config_default(&mut transport_cfg);
    transport_cfg.port = tls.port as u32;
    transport_cfg.tls_setting.cert_file = pj_str_from_cstring(&cert_file);
    transport_cfg.tls_setting.privkey_file = pj_str_from_cstring(&privkey_file);
    transport_cfg.tls_setting.verify_client = tls.verify_client as pjsip_sys::pj_bool_t;
    transport_cfg.tls_setting.require_client_cert =
        tls.require_client_cert as pjsip_sys::pj_bool_t;

    if let Some(value) = &ca_list_file {
        transport_cfg.tls_setting.ca_list_file = pj_str_from_cstring(value);
    }
    if let Some(value) = &ca_list_path {
        transport_cfg.tls_setting.ca_list_path = pj_str_from_cstring(value);
    }
    if let Some(value) = &password {
        transport_cfg.tls_setting.password = pj_str_from_cstring(value);
    }

    status_to_result(
        pjsip_sys::pjsua_transport_create(
            transport_type,
            &transport_cfg,
            std::ptr::null_mut(),
        ),
        &format!("{label} transport on port {}", tls.port),
    )
}

fn status_to_result(status: i32, operation: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed with PJSIP status {status}"))
    }
}

unsafe extern "C" fn on_incoming_call(
    _acc_id: pjsip_sys::pjsua_acc_id,
    call_id: pjsip_sys::pjsua_call_id,
    _rdata: *mut pjsip_sys::pjsip_rx_data,
) {
    record_call_state(call_id, SipDialogStatus::Ringing);
    let status = pjsip_sys::pjsua_call_answer(call_id, 180, std::ptr::null(), std::ptr::null());
    if status != 0 {
        log::warn!("failed to send 180 Ringing for PJSIP call {call_id}: status={status}");
    }
}

unsafe extern "C" fn on_call_state(
    call_id: pjsip_sys::pjsua_call_id,
    _event: *mut pjsip_sys::pjsip_event,
) {
    let mut call_info: pjsip_sys::pjsua_call_info = std::mem::zeroed();
    let status = pjsip_sys::pjsua_call_get_info(call_id, &mut call_info);
    if status != 0 {
        log::warn!("failed to read PJSIP call {call_id} state: status={status}");
        return;
    }

    let dialog_status = match call_info.state {
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_CALLING
        | pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_INCOMING
        | pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_EARLY => SipDialogStatus::Ringing,
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_CONNECTING
        | pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_CONFIRMED => SipDialogStatus::Routing,
        pjsip_sys::pjsip_inv_state_PJSIP_INV_STATE_DISCONNECTED => SipDialogStatus::Ended,
        _ => SipDialogStatus::Routing,
    };

    record_call_info(call_id, &call_info, dialog_status);
}

unsafe extern "C" fn on_call_media_state(call_id: pjsip_sys::pjsua_call_id) {
    let mut call_info: pjsip_sys::pjsua_call_info = std::mem::zeroed();
    let status = pjsip_sys::pjsua_call_get_info(call_id, &mut call_info);
    if status != 0 {
        log::warn!("failed to read PJSIP call {call_id} media state: status={status}");
        return;
    }

    if call_info.media_status == pjsip_sys::pjsua_call_media_status_PJSUA_CALL_MEDIA_ACTIVE {
        let _ = pjsip_sys::pjsua_conf_connect(call_info.conf_slot, 0);
        let _ = pjsip_sys::pjsua_conf_connect(0, call_info.conf_slot);
    }
}

unsafe fn record_call_state(call_id: pjsip_sys::pjsua_call_id, status: SipDialogStatus) {
    let mut call_info: pjsip_sys::pjsua_call_info = std::mem::zeroed();
    if pjsip_sys::pjsua_call_get_info(call_id, &mut call_info) == 0 {
        record_call_info(call_id, &call_info, status);
    }
}

unsafe fn record_call_info(
    call_id: pjsip_sys::pjsua_call_id,
    call_info: &pjsip_sys::pjsua_call_info,
    status: SipDialogStatus,
) {
    let Some(state) = APP_STATE.get() else {
        return;
    };

    let remote_info = pj_str_to_string(&call_info.remote_info);
    let local_info = pj_str_to_string(&call_info.local_info);
    let remote_uri = parse_sip_identity(&remote_info);
    let local_uri = parse_sip_identity(&local_info);
    let call_key = pj_str_to_string(&call_info.call_id);
    let call_key = if call_key.is_empty() {
        format!("pjsip:{call_id}")
    } else {
        call_key
    };

    let (from_uri, to_uri) = if call_info.role == pjsip_sys::pjsip_role_e_PJSIP_ROLE_UAS {
        (remote_uri, local_uri)
    } else {
        (local_uri, remote_uri)
    };

    state.upsert_sip_dialog(UpsertSipDialog {
        call_id: call_key,
        from_uri,
        to_uri,
        target_contact: None,
        status,
        media_types: vec![],
    });
}

unsafe fn pj_str_to_string(pj: &pjsip_sys::pj_str_t) -> String {
    if pj.ptr.is_null() || pj.slen <= 0 {
        return String::new();
    }

    let slice = std::slice::from_raw_parts(pj.ptr as *const u8, pj.slen as usize);
    String::from_utf8_lossy(slice).to_string()
}

unsafe fn pj_str_from_cstring(value: &CString) -> pjsip_sys::pj_str_t {
    pjsip_sys::pj_str_t {
        ptr: value.as_ptr() as *mut _,
        slen: value.as_bytes().len() as _,
    }
}

fn parse_sip_identity(identity: &str) -> String {
    let identity = identity.trim();
    identity
        .find('<')
        .and_then(|start| identity[start..].find('>').map(|end| identity[start + 1..start + end].to_string()))
        .unwrap_or_else(|| identity.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_sip_identity;

    #[test]
    fn parses_angle_bracket_sip_identity() {
        assert_eq!(
            parse_sip_identity("\"Alice\" <sip:alice@example.com>"),
            "sip:alice@example.com"
        );
    }

    #[test]
    fn preserves_plain_sip_identity() {
        assert_eq!(
            parse_sip_identity("sip:bob@example.com"),
            "sip:bob@example.com"
        );
    }
}
