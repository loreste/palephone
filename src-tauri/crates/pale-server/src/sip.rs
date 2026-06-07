use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use rsip::headers::untyped::UntypedHeader;
use rsip::{Header, Headers, Request, Response, StatusCode, Version};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use uuid::Uuid;

use crate::{
    md5_hex, AppState, MediaKind, PresenceStatus, SipDialogStatus, SipRegistration,
    StoreSipMessage, StoreSipNotification, StoreSipTransaction, UpsertSipDialog,
    UpsertSipSubscription,
};

pub async fn run_udp_server(
    addr: SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !allow_insecure_udp_parser() {
        return Err(
            "UDP SIP parser is insecure and disabled; set PALE_ALLOW_INSECURE_SIP_UDP=1 for development fallback use"
                .into(),
        );
    }

    let socket = Arc::new(UdpSocket::bind(addr).await?);
    let mut buf = vec![0_u8; 8192];

    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        let packet = String::from_utf8_lossy(&buf[..len]).to_string();
        let socket = socket.clone();
        let state = state.clone();
        tokio::spawn(async move {
            let response = handle_udp_packet(&socket, &packet, peer, &state).await;
            if let Some(response) = response {
                let _ = socket.send_to(response.as_bytes(), peer).await;
            }
        });
    }
}

fn allow_insecure_udp_parser() -> bool {
    std::env::var("PALE_ALLOW_INSECURE_SIP_UDP")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

async fn handle_udp_packet(
    socket: &UdpSocket,
    packet: &str,
    peer: SocketAddr,
    state: &AppState,
) -> Option<String> {
    let request = SipRequest::parse(packet)?;
    if request.method() == "INVITE" {
        if let Some(outcome) = proxy_invite(socket, packet, &request, state).await {
            record_transaction(&request, peer, Some(outcome.as_str()), state);
            return Some(outcome);
        }
    }
    if request.method() == "MESSAGE" {
        if let Some(outcome) = relay_message(socket, &request, peer, state).await {
            record_transaction(&request, peer, Some(outcome.as_str()), state);
            return Some(outcome);
        }
    }

    let response = handle_request(&request, peer, state);
    record_transaction(&request, peer, response.as_deref(), state);
    response
}

pub fn handle_packet(packet: &str, peer: SocketAddr, state: &AppState) -> Option<String> {
    let request = SipRequest::parse(packet)?;
    let response = handle_request(&request, peer, state);
    record_transaction(&request, peer, response.as_deref(), state);
    response
}

fn handle_request(request: &SipRequest, peer: SocketAddr, state: &AppState) -> Option<String> {
    match request.method().as_str() {
        "REGISTER" => handle_register(&request, peer, state),
        "INVITE" => handle_invite(&request, state),
        "ACK" => handle_ack(&request, state),
        "BYE" => handle_bye(&request, state),
        "CANCEL" => handle_cancel(&request, state),
        "OPTIONS" => Some(request.options_response()),
        "INFO" => handle_info(&request, state),
        "MESSAGE" => handle_message(&request, state),
        "REFER" => handle_refer(&request, state),
        "NOTIFY" => handle_notify(&request, state),
        "SUBSCRIBE" => handle_subscribe(&request, peer, state),
        "PRACK" => Some(request.response(200, "OK", &[])),
        "UPDATE" => Some(request.response(200, "OK", &[])),
        "PUBLISH" => Some(request.response(202, "Accepted", &[])),
        _ => Some(request.response(
            501,
            "Not Implemented",
            &[("Allow", allowed_methods())],
        )),
    }
}

fn handle_register(request: &SipRequest, peer: SocketAddr, state: &AppState) -> Option<String> {
    let aor = request
        .header("to")
        .or_else(|| request.header("from"))
        .and_then(extract_sip_uri)
        .unwrap_or_else(|| request.uri());
    let Some((_, realm)) = split_sip_aor(&aor) else {
        return Some(request.response(400, "Bad Request", &[]));
    };

    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }

    let contact = request
        .header("contact")
        .and_then(extract_sip_uri)
        .unwrap_or_else(|| format!("sip:{}", peer));
    let expires = request
        .header("expires")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(3600)
        .clamp(0, 86_400);

    if expires == 0 {
        let _ = state.remove_registration(&aor);
        return Some(request.response(200, "OK", &[("Expires", "0".to_string())]));
    }

    state.upsert_registration(SipRegistration {
        aor,
        contact,
        source: peer.to_string(),
        user_agent: request.header("user-agent").map(ToOwned::to_owned),
        expires_at: Utc::now() + Duration::seconds(expires),
        updated_at: Utc::now(),
    });

    Some(request.response(200, "OK", &[("Expires", expires.to_string())]))
}

fn handle_invite(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(from_aor) = request.header("from").and_then(extract_sip_uri) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    let Some((_, realm)) = split_sip_aor(&from_aor) else {
        return Some(request.response(400, "Bad Request", &[]));
    };

    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }

    // Re-INVITE detection: if dialog exists, this is a mid-call update (hold/video toggle)
    if let Some(call_id) = request.call_id() {
        if state.dialog_exists(call_id) {
            let body = request.body_text();
            let hold_status = detect_hold_from_sdp(&body);
            let status = match hold_status {
                Some(true) => SipDialogStatus::Held,
                Some(false) => SipDialogStatus::Ringing,
                None => SipDialogStatus::Ringing,
            };
            // Update media types from re-INVITE SDP
            let media = extract_media_types(&body);
            state.upsert_sip_dialog(UpsertSipDialog {
                call_id: call_id.to_string(),
                from_uri: from_aor.clone(),
                to_uri: request.uri(),
                target_contact: None,
                status,
                media_types: media,
            });
            return Some(request.response(200, "OK", &[]));
        }
    }

    let requested_uri = request.uri();

    // Conference call routing: sip:conf-{uuid}@domain → join conference
    if let Some(conference) = state.conference_by_uri(&requested_uri) {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: request.call_id().unwrap_or_default().to_string(),
            from_uri: from_aor.clone(),
            to_uri: requested_uri,
            target_contact: None,
            status: SipDialogStatus::Ringing,
            media_types: extract_media_types(&request.body_text()),
        });
        if conference.active {
            return Some(request.response(200, "OK", &[]));
        }
        return Some(request.response(480, "Conference Not Active", &[]));
    }

    let routed_uri = state
        .resolve_routing_target(&from_aor, &requested_uri)
        .unwrap_or_else(|| requested_uri.clone());

    match state.registration_for(&routed_uri) {
        Some(registration) => {
            state.upsert_sip_dialog(UpsertSipDialog {
                call_id: request.call_id().unwrap_or_default().to_string(),
                from_uri: from_aor,
                to_uri: requested_uri,
                target_contact: Some(registration.contact.clone()),
                status: SipDialogStatus::Ringing,
                media_types: extract_media_types(&request.body_text()),
            });
            Some(request.response(
                302,
                "Moved Temporarily",
                &[("Contact", format!("<{}>", registration.contact))],
            ))
        }
        None => {
            let target_contact = if routed_uri.starts_with("sip:") && routed_uri != requested_uri {
                Some(routed_uri)
            } else {
                None
            };
            state.upsert_sip_dialog(UpsertSipDialog {
                call_id: request.call_id().unwrap_or_default().to_string(),
                from_uri: from_aor,
                to_uri: requested_uri,
                target_contact: target_contact.clone(),
                status: if target_contact.is_some() {
                    SipDialogStatus::Routing
                } else {
                    SipDialogStatus::Failed
                },
                media_types: extract_media_types(&request.body_text()),
            });
            target_contact
                .map(|contact| {
                    request.response(
                        302,
                        "Moved Temporarily",
                        &[("Contact", format!("<{}>", contact))],
                    )
                })
                .or_else(|| Some(request.response(480, "Temporarily Unavailable", &[])))
        }
    }
}

fn handle_ack(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(realm) = request.sender_realm() else {
        return None;
    };
    if !request.is_authorized(state, &realm) {
        return None;
    }
    if let Some(call_id) = request.call_id() {
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Ringing);
    }
    None
}

fn handle_bye(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(realm) = request.sender_realm() else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }
    if let Some(call_id) = request.call_id() {
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Ended);
    }
    Some(request.response(200, "OK", &[]))
}

fn handle_cancel(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(realm) = request.sender_realm() else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }
    if let Some(call_id) = request.call_id() {
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Cancelled);
    }
    Some(request.response(200, "OK", &[]))
}

fn handle_info(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(realm) = request.sender_realm() else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }
    Some(request.response(200, "OK", &[]))
}

fn handle_message(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(from_uri) = request.header("from").and_then(extract_sip_uri) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    let Some((_, realm)) = split_sip_aor(&from_uri) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }

    state.store_sip_message(StoreSipMessage {
        call_id: request.call_id().map(ToOwned::to_owned),
        from_uri,
        to_uri: request.uri(),
        content_type: request
            .header("content-type")
            .unwrap_or("text/plain")
            .to_string(),
        body: request.body_text(),
    });
    Some(request.response(202, "Accepted", &[]))
}

fn handle_refer(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(realm) = request.sender_realm() else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }

    let refer_to = request
        .header("refer-to")
        .and_then(extract_sip_uri)
        .or_else(|| request.header("refer-to").map(|v| v.trim().to_string()));

    let Some(target) = refer_to else {
        return Some(request.response(400, "Bad Request", &[]));
    };

    // End the original dialog
    if let Some(call_id) = request.call_id() {
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Ended);
    }

    // Record audit
    let from_uri = request
        .header("from")
        .and_then(extract_sip_uri)
        .unwrap_or_default();
    state.record_audit_event(&from_uri, "call.transferred", Some(target.clone()));

    Some(request.response(
        202,
        "Accepted",
        &[("Subscription-State", "terminated;reason=noresource".to_string())],
    ))
}

const SUPPORTED_EVENTS: &[&str] = &["presence", "dialog", "message-summary", "conference"];

fn handle_subscribe(
    request: &SipRequest,
    _peer: SocketAddr,
    state: &AppState,
) -> Option<String> {
    let Some(subscriber) = request.header("from").and_then(extract_sip_uri) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    let Some((_, realm)) = split_sip_aor(&subscriber) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }

    let Some(event) = request.header("event").map(|v| v.split(';').next().unwrap_or(v).trim()) else {
        return Some(request.response(
            489,
            "Bad Event",
            &[("Allow-Events", SUPPORTED_EVENTS.join(", "))],
        ));
    };
    if !SUPPORTED_EVENTS.contains(&event) {
        return Some(request.response(
            489,
            "Bad Event",
            &[("Allow-Events", SUPPORTED_EVENTS.join(", "))],
        ));
    }

    let expires = request
        .header("expires")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(3600)
        .clamp(0, 86_400);

    let from_tag = request
        .header("from")
        .and_then(|h| h.split("tag=").nth(1))
        .map(|t| t.split(';').next().unwrap_or(t))
        .unwrap_or("notag");
    let subscription_id = format!(
        "{}:{}",
        request.call_id().unwrap_or("unknown"),
        from_tag
    );

    if expires == 0 {
        let _ = state.remove_sip_subscription(&subscription_id);
        return Some(request.response(200, "OK", &[("Expires", "0".to_string())]));
    }

    let event_str = event.to_string();
    state.upsert_sip_subscription(UpsertSipSubscription {
        subscription_id,
        subscriber,
        target: request.uri(),
        event: event_str.clone(),
        expires_at: Utc::now() + Duration::seconds(expires),
    });

    Some(request.response(
        200,
        "OK",
        &[
            ("Expires", expires.to_string()),
            ("Event", event_str),
            (
                "Subscription-State",
                format!("active;expires={}", expires),
            ),
        ],
    ))
}

fn handle_notify(request: &SipRequest, state: &AppState) -> Option<String> {
    let Some(notifier) = request.header("from").and_then(extract_sip_uri) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    let Some((_, realm)) = split_sip_aor(&notifier) else {
        return Some(request.response(400, "Bad Request", &[]));
    };
    if !request.is_authorized(state, &realm) {
        return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
    }

    let event = request.header("event").map(ToOwned::to_owned);
    let subscription_state = request
        .header("subscription-state")
        .map(ToOwned::to_owned);
    let content_type = request
        .header("content-type")
        .unwrap_or("application/pidf+xml")
        .to_string();
    let body = request.body_text();
    let subscription_id = request.call_id().map(ToOwned::to_owned);

    state.store_sip_notification(StoreSipNotification {
        subscription_id: subscription_id.clone(),
        notifier: notifier.clone(),
        target: request.uri(),
        event: event.clone(),
        subscription_state: subscription_state.clone(),
        content_type,
        body: body.clone(),
    });

    // Derive presence from PIDF body
    if event.as_deref() == Some("presence") {
        let status = if body.contains("<basic>open</basic>") {
            Some(PresenceStatus::Online)
        } else if body.contains("<basic>closed</basic>") {
            Some(PresenceStatus::Offline)
        } else {
            None
        };
        if let Some(status) = status {
            // Extract the presentity from the NOTIFY target or notifier
            let presentity = request.uri();
            state.update_presence(&presentity, status, None);
        }
    }

    // If the subscription is terminated, remove it
    if let Some(sub_state) = &subscription_state {
        if sub_state.contains("terminated") {
            if let Some(sid) = &subscription_id {
                let _ = state.remove_sip_subscription(sid);
            }
        }
    }

    Some(request.response(200, "OK", &[]))
}

async fn relay_message(
    socket: &UdpSocket,
    request: &SipRequest,
    _peer: SocketAddr,
    state: &AppState,
) -> Option<String> {
    let Some(from_uri) = request.header("from").and_then(extract_sip_uri) else {
        return None;
    };
    let Some((_, realm)) = split_sip_aor(&from_uri) else {
        return None;
    };
    if !request.is_authorized(state, &realm) {
        return None;
    }

    let to_uri = request.uri();
    let (target_contact, target_addr) = invite_target(state, &to_uri)?;

    let content_type = request
        .header("content-type")
        .unwrap_or("text/plain")
        .to_string();
    let body = request.body_text();

    // Store the message
    state.store_sip_message(StoreSipMessage {
        call_id: request.call_id().map(ToOwned::to_owned),
        from_uri: from_uri.clone(),
        to_uri: to_uri.clone(),
        content_type: content_type.clone(),
        body: body.clone(),
    });

    // Construct a forwarded MESSAGE to the registered contact
    let branch = format!("z9hG4bK-pale-{}", Uuid::new_v4());
    let Some(local_addr) = socket.local_addr().ok() else {
        return Some(request.response(502, "Bad Gateway", &[]));
    };

    let from = request.header("from").unwrap_or(&from_uri);
    let to = request.header("to").unwrap_or(&to_uri);
    let call_id = request.call_id().unwrap_or("relay");
    let cseq = request.header("cseq").unwrap_or("1 MESSAGE");

    let forwarded = format!(
        "MESSAGE {} SIP/2.0\r\n\
         Via: SIP/2.0/UDP {};branch={}\r\n\
         From: {}\r\n\
         To: {}\r\n\
         Call-ID: {}\r\n\
         CSeq: {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\r\n{}",
        target_contact,
        sip_sent_by(local_addr),
        branch,
        from,
        to,
        call_id,
        cseq,
        content_type,
        body.len(),
        body,
    );

    let _ = socket.send_to(forwarded.as_bytes(), target_addr).await;

    Some(request.response(202, "Accepted", &[]))
}

async fn proxy_invite(
    server_socket: &UdpSocket,
    packet: &str,
    request: &SipRequest,
    state: &AppState,
) -> Option<String> {
    let Some(from_aor) = request.header("from").and_then(extract_sip_uri) else {
        return None;
    };
    let Some((_, realm)) = split_sip_aor(&from_aor) else {
        return None;
    };
    let requested_uri = request.uri();
    let routed_uri = state
        .resolve_routing_target(&from_aor, &requested_uri)
        .unwrap_or_else(|| requested_uri.clone());
    let (target_contact, target_addr) = invite_target(state, &routed_uri)?;
    if !request.is_authorized(state, &realm) {
        return None;
    }

    let branch = format!("z9hG4bK-pale-{}", Uuid::new_v4());
    let Some(proxy_bind_addr) = server_socket.local_addr().ok().map(proxy_bind_addr) else {
        return Some(request.response(502, "Bad Gateway", &[]));
    };
    let proxy_socket = match UdpSocket::bind(proxy_bind_addr).await {
        Ok(socket) => socket,
        Err(_) => return Some(request.response(502, "Bad Gateway", &[])),
    };
    let local_addr = match proxy_socket.local_addr() {
        Ok(addr) => addr,
        Err(_) => return Some(request.response(502, "Bad Gateway", &[])),
    };
    let Some(forwarded) = rewrite_invite_for_proxy(packet, &target_contact, local_addr, &branch)
    else {
        return Some(request.response(400, "Bad Request", &[]));
    };

    state.upsert_sip_dialog(UpsertSipDialog {
        call_id: request.call_id().unwrap_or_default().to_string(),
        from_uri: from_aor,
        to_uri: requested_uri,
        target_contact: Some(target_contact),
        status: SipDialogStatus::Routing,
        media_types: extract_media_types(packet),
    });

    if proxy_socket
        .send_to(forwarded.as_bytes(), target_addr)
        .await
        .is_err()
    {
        return Some(request.response(502, "Bad Gateway", &[]));
    }
    let mut response = vec![0_u8; 16 * 1024];
    let (len, _) = match timeout(
        StdDuration::from_secs(5),
        proxy_socket.recv_from(&mut response),
    )
    .await
    {
        Ok(Ok(received)) => received,
        Ok(Err(_)) => return Some(request.response(502, "Bad Gateway", &[])),
        Err(_) => return Some(request.response(504, "Gateway Timeout", &[])),
    };
    let response = String::from_utf8_lossy(&response[..len]).to_string();
    let relayed = strip_proxy_via(&response, &branch).unwrap_or(response);
    if let Some((status, _)) = parse_response_status(&relayed) {
        let status = if status >= 200 {
            SipDialogStatus::Ended
        } else {
            SipDialogStatus::Ringing
        };
        if let Some(call_id) = request.call_id() {
            let _ = state.update_sip_dialog_status(call_id, status);
        }
    }
    Some(relayed)
}

fn invite_target(state: &AppState, routed_uri: &str) -> Option<(String, SocketAddr)> {
    if let Some(registration) = state.registration_for(routed_uri) {
        let target_addr = registration
            .source
            .parse()
            .ok()
            .or_else(|| sip_uri_socket_addr(&registration.contact))?;
        return Some((registration.contact, target_addr));
    }

    sip_uri_socket_addr(routed_uri).map(|addr| (routed_uri.to_string(), addr))
}

fn proxy_bind_addr(local_addr: SocketAddr) -> SocketAddr {
    if local_addr.is_ipv6() {
        "[::]:0".parse().expect("valid IPv6 wildcard")
    } else {
        "0.0.0.0:0".parse().expect("valid IPv4 wildcard")
    }
}

fn rewrite_invite_for_proxy(
    packet: &str,
    target_uri: &str,
    proxy_addr: SocketAddr,
    branch: &str,
) -> Option<String> {
    let mut lines = packet.split_inclusive("\r\n");
    let first_line = lines.next()?;
    let mut parts = first_line.trim_end_matches("\r\n").split_whitespace();
    let method = parts.next()?;
    let _uri = parts.next()?;
    let version = parts.next().unwrap_or("SIP/2.0");
    if method != "INVITE" {
        return None;
    }

    let mut out = String::new();
    out.push_str(&format!("{method} {target_uri} {version}\r\n"));
    out.push_str(&format!(
        "Via: SIP/2.0/UDP {};branch={branch}\r\n",
        sip_sent_by(proxy_addr)
    ));
    for line in lines {
        out.push_str(line);
    }
    Some(out)
}

fn strip_proxy_via(response: &str, branch: &str) -> Option<String> {
    let mut removed = false;
    let mut out = String::new();
    for line in response.split_inclusive("\r\n") {
        if !removed
            && line
                .to_ascii_lowercase()
                .starts_with("via: sip/2.0/udp ")
            && line.contains(branch)
        {
            removed = true;
            continue;
        }
        out.push_str(line);
    }
    removed.then_some(out)
}

fn sip_sent_by(addr: SocketAddr) -> String {
    if addr.is_ipv6() {
        format!("[{}]:{}", addr.ip(), addr.port())
    } else {
        addr.to_string()
    }
}

fn sip_uri_socket_addr(uri: &str) -> Option<SocketAddr> {
    let uri = uri
        .strip_prefix("sip:")
        .or_else(|| uri.strip_prefix("sips:"))?;
    let authority = uri.split(';').next()?.split('?').next()?;
    let host_port = authority.rsplit_once('@').map(|(_, value)| value).unwrap_or(authority);
    if host_port.starts_with('[') {
        return host_port.parse().ok();
    }
    if host_port.matches(':').count() == 1 {
        return host_port.parse().ok();
    }
    format!("{host_port}:5060").parse().ok()
}

fn record_transaction(
    request: &SipRequest,
    peer: SocketAddr,
    response: Option<&str>,
    state: &AppState,
) {
    let (status_code, reason) = response
        .and_then(parse_response_status)
        .map(|(code, reason)| (Some(code), Some(reason)))
        .unwrap_or((None, None));

    state.store_sip_transaction(StoreSipTransaction {
        method: request.method(),
        uri: request.uri(),
        call_id: request.call_id().map(ToOwned::to_owned),
        cseq: request.header("cseq").map(ToOwned::to_owned),
        source: peer.to_string(),
        status_code,
        reason,
    });
}

#[derive(Debug)]
struct SipRequest {
    inner: Request,
}

impl SipRequest {
    fn parse(packet: &str) -> Option<Self> {
        Request::try_from(packet).ok().map(|inner| Self { inner })
    }

    fn header(&self, name: &str) -> Option<&str> {
        let expected = canonical_header_name(name);
        self.inner
            .headers
            .iter()
            .find_map(|header| header_value(header, &expected))
    }

    fn call_id(&self) -> Option<&str> {
        self.header("call-id")
    }

    fn method(&self) -> String {
        self.inner.method.to_string()
    }

    fn uri(&self) -> String {
        self.inner.uri.to_string()
    }

    fn body_text(&self) -> String {
        let body = trim_body_to_content_length(&self.inner.body, self.header("content-length"));
        String::from_utf8_lossy(body).to_string()
    }

    fn sender_realm(&self) -> Option<String> {
        let from = self.header("from").and_then(extract_sip_uri)?;
        let (_, realm) = split_sip_aor(&from)?;
        Some(realm)
    }

    fn options_response(&self) -> String {
        self.response(
            200,
            "OK",
            &[
                ("Allow", allowed_methods()),
                ("Accept", "application/sdp, text/plain".to_string()),
                (
                    "Supported",
                    "100rel, replaces, timer, norefersub, outbound".to_string(),
                ),
                (
                    "Allow-Events",
                    "conference, dialog, message-summary, presence".to_string(),
                ),
            ],
        )
    }

    fn response(&self, code: u16, reason: &str, extra_headers: &[(&str, String)]) -> String {
        let mut headers = Vec::new();
        for header in ["via", "from", "to", "call-id", "cseq"] {
            if let Some(value) = self.header(header) {
                headers.push(response_header(header, value.to_string()));
            }
        }
        for (name, value) in extra_headers {
            headers.push(response_header(name, value.clone()));
        }
        headers.push(Header::Server(rsip::headers::Server::new("Pale SIP")));
        headers.push(Header::ContentLength(rsip::headers::ContentLength::new("0")));
        Response {
            status_code: status_code(code, reason),
            version: Version::V2,
            headers: Headers::from(headers),
            body: Vec::new(),
        }
        .to_string()
    }

    fn digest_challenge(&self, realm: &str, nonce: String) -> String {
        self.response(
            401,
            "Unauthorized",
            &[(
                "WWW-Authenticate",
                format!(
                    "Digest realm=\"{}\", nonce=\"{}\", algorithm=MD5, qop=\"auth\"",
                    escape_quoted(realm),
                    escape_quoted(&nonce)
                ),
            )],
        )
    }

    fn is_authorized(&self, state: &AppState, realm: &str) -> bool {
        let Some(header) = self.header("authorization") else {
            return false;
        };
        let Some(auth) = DigestAuth::parse(header) else {
            return false;
        };
        if auth.realm != realm || auth.uri != self.uri() {
            return false;
        }
        let Some(account) = state.sip_account(&auth.username, &auth.realm) else {
            return false;
        };
        if !account.enabled || !auth.verify(&self.method(), &account.password_ha1) {
            return false;
        }
        state.consume_sip_nonce(&auth.nonce)
    }
}

fn trim_body_to_content_length<'a>(body: &'a [u8], content_length: Option<&str>) -> &'a [u8] {
    let Some(length) = content_length.and_then(|v| v.parse::<usize>().ok()) else {
        return body;
    };
    body.get(..length.min(body.len())).unwrap_or_default()
}

fn canonical_header_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "f" => "from",
        "t" => "to",
        "i" => "call-id",
        "m" => "contact",
        "l" => "content-length",
        "c" => "content-type",
        "v" => "via",
        "k" => "supported",
        "o" => "event",
        other => other,
    }
    .to_string()
}

fn parse_response_status(response: &str) -> Option<(u16, String)> {
    let response = Response::try_from(response).ok()?;
    let status = response.status_code;
    Some((status.code(), status_reason(&status)))
}

fn header_value<'a>(header: &'a Header, expected: &str) -> Option<&'a str> {
    match header {
        Header::Accept(value) if expected == "accept" => Some(value.value()),
        Header::Allow(value) if expected == "allow" => Some(value.value()),
        Header::Authorization(value) if expected == "authorization" => Some(value.value()),
        Header::CSeq(value) if expected == "cseq" => Some(value.value()),
        Header::CallId(value) if expected == "call-id" => Some(value.value()),
        Header::Contact(value) if expected == "contact" => Some(value.value()),
        Header::ContentLength(value) if expected == "content-length" => Some(value.value()),
        Header::ContentType(value) if expected == "content-type" => Some(value.value()),
        Header::Event(value) if expected == "event" => Some(value.value()),
        Header::Expires(value) if expected == "expires" => Some(value.value()),
        Header::From(value) if expected == "from" => Some(value.value()),
        Header::Server(value) if expected == "server" => Some(value.value()),
        Header::SubscriptionState(value) if expected == "subscription-state" => Some(value.value()),
        Header::Supported(value) if expected == "supported" => Some(value.value()),
        Header::To(value) if expected == "to" => Some(value.value()),
        Header::UserAgent(value) if expected == "user-agent" => Some(value.value()),
        Header::Via(value) if expected == "via" => Some(value.value()),
        Header::WwwAuthenticate(value) if expected == "www-authenticate" => Some(value.value()),
        Header::Other(name, value) if canonical_header_name(name) == expected => Some(value),
        _ => None,
    }
}

fn response_header(name: &str, value: String) -> Header {
    match canonical_header_name(name).as_str() {
        "accept" => Header::Accept(rsip::headers::Accept::new(value)),
        "allow" => Header::Allow(rsip::headers::Allow::new(value)),
        "authorization" => Header::Authorization(rsip::headers::Authorization::new(value)),
        "call-id" => Header::CallId(rsip::headers::CallId::new(value)),
        "contact" => Header::Contact(rsip::headers::Contact::new(value)),
        "content-length" => Header::ContentLength(rsip::headers::ContentLength::new(value)),
        "content-type" => Header::ContentType(rsip::headers::ContentType::new(value)),
        "cseq" => Header::CSeq(rsip::headers::CSeq::new(value)),
        "event" => Header::Event(rsip::headers::Event::new(value)),
        "expires" => Header::Expires(rsip::headers::Expires::new(value)),
        "from" => Header::From(rsip::headers::From::new(value)),
        "server" => Header::Server(rsip::headers::Server::new(value)),
        "subscription-state" => {
            Header::SubscriptionState(rsip::headers::SubscriptionState::new(value))
        }
        "supported" => Header::Supported(rsip::headers::Supported::new(value)),
        "to" => Header::To(rsip::headers::To::new(value)),
        "user-agent" => Header::UserAgent(rsip::headers::UserAgent::new(value)),
        "via" => Header::Via(rsip::headers::Via::new(value)),
        "www-authenticate" => {
            Header::WwwAuthenticate(rsip::headers::WwwAuthenticate::new(value))
        }
        other => Header::Other(display_header_name(other).to_string(), value),
    }
}

fn display_header_name(name: &str) -> &str {
    match name {
        "allow-events" => "Allow-Events",
        other => other,
    }
}

fn status_code(code: u16, reason: &str) -> StatusCode {
    match StatusCode::from(code) {
        status if status_reason(&status) == reason => status,
        _ => StatusCode::Other(code, reason.to_string()),
    }
}

fn status_reason(status: &StatusCode) -> String {
    let rendered = status.to_string();
    rendered
        .split_once(' ')
        .map(|(_, reason)| reason.to_string())
        .unwrap_or_default()
}

fn allowed_methods() -> String {
    "INVITE, ACK, BYE, CANCEL, OPTIONS, REGISTER, INFO, MESSAGE, REFER, NOTIFY, SUBSCRIBE, PRACK, UPDATE, PUBLISH".to_string()
}

fn extract_media_types(sdp_body: &str) -> Vec<MediaKind> {
    let mut media = Vec::new();
    for line in sdp_body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("m=audio ") {
            if !media.contains(&MediaKind::Audio) {
                media.push(MediaKind::Audio);
            }
        } else if trimmed.starts_with("m=video ") {
            if !media.contains(&MediaKind::Video) {
                media.push(MediaKind::Video);
            }
        }
    }
    media
}

fn detect_hold_from_sdp(sdp_body: &str) -> Option<bool> {
    for line in sdp_body.lines() {
        let trimmed = line.trim();
        if trimmed == "a=sendonly" || trimmed == "a=inactive" {
            return Some(true);
        }
        if trimmed == "a=sendrecv" {
            return Some(false);
        }
    }
    None
}

fn extract_sip_uri(value: &str) -> Option<String> {
    if let Some(start) = value.find('<') {
        let rest = &value[start + 1..];
        let end = rest.find('>')?;
        return Some(rest[..end].to_string());
    }
    value
        .split(';')
        .next()
        .map(str::trim)
        .filter(|v| v.starts_with("sip:") || v.starts_with("sips:"))
        .map(ToOwned::to_owned)
}

fn split_sip_aor(aor: &str) -> Option<(String, String)> {
    let aor = aor
        .strip_prefix("sip:")
        .or_else(|| aor.strip_prefix("sips:"))?;
    let bare = aor.split(';').next()?.split('?').next()?;
    let (username, domain) = bare.split_once('@')?;
    Some((username.to_string(), domain.to_string()))
}

fn escape_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug)]
struct DigestAuth {
    username: String,
    realm: String,
    nonce: String,
    uri: String,
    response: String,
    qop: Option<String>,
    nc: Option<String>,
    cnonce: Option<String>,
}

impl DigestAuth {
    fn parse(header: &str) -> Option<Self> {
        let params = header.trim().strip_prefix("Digest")?.trim();
        let mut values = HashMap::new();
        for part in params.split(',') {
            let (key, value) = part.trim().split_once('=')?;
            values.insert(key.trim().to_ascii_lowercase(), unquote(value.trim()));
        }
        Some(Self {
            username: values.remove("username")?,
            realm: values.remove("realm")?,
            nonce: values.remove("nonce")?,
            uri: values.remove("uri")?,
            response: values.remove("response")?,
            qop: values.remove("qop"),
            nc: values.remove("nc"),
            cnonce: values.remove("cnonce"),
        })
    }

    fn verify(&self, method: &str, ha1: &str) -> bool {
        let ha2 = md5_hex(format!("{}:{}", method, self.uri).as_bytes());
        let expected = match (&self.qop, &self.nc, &self.cnonce) {
            (Some(qop), Some(nc), Some(cnonce)) => md5_hex(
                format!(
                    "{}:{}:{}:{}:{}:{}",
                    ha1, self.nonce, nc, cnonce, qop, ha2
                )
                .as_bytes(),
            ),
            _ => md5_hex(format!("{}:{}:{}", ha1, self.nonce, ha2).as_bytes()),
        };
        self.response.eq_ignore_ascii_case(&expected)
    }
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn register_packet_updates_registrar() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "REGISTER",
            "sip:example.com",
            &nonce,
        );
        let packet = format!(
            "REGISTER sip:example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bK\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:alice@example.com>\r\n\
Call-ID: abc\r\n\
CSeq: 1 REGISTER\r\n\
Contact: <sip:alice@127.0.0.1:5062>\r\n\
Expires: 600\r\n\
Authorization: {}\r\n\r\n",
            auth
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.registrations().len(), 1);
        assert_eq!(state.registrations()[0].aor, "sip:alice@example.com");
    }

    #[test]
    fn register_without_authorization_is_challenged() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let packet = "REGISTER sip:example.com SIP/2.0\r\n\
From: <sip:alice@example.com>\r\n\
To: <sip:alice@example.com>\r\n\
Call-ID: abc\r\n\
CSeq: 1 REGISTER\r\n\r\n";

        let response = handle_packet(packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 401 Unauthorized"));
        assert!(response.contains("WWW-Authenticate: Digest"));
        assert!(state.registrations().is_empty());
    }

    #[test]
    fn invite_to_registered_user_redirects_to_contact() {
        let state = test_state();
        state.upsert_registration(SipRegistration {
            aor: "sip:bob@example.com".to_string(),
            contact: "sip:bob@10.0.0.2:5060".to_string(),
            source: "10.0.0.2:5060".to_string(),
            user_agent: None,
            expires_at: Utc::now() + Duration::minutes(10),
            updated_at: Utc::now(),
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "INVITE",
            "sip:bob@example.com",
            &nonce,
        );
        let packet = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bK\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: invite-1\r\n\
CSeq: 1 INVITE\r\n\
Authorization: {}\r\n\r\n",
            auth
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 302 Moved Temporarily"));
        assert!(response.contains("Contact: <sip:bob@10.0.0.2:5060>"));
        assert_eq!(state.sip_dialogs().len(), 1);
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Ringing);
    }

    #[tokio::test]
    async fn udp_invite_is_proxied_to_registered_contact() {
        let state = test_state();
        let downstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let downstream_addr = downstream.local_addr().unwrap();
        state.upsert_registration(SipRegistration {
            aor: "sip:bob@example.com".to_string(),
            contact: format!("sip:bob@{}", downstream_addr),
            source: downstream_addr.to_string(),
            user_agent: None,
            expires_at: Utc::now() + Duration::minutes(10),
            updated_at: Utc::now(),
        });

        let downstream_task = tokio::spawn(async move {
            let mut buf = vec![0_u8; 8192];
            let (len, proxy_addr) = downstream.recv_from(&mut buf).await.unwrap();
            let forwarded = String::from_utf8_lossy(&buf[..len]).to_string();
            assert!(forwarded.starts_with(&format!("INVITE sip:bob@{} SIP/2.0", downstream_addr)));
            let top_via = forwarded
                .lines()
                .find(|line| line.contains("z9hG4bK-pale-"))
                .unwrap()
                .to_string();
            let response = format!(
                "SIP/2.0 180 Ringing\r\n{}\r\nVia: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bKcaller\r\nFrom: <sip:alice@example.com>;tag=1\r\nTo: <sip:bob@example.com>;tag=2\r\nCall-ID: invite-proxy\r\nCSeq: 1 INVITE\r\nContent-Length: 0\r\n\r\n",
                top_via
            );
            downstream
                .send_to(response.as_bytes(), proxy_addr)
                .await
                .unwrap();
        });

        let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "INVITE",
            "sip:bob@example.com",
            &nonce,
        );
        let packet = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bKcaller\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: invite-proxy\r\n\
CSeq: 1 INVITE\r\n\
Authorization: {}\r\n\r\n",
            auth
        );

        let response = handle_udp_packet(&server_socket, &packet, peer, &state)
            .await
            .unwrap();

        downstream_task.await.unwrap();
        assert!(response.starts_with("SIP/2.0 180 Ringing"));
        assert!(!response.contains("z9hG4bK-pale-"));
        assert!(response.contains("z9hG4bKcaller"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Ringing);
    }

    #[test]
    fn cancel_and_bye_update_dialog_state() {
        let state = test_state();
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "call-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: Some("sip:bob@10.0.0.2:5060".to_string()),
            status: SipDialogStatus::Ringing,
            media_types: vec![],
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();

        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "CANCEL",
            "sip:bob@example.com",
            &nonce,
        );
        let cancel = format!("CANCEL sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
Call-ID: call-1\r\n\
CSeq: 2 CANCEL\r\n\
Authorization: {}\r\n\r\n", auth);
        let response = handle_packet(&cancel, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Cancelled);

        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "BYE",
            "sip:bob@example.com",
            &nonce,
        );
        let bye = format!("BYE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
Call-ID: call-1\r\n\
CSeq: 3 BYE\r\n\
Authorization: {}\r\n\r\n", auth);
        let response = handle_packet(&bye, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Ended);
    }

    #[test]
    fn bye_without_authorization_does_not_update_dialog() {
        let state = test_state();
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "call-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: Some("sip:bob@10.0.0.2:5060".to_string()),
            status: SipDialogStatus::Ringing,
            media_types: vec![],
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let bye = "BYE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
Call-ID: call-1\r\n\
CSeq: 3 BYE\r\n\r\n";

        let response = handle_packet(bye, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 401 Unauthorized"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Ringing);
    }

    #[test]
    fn options_advertises_supported_methods() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let packet = "OPTIONS sip:example.com SIP/2.0\r\n\
Call-ID: options-1\r\n\
CSeq: 1 OPTIONS\r\n\r\n";

        let response = handle_packet(packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert!(response.contains("Allow: INVITE, ACK, BYE"));
        assert!(response.contains("Supported: 100rel"));
    }

    #[test]
    fn register_expires_zero_removes_registration() {
        let state = test_state();
        state.upsert_registration(SipRegistration {
            aor: "sip:alice@example.com".to_string(),
            contact: "sip:alice@127.0.0.1:5062".to_string(),
            source: "127.0.0.1:5062".to_string(),
            user_agent: None,
            expires_at: Utc::now() + Duration::minutes(10),
            updated_at: Utc::now(),
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "REGISTER",
            "sip:example.com",
            &nonce,
        );
        let packet = format!(
            "REGISTER sip:example.com SIP/2.0\r\n\
f: <sip:alice@example.com>;tag=1\r\n\
t: <sip:alice@example.com>\r\n\
i: compact-register\r\n\
CSeq: 2 REGISTER\r\n\
m: <sip:alice@127.0.0.1:5062>\r\n\
Expires: 0\r\n\
Authorization: {}\r\n\r\n",
            auth
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert!(state.registrations().is_empty());
    }

    #[test]
    fn message_is_authenticated_and_stored() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "MESSAGE",
            "sip:bob@example.com",
            &nonce,
        );
        let packet = format!(
            "MESSAGE sip:bob@example.com SIP/2.0\r\n\
f: <sip:alice@example.com>;tag=1\r\n\
t: <sip:bob@example.com>\r\n\
i: message-1\r\n\
CSeq: 1 MESSAGE\r\n\
c: text/plain\r\n\
l: 5\r\n\
Authorization: {}\r\n\r\nhello ignored",
            auth
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 202 Accepted"));
        assert_eq!(state.sip_messages().len(), 1);
        assert_eq!(state.sip_messages()[0].body, "hello");
        assert_eq!(state.sip_messages()[0].content_type, "text/plain");
    }

    #[test]
    fn handled_requests_are_recorded_as_transactions() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let packet = "OPTIONS sip:example.com SIP/2.0\r\n\
Call-ID: tx-1\r\n\
CSeq: 1 OPTIONS\r\n\r\n";

        let response = handle_packet(packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_transactions().len(), 1);
        let tx = &state.sip_transactions()[0];
        assert_eq!(tx.method, "OPTIONS");
        assert_eq!(tx.call_id.as_deref(), Some("tx-1"));
        assert_eq!(tx.status_code, Some(200));
        assert_eq!(tx.reason.as_deref(), Some("OK"));
    }

    #[test]
    fn subscribe_creates_subscription() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "SUBSCRIBE",
            "sip:bob@example.com",
            &nonce,
        );
        let packet = format!(
            "SUBSCRIBE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: sub-1\r\n\
CSeq: 1 SUBSCRIBE\r\n\
Event: presence\r\n\
Expires: 600\r\n\
Authorization: {}\r\n\r\n",
            auth
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert!(response.contains("Expires:"));
        assert_eq!(state.sip_subscriptions().len(), 1);
        assert_eq!(state.sip_subscriptions()[0].event, "presence");
    }

    #[test]
    fn subscribe_without_auth_is_challenged() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let packet = "SUBSCRIBE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: sub-2\r\n\
CSeq: 1 SUBSCRIBE\r\n\
Event: presence\r\n\r\n";

        let response = handle_packet(packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 401 Unauthorized"));
        assert!(response.contains("WWW-Authenticate: Digest"));
        assert!(state.sip_subscriptions().is_empty());
    }

    #[test]
    fn subscribe_with_unsupported_event_returns_489() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "SUBSCRIBE",
            "sip:bob@example.com",
            &nonce,
        );
        let packet = format!(
            "SUBSCRIBE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
Call-ID: sub-3\r\n\
CSeq: 1 SUBSCRIBE\r\n\
Event: unknown-event\r\n\
Authorization: {}\r\n\r\n",
            auth
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 489 Bad Event"));
    }

    #[test]
    fn subscribe_expires_zero_removes_subscription() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();

        // First create a subscription
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "SUBSCRIBE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "SUBSCRIBE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
Call-ID: sub-4\r\n\
CSeq: 1 SUBSCRIBE\r\n\
Event: presence\r\n\
Expires: 600\r\n\
Authorization: {}\r\n\r\n", auth);
        handle_packet(&packet, peer, &state);
        assert_eq!(state.sip_subscriptions().len(), 1);

        // Now unsubscribe
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "SUBSCRIBE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "SUBSCRIBE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
Call-ID: sub-4\r\n\
CSeq: 2 SUBSCRIBE\r\n\
Event: presence\r\n\
Expires: 0\r\n\
Authorization: {}\r\n\r\n", auth);
        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert!(state.sip_subscriptions().is_empty());
    }

    #[test]
    fn notify_stores_notification_and_updates_presence() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization(
            "alice",
            "example.com",
            "secret",
            "NOTIFY",
            "sip:bob@example.com",
            &nonce,
        );
        let pidf_body = "<presence><tuple><status><basic>open</basic></status></tuple></presence>";
        let packet = format!(
            "NOTIFY sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: notify-1\r\n\
CSeq: 1 NOTIFY\r\n\
Event: presence\r\n\
Subscription-State: active;expires=600\r\n\
Content-Type: application/pidf+xml\r\n\
Content-Length: {}\r\n\
Authorization: {}\r\n\r\n{}",
            pidf_body.len(),
            auth,
            pidf_body
        );

        let response = handle_packet(&packet, peer, &state).unwrap();

        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_notifications().len(), 1);
        // Presence should be updated for the target
        let presence = state.presence("sip:bob@example.com");
        assert!(presence.is_some());
        assert_eq!(presence.unwrap().status, crate::PresenceStatus::Online);
    }

    #[test]
    fn extract_media_types_parses_sdp() {
        let audio_only = "v=0\r\nm=audio 5004 RTP/AVP 0\r\n";
        let audio_video = "v=0\r\nm=audio 5004 RTP/AVP 0\r\nm=video 5006 RTP/AVP 96\r\n";
        let empty = "";

        assert_eq!(extract_media_types(audio_only), vec![crate::MediaKind::Audio]);
        assert_eq!(extract_media_types(audio_video), vec![crate::MediaKind::Audio, crate::MediaKind::Video]);
        assert!(extract_media_types(empty).is_empty());
    }

    #[test]
    fn detect_hold_from_sdp_works() {
        assert_eq!(detect_hold_from_sdp("a=sendonly\r\n"), Some(true));
        assert_eq!(detect_hold_from_sdp("a=inactive\r\n"), Some(true));
        assert_eq!(detect_hold_from_sdp("a=sendrecv\r\n"), Some(false));
        assert_eq!(detect_hold_from_sdp("a=recvonly\r\n"), None);
    }

    #[test]
    fn re_invite_with_hold_sdp_updates_dialog_to_held() {
        let state = test_state();
        // Create initial dialog
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "reinvite-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: None,
            status: SipDialogStatus::Ringing,
            media_types: vec![],
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "INVITE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: reinvite-1\r\n\
CSeq: 2 INVITE\r\n\
Authorization: {}\r\n\r\nv=0\r\nm=audio 5004 RTP/AVP 0\r\na=sendonly\r\n", auth);

        let response = handle_packet(&packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Held);
    }

    #[test]
    fn refer_ends_dialog_and_returns_202() {
        let state = test_state();
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "refer-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: None,
            status: SipDialogStatus::Ringing,
            media_types: vec![],
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "REFER", "sip:bob@example.com", &nonce);
        let packet = format!(
            "REFER sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: refer-1\r\n\
CSeq: 3 REFER\r\n\
Refer-To: <sip:charlie@example.com>\r\n\
Authorization: {}\r\n\r\n", auth);

        let response = handle_packet(&packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 202 Accepted"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Ended);
    }

    #[test]
    fn invite_to_conference_uri_returns_response() {
        let state = test_state();
        let conference = state.create_conference(crate::CreateConferenceRequest {
            title: "Test".to_string(),
            mode: crate::ConferenceMode::Audio,
        });
        state.activate_conference(conference.id);

        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let conf_uri = format!("sip:conf-{}@example.com", conference.id);
        let auth = authorization("alice", "example.com", "secret", "INVITE", &conf_uri, &nonce);
        let packet = format!(
            "INVITE {} SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <{}>\r\n\
Call-ID: conf-call\r\n\
CSeq: 1 INVITE\r\n\
Authorization: {}\r\n\r\n", conf_uri, conf_uri, auth);

        let response = handle_packet(&packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 200 OK"));
    }

    #[test]
    fn token_refresh_rotates_session() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-refresh-test"),
            "012345678901234567890123".to_string(),
            crate::sha256_hex("admin-password".as_bytes()),
        );
        let session = state.authenticate_admin("admin", "admin-password", "test").unwrap();
        assert!(state.principal_for_bearer(&session.token).is_some());

        let new_session = state.refresh_admin_session(&session.token).unwrap();
        assert_ne!(session.token, new_session.token);
        assert!(state.principal_for_bearer(&new_session.token).is_some());
        assert!(state.principal_for_bearer(&session.token).is_none());
    }

    #[test]
    fn call_history_sync_merges_without_duplicates() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-history-test"),
            "012345678901234567890123".to_string(),
            crate::sha256_hex("admin-password".as_bytes()),
        );
        let entries = vec![
            crate::CallHistoryInput {
                direction: "outbound".to_string(),
                remote_uri: "sip:bob@example.com".to_string(),
                remote_name: "Bob".to_string(),
                start_time: "2026-06-05T12:00:00Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap(),
                duration_secs: 120,
                answered: true,
            },
        ];
        let merged = state.merge_call_history("sip:alice@example.com", entries.clone());
        assert_eq!(merged, 1);
        assert_eq!(state.call_history_for_user("sip:alice@example.com").len(), 1);

        // Re-sync same entries — should not duplicate
        let merged = state.merge_call_history("sip:alice@example.com", entries);
        assert_eq!(merged, 0);
        assert_eq!(state.call_history_for_user("sip:alice@example.com").len(), 1);
    }

    fn test_state() -> AppState {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-sip-test"),
            "012345678901234567890123".to_string(),
            crate::sha256_hex("admin-password".as_bytes()),
        );
        state.upsert_sip_account(crate::CreateSipAccountRequest {
            username: "alice".to_string(),
            domain: "example.com".to_string(),
            password_ha1: crate::sip_ha1("alice", "example.com", "secret"),
            display_name: None,
        });
        state
    }

    fn authorization(
        username: &str,
        realm: &str,
        password: &str,
        method: &str,
        uri: &str,
        nonce: &str,
    ) -> String {
        let ha1 = crate::sip_ha1(username, realm, password);
        let ha2 = md5_hex(format!("{}:{}", method, uri).as_bytes());
        let response = md5_hex(format!("{}:{}:{}", ha1, nonce, ha2).as_bytes());
        format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        )
    }
}
