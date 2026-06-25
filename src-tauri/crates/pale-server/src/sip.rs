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
    SipHeaderAction, SipHeaderActionKind, StoreSipMessage, StoreSipNotification, StoreSipTransaction, UpsertSipDialog,
    UpsertSipSubscription,
};

/// Perform the startup half of the UDP SIP server: gate check and socket
/// bind. Errors here mean the SIP listener cannot start and must be treated
/// as fatal by the caller — never swallowed inside a spawned task.
pub async fn bind_udp_socket(
    addr: SocketAddr,
) -> Result<Arc<UdpSocket>, Box<dyn std::error::Error + Send + Sync>> {
    if !allow_insecure_udp_parser() {
        return Err(
            "UDP SIP parser is insecure and disabled; set PALE_ALLOW_INSECURE_SIP_UDP=1 for development fallback use"
                .into(),
        );
    }

    Ok(Arc::new(UdpSocket::bind(addr).await?))
}

pub async fn run_udp_server(
    addr: SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket = bind_udp_socket(addr).await?;
    serve_udp(socket, state).await
}

/// Receive loop over an already-bound socket. Only returns on socket errors.
pub async fn serve_udp(
    socket: Arc<UdpSocket>,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    let has_to_tag = request
        .header("to")
        .map(|to| to.contains("tag="))
        .unwrap_or(false);

    match request.method().as_str() {
        // Initial INVITE: stateful one-shot proxy toward the routed target.
        "INVITE" if !has_to_tag => {
            if let Some(outcome) = proxy_invite(socket, packet, &request, peer, state).await {
                record_transaction(&request, peer, Some(outcome.as_str()), state);
                return Some(outcome);
            }
        }
        "MESSAGE" => {
            if let Some(outcome) = relay_message(socket, &request, peer, state).await {
                record_transaction(&request, peer, Some(outcome.as_str()), state);
                return Some(outcome);
            }
        }
        // CANCEL matches an in-flight proxied INVITE by its top-Via branch;
        // the proxy task forwards the CANCEL upstream and answers the INVITE
        // with 487. The sync handler below still produces the hop-by-hop
        // 200 OK for the CANCEL itself.
        "CANCEL" => {
            if let Some(branch) = top_via_branch(&request) {
                state.cancel_pending_invite(&branch);
            }
        }
        // In-dialog requests are relayed to the peer leg when the dialog has
        // peer addressing (proxied calls). REFERs aimed at the server itself
        // (call park/pickup) stay local.
        "INVITE" | "INFO" | "BYE" | "UPDATE" | "ACK" => {
            if let Some(outcome) =
                relay_in_dialog_request(socket, packet, &request, state).await
            {
                record_transaction(&request, peer, Some(outcome.as_str()), state);
                return Some(outcome);
            }
        }
        "REFER" => {
            if !refer_targets_server(&request) {
                if let Some(outcome) =
                    relay_in_dialog_request(socket, packet, &request, state).await
                {
                    record_transaction(&request, peer, Some(outcome.as_str()), state);
                    return Some(outcome);
                }
            }
            let response = handle_request(&request, peer, state);
            record_transaction(&request, peer, response.as_deref(), state);
            // RFC 3515 implicit subscription: a 202 must be followed by a
            // NOTIFY with a sipfrag result, unless RFC 4488 Refer-Sub: false.
            if let Some(text) = &response {
                let accepted = parse_response_status(text)
                    .map(|(code, _)| code == 202)
                    .unwrap_or(false);
                if accepted && !refer_sub_disabled(&request) {
                    // Park/pickup transfers are executed by the server (200);
                    // anything else was not executed (503) — honest sipfrag.
                    let sipfrag = if refer_targets_server(&request) {
                        "SIP/2.0 200 OK"
                    } else {
                        "SIP/2.0 503 Service Unavailable"
                    };
                    send_refer_notify(socket, &request, peer, sipfrag).await;
                }
            }
            return response;
        }
        _ => {}
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

    // ── Re-INVITE detection (mid-call: hold/video toggle) ──
    // In-dialog requests carry a To-tag (RFC 3261 §12.2.2); a tag-less INVITE
    // reusing a stale Call-ID is a NEW call and takes the routing path below.
    let has_to_tag = request
        .header("to")
        .map(|to| to.contains("tag="))
        .unwrap_or(false);
    if has_to_tag {
        if let Some(call_id) = request.call_id() {
            let live = state.dialog_for(call_id).map(|dialog| {
                !matches!(
                    dialog.status,
                    SipDialogStatus::Ended | SipDialogStatus::Cancelled | SipDialogStatus::Failed
                )
            });
            match live {
                Some(true) => {
                    let body = request.body_text();
                    let hold_status = detect_hold_from_sdp(&body);
                    let status = match hold_status {
                        Some(true) => SipDialogStatus::Held,
                        _ => SipDialogStatus::Answered,
                    };
                    let media = extract_media_types(&body);
                    state.upsert_sip_dialog(UpsertSipDialog {
                        call_id: call_id.to_string(),
                        from_uri: from_aor.clone(),
                        to_uri: request.uri(),
                        target_contact: None,
                        status,
                        media_types: media,
                        peer: Default::default(),
                    });
                    return Some(request.response(200, "OK", &[]));
                }
                _ => {
                    return Some(request.response(481, "Call/Transaction Does Not Exist", &[]))
                }
            }
        }
    }

    let requested_uri = request.uri();
    let call_id_str = request.call_id().unwrap_or_default().to_string();
    let media = extract_media_types(&request.body_text());

    // ── CDR start ──
    state.record_cdr_start(Some(&call_id_str), &from_aor, &requested_uri, "inbound");

    // Helper: create dialog and redirect to a target URI
    let make_redirect = |target: &str| -> Option<String> {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(),
            from_uri: from_aor.clone(),
            to_uri: requested_uri.clone(),
            target_contact: Some(target.to_string()),
            status: SipDialogStatus::Routing,
            media_types: media.clone(),
            peer: Default::default(),
        });
        if let Some(reg) = state.registration_for(target) {
            Some(request.response(302, "Moved Temporarily", &[("Contact", format!("<{}>", reg.contact))]))
        } else if target.starts_with("sip:") || target.starts_with("sips:") {
            Some(request.response(302, "Moved Temporarily", &[("Contact", format!("<{}>", target))]))
        } else {
            None
        }
    };

    // ── DND check ──
    let (is_dnd, dnd_forward) = state.check_dnd(&requested_uri);
    if is_dnd {
        if let Some(ref fwd) = dnd_forward {
            if let Some(resp) = make_redirect(fwd) { return Some(resp); }
        }
        state.record_cdr_end(&call_id_str, "busy");
        return Some(request.response(486, "Busy Here (Do Not Disturb)", &[]));
    }

    // ── Forward-Always ──
    if let Some(fwd) = state.resolve_call_forwarding(&requested_uri, "always") {
        if let Some(resp) = make_redirect(&fwd) { return Some(resp); }
    }

    // ── Holiday check ──
    if let Some(holiday) = state.active_holiday_today() {
        if let Some(dest) = &holiday.destination {
            if !dest.is_empty() {
                if let Some(resp) = make_redirect(dest) { return Some(resp); }
            }
        }
        // No destination — reject
        state.record_cdr_end(&call_id_str, "no_answer");
        return Some(request.response(480, "Holiday - Office Closed", &[]));
    }

    // ── Business hours check ──
    let (is_open, after_hours_dest) = state.is_within_business_hours();
    if !is_open {
        if let Some(dest) = after_hours_dest {
            if let Some(resp) = make_redirect(&dest) { return Some(resp); }
        }
        state.record_cdr_end(&call_id_str, "no_answer");
        return Some(request.response(480, "Outside Business Hours", &[]));
    }

    // ── Conference routing ──
    if let Some(conference) = state.conference_by_uri(&requested_uri) {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(), from_uri: from_aor.clone(),
            to_uri: requested_uri.clone(), target_contact: None,
            status: SipDialogStatus::Ringing, media_types: media.clone(),
            peer: Default::default(),
        });
        if conference.active {
            state.record_cdr_end(&call_id_str, "answered");
            return Some(request.response(200, "OK", &[]));
        }
        state.record_cdr_end(&call_id_str, "no_answer");
        return Some(request.response(480, "Conference Not Active", &[]));
    }

    // ── Queue/ACD routing ──
    if let Some(queue) = state.queue_by_extension(&requested_uri) {
        // VIP check: priority routing
        let vip = state.check_vip(&from_aor);
        if let Some(ref vip_info) = vip {
            if let Some(ref agent_uri) = vip_info.agent_override {
                if let Some(reg) = state.registration_for(agent_uri) {
                    let _ = state.transition_agent_state(agent_uri, "on_call", Some("vip_direct".to_string()));
                    let caller = state.enqueue_caller(queue.id, &from_aor, "");
                    state.dequeue_caller(caller.id, agent_uri);
                    state.upsert_sip_dialog(UpsertSipDialog {
                        call_id: call_id_str.clone(), from_uri: from_aor.clone(),
                        to_uri: requested_uri.clone(), target_contact: Some(agent_uri.clone()),
                        status: SipDialogStatus::Ringing, media_types: media.clone(),
                        peer: Default::default(),
                    });
                    return Some(request.response(302, "Moved Temporarily",
                        &[("Contact", format!("<{}>", reg.contact))]));
                }
            }
        }

        // Check queue capacity
        let waiting = state.queue_callers_waiting_count(queue.id);
        if waiting >= queue.max_queue_size as usize {
            if let Some(overflow) = &queue.overflow_destination {
                if let Some(resp) = make_redirect(overflow) { return Some(resp); }
            }
            state.record_cdr_end(&call_id_str, "no_answer");
            return Some(request.response(480, "Queue Full", &[]));
        }

        // Try to claim an agent atomically
        let required_skills: Vec<String> = vec![];
        if let Some(agent_uri) = state.claim_next_agent(&queue, &required_skills) {
            if let Some(reg) = state.registration_for(&agent_uri) {
                let caller = state.enqueue_caller(queue.id, &from_aor, "");
                state.dequeue_caller(caller.id, &agent_uri);
                state.upsert_sip_dialog(UpsertSipDialog {
                    call_id: call_id_str.clone(), from_uri: from_aor.clone(),
                    to_uri: requested_uri.clone(), target_contact: Some(agent_uri.clone()),
                    status: SipDialogStatus::Ringing, media_types: media.clone(),
                    peer: Default::default(),
                });
                return Some(request.response(302, "Moved Temporarily",
                    &[("Contact", format!("<{}>", reg.contact))]));
            } else {
                // Agent not registered - release them
                let _ = state.transition_agent_state(&agent_uri, "available", Some("not_registered".to_string()));
            }
        }

        // No agent available - enqueue caller and accept the call (hold music)
        let _caller = state.enqueue_caller(queue.id, &from_aor, "");
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(), from_uri: from_aor.clone(),
            to_uri: requested_uri.clone(), target_contact: None,
            status: SipDialogStatus::Queued, media_types: media.clone(),
            peer: Default::default(),
        });
        // Caller waits in queue (200 OK - server plays hold music)
        return Some(request.response(200, "OK", &[]));
    }

    // ── Ring group routing (with strategy) ──
    if let Some(group) = state.ring_group_by_extension(&requested_uri) {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(), from_uri: from_aor.clone(),
            to_uri: requested_uri.clone(), target_contact: None,
            status: SipDialogStatus::Ringing, media_types: media.clone(),
            peer: Default::default(),
        });
        let members = match group.strategy {
            crate::RingStrategy::Random => {
                use rand::seq::SliceRandom;
                let mut m = group.members.clone();
                m.shuffle(&mut rand::thread_rng());
                m
            }
            _ => group.members.clone(), // Sequential and Simultaneous use original order
        };
        if group.strategy == crate::RingStrategy::Simultaneous {
            // Collect all registered contacts for SIP forking
            let contacts: Vec<String> = members.iter()
                .filter_map(|m| state.registration_for(m))
                .map(|r| format!("<{}>", r.contact))
                .collect();
            if !contacts.is_empty() {
                return Some(request.response(302, "Moved Temporarily",
                    &[("Contact", contacts.join(", "))]));
            }
        } else {
            // Sequential / Random: find first registered member
            for member in &members {
                if let Some(reg) = state.registration_for(member) {
                    return Some(request.response(302, "Moved Temporarily",
                        &[("Contact", format!("<{}>", reg.contact))]));
                }
            }
        }
        // Fallback
        if let Some(fallback) = &group.fallback_uri {
            if let Some(reg) = state.registration_for(fallback) {
                return Some(request.response(302, "Moved Temporarily",
                    &[("Contact", format!("<{}>", reg.contact))]));
            }
        }
        state.record_cdr_end(&call_id_str, "no_answer");
        return Some(request.response(480, "No Group Members Available", &[]));
    }

    // ── IVR routing ──
    if let Some(_ivr) = state.ivr_by_extension(&requested_uri) {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(), from_uri: from_aor.clone(),
            to_uri: requested_uri.clone(), target_contact: None,
            status: SipDialogStatus::Ringing, media_types: media.clone(),
            peer: Default::default(),
        });
        state.record_cdr_end(&call_id_str, "answered");
        return Some(request.response(200, "OK", &[]));
    }

    // ── Extension resolution ──
    let user_part = crate::sip_user_part(&requested_uri);
    if let Some(ext) = state.resolve_extension(user_part) {
        match ext.destination_type.as_str() {
            "voicemail" => {
                state.create_voicemail_for_user(&ext.destination, &from_aor, "", 0, None);
                state.record_cdr_end(&call_id_str, "voicemail");
                return Some(request.response(200, "OK", &[]));
            }
            _ => {
                // user, external, or other — redirect to destination
                if let Some(resp) = make_redirect(&ext.destination) { return Some(resp); }
            }
        }
    }

    // ── Routing rules ──
    let routed_uri = state
        .resolve_routing_rule(&from_aor, &requested_uri, "INVITE", &request.headers_for_routing())
        .map(|rule| rule.target)
        .unwrap_or_else(|| requested_uri.clone());

    // ── Direct registration lookup ──
    if let Some(registration) = state.registration_for(&routed_uri) {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(), from_uri: from_aor.clone(),
            to_uri: requested_uri.clone(), target_contact: Some(registration.contact.clone()),
            status: SipDialogStatus::Ringing, media_types: media.clone(),
            peer: Default::default(),
        });
        return Some(request.response(302, "Moved Temporarily",
            &[("Contact", format!("<{}>", registration.contact))]));
    }

    // ── Follow-me sequential dialing ──
    let settings = state.get_user_call_settings(&requested_uri);
    if settings.followme_enabled && !settings.followme_numbers.is_empty() {
        for entry in &settings.followme_numbers {
            if let Some(reg) = state.registration_for(&entry.number) {
                state.upsert_sip_dialog(UpsertSipDialog {
                    call_id: call_id_str.clone(), from_uri: from_aor.clone(),
                    to_uri: requested_uri.clone(), target_contact: Some(reg.contact.clone()),
                    status: SipDialogStatus::Ringing, media_types: media.clone(),
                    peer: Default::default(),
                });
                return Some(request.response(302, "Moved Temporarily",
                    &[("Contact", format!("<{}>", reg.contact))]));
            }
        }
        // Follow-me final action
        match settings.followme_final.as_str() {
            "voicemail" => {
                state.create_voicemail_for_user(&requested_uri, &from_aor, "", 0, None);
                state.record_cdr_end(&call_id_str, "voicemail");
                return Some(request.response(200, "OK", &[]));
            }
            "hangup" => {
                state.record_cdr_end(&call_id_str, "no_answer");
                return Some(request.response(480, "Temporarily Unavailable", &[]));
            }
            uri if !uri.is_empty() => {
                if let Some(resp) = make_redirect(uri) { return Some(resp); }
            }
            _ => {}
        }
    }

    // ── Forward-no-answer / voicemail fallback ──
    if let Some(fwd) = state.resolve_call_forwarding(&requested_uri, "no_answer") {
        if let Some(resp) = make_redirect(&fwd) { return Some(resp); }
    }
    if settings.voicemail_enabled {
        state.create_voicemail_for_user(&requested_uri, &from_aor, "", 0, None);
        state.record_cdr_end(&call_id_str, "voicemail");
        return Some(request.response(200, "OK", &[]));
    }

    // ── Forward to external if routing rules resolved a different URI ──
    if routed_uri != requested_uri && routed_uri.starts_with("sip:") {
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: call_id_str.clone(), from_uri: from_aor,
            to_uri: requested_uri, target_contact: Some(routed_uri.clone()),
            status: SipDialogStatus::Routing, media_types: media,
            peer: Default::default(),
        });
        return Some(request.response(302, "Moved Temporarily",
            &[("Contact", format!("<{}>", routed_uri))]));
    }

    // ── 480 Unavailable ──
    state.record_cdr_end(&call_id_str, "no_answer");
    Some(request.response(480, "Temporarily Unavailable", &[]))
}

fn handle_ack(request: &SipRequest, state: &AppState) -> Option<String> {
    // ACK is never challenged (RFC 3261 §22.2 — its Authorization carries the
    // INVITE's already-consumed nonce) and confirms the dialog.
    if let Some(call_id) = request.call_id() {
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Answered);
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
    match request.call_id().map(|call_id| state.dialog_exists(call_id)) {
        Some(true) => bye_bookkeeping(request, state),
        Some(false) => {
            return Some(request.response(481, "Call/Transaction Does Not Exist", &[]))
        }
        None => {}
    }
    Some(request.response(200, "OK", &[]))
}

fn handle_cancel(request: &SipRequest, state: &AppState) -> Option<String> {
    // RFC 3261 §22.1: a CANCEL MUST NOT be challenged — clients cannot supply
    // fresh credentials in it. It is answered hop-by-hop with 200 OK.
    if let Some(call_id) = request.call_id() {
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Cancelled);
        // Finalize CDR with cancelled/no_answer disposition
        state.record_cdr_end(call_id, "no_answer");
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
    // In-dialog INFO for proxied dialogs is relayed before reaching here;
    // this local path only answers dialogs the server itself terminates.
    if let Some(call_id) = request.call_id() {
        if !state.dialog_exists(call_id) {
            return Some(request.response(481, "Call/Transaction Does Not Exist", &[]));
        }
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

    let refer_to_raw = request.header("refer-to").map(|v| v.trim().to_string());
    let refer_to = refer_to_raw
        .as_deref()
        .and_then(extract_sip_uri)
        .or_else(|| refer_to_raw.clone());

    let Some(target) = refer_to else {
        return Some(request.response(400, "Bad Request", &[]));
    };

    // ── Call Park / Pickup via REFER ──
    let target_user = crate::sip_user_part(&target);
    if let Some(slot) = target_user.strip_prefix("park-") {
        let call_id = request.call_id().unwrap_or_default();
        let from_uri = request
            .header("from")
            .and_then(extract_sip_uri)
            .unwrap_or_default();
        // parked_by = person parking (from), caller_uri = original caller (to of the dialog)
        let to_uri = request.uri();
        state.park_call(slot, call_id, &from_uri, &to_uri, "");
        if let Some(cid) = request.call_id() {
            let _ = state.update_sip_dialog_status(cid, SipDialogStatus::Ended);
        }
        state.record_audit_event(&from_uri, "call.parked", Some(format!("slot={}", slot)));
        // Subscription-State is only legal on NOTIFY (RFC 6665 §7.2); the
        // implicit subscription is closed by the NOTIFY the async path sends.
        return Some(request.response(202, "Accepted", &[]));
    }
    if let Some(slot) = target_user.strip_prefix("pickup-") {
        let from_uri = request
            .header("from")
            .and_then(extract_sip_uri)
            .unwrap_or_default();
        if let Some(parked) = state.pickup_parked_call(slot) {
            // Redirect the retriever to the original caller
            if let Some(cid) = request.call_id() {
                let _ = state.update_sip_dialog_status(cid, SipDialogStatus::Ended);
            }
            state.record_audit_event(&from_uri, "call.pickup", Some(format!("slot={}", slot)));
            if let Some(reg) = state.registration_for(&parked.caller_uri) {
                return Some(request.response(
                    302,
                    "Moved Temporarily",
                    &[("Contact", format!("<{}>", reg.contact))],
                ));
            }
            return Some(request.response(
                302,
                "Moved Temporarily",
                &[("Contact", format!("<{}>", parked.caller_uri))],
            ));
        }
        return Some(request.response(480, "No Call Parked In Slot", &[]));
    }

    // Check for Replaces header in the Refer-To URI (attended transfer)
    // Format: <sip:target?Replaces=call-id%3Bto-tag%3D...%3Bfrom-tag%3D...>
    let is_attended = refer_to_raw
        .as_deref()
        .map(|v| v.contains("Replaces=") || v.contains("replaces="))
        .unwrap_or(false);

    let replaces_call_id = if is_attended {
        refer_to_raw
            .as_deref()
            .and_then(|v| v.split("Replaces=").nth(1).or_else(|| v.split("replaces=").nth(1)))
            .map(|v| v.split('%').next().unwrap_or(v))
            .map(|v| v.split(';').next().unwrap_or(v))
            .map(|v| v.split('>').next().unwrap_or(v))
            .map(ToOwned::to_owned)
    } else {
        None
    };

    // The transfer dialogs are NOT marked Ended here: a blind-transfer dialog
    // ends when the subsequent BYE arrives (RFC 3515 §2.4.5). Only the
    // attended-transfer consultation leg, which Replaces consumes, is closed.
    if let Some(replaces_id) = &replaces_call_id {
        let _ = state.update_sip_dialog_status(replaces_id, SipDialogStatus::Ended);
    }

    let from_uri = request
        .header("from")
        .and_then(extract_sip_uri)
        .unwrap_or_default();

    let transfer_type = if is_attended { "call.attended_transfer" } else { "call.blind_transfer" };
    state.record_audit_event(&from_uri, transfer_type, Some(target.clone()));

    Some(request.response(202, "Accepted", &[]))
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

    // Resolve the target BEFORE the auth check: is_authorized consumes the
    // single-use nonce, and an unresolvable target must fall through to
    // handle_message's store-and-forward with the nonce still valid —
    // otherwise the fallback re-challenges and the client loops on 401.
    let to_uri = request.uri();
    let (target_contact, target_addr) = invite_target(state, &to_uri)?;
    if !request.is_authorized(state, &realm) {
        return None;
    }

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
         Max-Forwards: {}\r\n\
         From: {}\r\n\
         To: {}\r\n\
         Call-ID: {}\r\n\
         CSeq: {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\r\n{}",
        target_contact,
        sip_sent_by(local_addr),
        branch,
        max_forwards(request).unwrap_or(70).saturating_sub(1),
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
    peer: SocketAddr,
    state: &AppState,
) -> Option<String> {
    let received_branch = top_via_branch(request);
    let result = proxy_invite_inner(server_socket, packet, request, peer, state, &received_branch).await;
    if let Some(branch) = &received_branch {
        state.remove_pending_invite(branch);
    }
    result
}

async fn proxy_invite_inner(
    server_socket: &UdpSocket,
    packet: &str,
    request: &SipRequest,
    peer: SocketAddr,
    state: &AppState,
    received_branch: &Option<String>,
) -> Option<String> {
    let Some(from_aor) = request.header("from").and_then(extract_sip_uri) else {
        return None;
    };
    let Some((_, realm)) = split_sip_aor(&from_aor) else {
        return None;
    };
    if max_forwards(request) == Some(0) {
        return Some(request.response(483, "Too Many Hops", &[]));
    }
    let requested_uri = request.uri();
    let routing_rule = state.resolve_routing_rule(
        &from_aor,
        &requested_uri,
        &request.method(),
        &request.headers_for_routing(),
    );
    let routed_uri = routing_rule
        .as_ref()
        .map(|rule| rule.target.clone())
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
    let header_actions = routing_rule
        .as_ref()
        .map(|rule| rule.header_actions.as_slice())
        .unwrap_or(&[]);
    let Some(forwarded) = rewrite_request_for_proxy(packet, &target_contact, local_addr, &branch, header_actions)
    else {
        return Some(request.response(400, "Bad Request", &[]));
    };

    state.upsert_sip_dialog(UpsertSipDialog {
        call_id: request.call_id().unwrap_or_default().to_string(),
        from_uri: from_aor,
        to_uri: requested_uri,
        target_contact: Some(target_contact.clone()),
        status: SipDialogStatus::Routing,
        media_types: extract_media_types(packet),
        peer: crate::DialogPeerInfo {
            from_contact: request.header("contact").and_then(extract_sip_uri),
            from_source: Some(peer.to_string()),
            to_source: Some(target_addr.to_string()),
        },
    });

    if proxy_socket
        .send_to(forwarded.as_bytes(), target_addr)
        .await
        .is_err()
    {
        return Some(request.response(502, "Bad Gateway", &[]));
    }

    // Register for CANCEL matching only once the INVITE is actually in flight.
    let cancel_notify = received_branch
        .as_deref()
        .map(|branch| state.register_pending_invite(branch));
    let mut cancel_requested = false;

    // Relay every provisional immediately; return on the final response or
    // an overall Timer C-style deadline (RFC 3261 §16.6/§16.8).
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(32);
    let mut buf = vec![0_u8; 16 * 1024];
    loop {
        let received = tokio::select! {
            received = proxy_socket.recv_from(&mut buf) => received,
            _ = async {
                match &cancel_notify {
                    Some(notify) => notify.notified().await,
                    None => std::future::pending().await,
                }
            }, if !cancel_requested => {
                cancel_requested = true;
                let cancel = build_proxy_cancel(request, &target_contact, local_addr, &branch);
                let _ = proxy_socket.send_to(cancel.as_bytes(), target_addr).await;
                continue;
            }
            _ = tokio::time::sleep_until(deadline) => {
                // Stop the UAS from ringing forever, then report the timeout.
                if !cancel_requested {
                    let cancel = build_proxy_cancel(request, &target_contact, local_addr, &branch);
                    let _ = proxy_socket.send_to(cancel.as_bytes(), target_addr).await;
                }
                if let Some(call_id) = request.call_id() {
                    let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Failed);
                }
                return Some(request.response(504, "Gateway Timeout", &[]));
            }
        };
        let Ok((len, _)) = received else {
            return Some(request.response(502, "Bad Gateway", &[]));
        };
        let text = String::from_utf8_lossy(&buf[..len]).to_string();
        // Responses to our hop-by-hop CANCEL are consumed here, not relayed.
        if response_cseq_method(&text).as_deref() == Some("CANCEL") {
            continue;
        }
        let relayed = strip_proxy_via(&text, &branch).unwrap_or_else(|| text.clone());
        let Some((status, _)) = parse_response_status(&relayed) else {
            continue;
        };
        if status < 200 {
            let _ = server_socket.send_to(relayed.as_bytes(), peer).await;
            if let Some(call_id) = request.call_id() {
                let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Ringing);
            }
            continue;
        }
        // Final response: ACK non-2xx hop-by-hop so the UAS stops
        // retransmitting (RFC 3261 §17.1.1.3). The 2xx is ACKed end-to-end
        // by the caller directly (we do not Record-Route).
        if !(200..300).contains(&status) {
            let ack = build_proxy_ack(request, &target_contact, local_addr, &branch, &relayed);
            let _ = proxy_socket.send_to(ack.as_bytes(), target_addr).await;
        }
        let dialog_status = if (200..300).contains(&status) {
            SipDialogStatus::Answered
        } else if cancel_requested || status == 487 {
            SipDialogStatus::Cancelled
        } else {
            SipDialogStatus::Ended
        };
        if let Some(call_id) = request.call_id() {
            let _ = state.update_sip_dialog_status(call_id, dialog_status);
        }
        return Some(relayed);
    }
}

/// Relay an in-dialog request (INFO, BYE, UPDATE, re-INVITE, REFER, ACK) to
/// the dialog's peer leg and pass the peer's response back. Returns `None`
/// when the dialog is unknown or has no peer addressing, so the caller can
/// fall through to local handling.
async fn relay_in_dialog_request(
    server_socket: &UdpSocket,
    packet: &str,
    request: &SipRequest,
    state: &AppState,
) -> Option<String> {
    let call_id = request.call_id()?;
    let dialog = state.dialog_for(call_id)?;
    let from_uri = request.header("from").and_then(extract_sip_uri)?;

    // Direction: requests from the dialog's caller go toward the callee leg.
    let toward_callee = crate::sip_user_part(&from_uri) == crate::sip_user_part(&dialog.from_uri);
    let (contact, source, fallback_uri) = if toward_callee {
        (dialog.target_contact, dialog.to_source, dialog.to_uri)
    } else {
        (dialog.from_contact, dialog.from_source, dialog.from_uri)
    };
    let target_uri = contact.unwrap_or(fallback_uri);
    let target_addr: SocketAddr = match source.as_deref().and_then(|s| s.parse().ok()) {
        Some(addr) => addr,
        None => invite_target(state, &target_uri).map(|(_, addr)| addr)?,
    };

    if max_forwards(request) == Some(0) {
        return Some(request.response(483, "Too Many Hops", &[]));
    }

    // Digest-check locally registered senders. The remote leg of a dialog
    // cannot answer a local-realm challenge, so unregistered senders relay
    // through (their requests are scoped to an established dialog). ACK is
    // never challengeable (RFC 3261 §22).
    let method = request.method();
    if method != "ACK" && state.registration_for(&from_uri).is_some() {
        if let Some((_, realm)) = split_sip_aor(&from_uri) {
            if !request.is_authorized(state, &realm) {
                return Some(request.digest_challenge(&realm, state.issue_sip_nonce()));
            }
        }
    }

    let branch = format!("z9hG4bK-pale-{}", Uuid::new_v4());
    let proxy_bind = server_socket.local_addr().ok().map(proxy_bind_addr)?;
    let proxy_socket = UdpSocket::bind(proxy_bind).await.ok()?;
    let local_addr = proxy_socket.local_addr().ok()?;
    let forwarded = rewrite_request_for_proxy(packet, &target_uri, local_addr, &branch, &[])?;
    proxy_socket
        .send_to(forwarded.as_bytes(), target_addr)
        .await
        .ok()?;

    if method == "ACK" {
        // ACK confirms the dialog; it has no response to relay.
        let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Answered);
        return None;
    }

    let mut buf = vec![0_u8; 16 * 1024];
    let relayed = match timeout(StdDuration::from_secs(5), proxy_socket.recv_from(&mut buf)).await
    {
        Ok(Ok((len, _))) => {
            let text = String::from_utf8_lossy(&buf[..len]).to_string();
            strip_proxy_via(&text, &branch).unwrap_or(text)
        }
        Ok(Err(_)) => request.response(502, "Bad Gateway", &[]),
        Err(_) => request.response(408, "Request Timeout", &[]),
    };

    match method.as_str() {
        "BYE" => bye_bookkeeping(request, state),
        "REFER" => {
            let transfer_type = if packet.contains("Replaces=") || packet.contains("replaces=") {
                "call.attended_transfer"
            } else {
                "call.blind_transfer"
            };
            state.record_audit_event(&from_uri, transfer_type, request.header("refer-to").map(ToOwned::to_owned));
        }
        _ => {}
    }

    Some(relayed)
}

/// Shared BYE bookkeeping: dialog teardown, CDR disposition derived from the
/// dialog state at hangup time, and agent wrap-up transition.
fn bye_bookkeeping(request: &SipRequest, state: &AppState) {
    let Some(call_id) = request.call_id() else {
        return;
    };
    let disposition = match state.dialog_for(call_id).map(|d| d.status) {
        Some(SipDialogStatus::Answered) | Some(SipDialogStatus::Held) => "answered",
        _ => "no_answer",
    };
    let _ = state.update_sip_dialog_status(call_id, SipDialogStatus::Ended);
    state.record_cdr_end(call_id, disposition);
    if let Some(from_uri) = request.header("from").and_then(extract_sip_uri) {
        if let Some(profile) = state.agent_profile(&from_uri) {
            if profile.state == "on_call" {
                let _ = state.transition_agent_state(&from_uri, "wrap_up", Some("call_ended".to_string()));
            }
        }
    }
}

/// Build the hop-by-hop CANCEL for a forwarded INVITE: same R-URI, same Via
/// branch as the INVITE it cancels (RFC 3261 §9.1).
fn build_proxy_cancel(
    request: &SipRequest,
    target_uri: &str,
    proxy_addr: SocketAddr,
    branch: &str,
) -> String {
    let cseq_num = request
        .header("cseq")
        .and_then(|cseq| cseq.split_whitespace().next())
        .unwrap_or("1");
    format!(
        "CANCEL {} SIP/2.0\r\n\
         Via: SIP/2.0/UDP {};branch={}\r\n\
         Max-Forwards: 70\r\n\
         From: {}\r\n\
         To: {}\r\n\
         Call-ID: {}\r\n\
         CSeq: {} CANCEL\r\n\
         Content-Length: 0\r\n\r\n",
        target_uri,
        sip_sent_by(proxy_addr),
        branch,
        request.header("from").unwrap_or(""),
        request.header("to").unwrap_or(""),
        request.call_id().unwrap_or(""),
        cseq_num,
    )
}

/// Build the hop-by-hop ACK for a non-2xx final response to a forwarded
/// INVITE (RFC 3261 §17.1.1.3). To is taken from the response (it carries
/// the UAS to-tag).
fn build_proxy_ack(
    request: &SipRequest,
    target_uri: &str,
    proxy_addr: SocketAddr,
    branch: &str,
    response: &str,
) -> String {
    let cseq_num = request
        .header("cseq")
        .and_then(|cseq| cseq.split_whitespace().next())
        .unwrap_or("1");
    let to = response_header_line(response, "to")
        .unwrap_or_else(|| request.header("to").unwrap_or("").to_string());
    format!(
        "ACK {} SIP/2.0\r\n\
         Via: SIP/2.0/UDP {};branch={}\r\n\
         Max-Forwards: 70\r\n\
         From: {}\r\n\
         To: {}\r\n\
         Call-ID: {}\r\n\
         CSeq: {} ACK\r\n\
         Content-Length: 0\r\n\r\n",
        target_uri,
        sip_sent_by(proxy_addr),
        branch,
        request.header("from").unwrap_or(""),
        to,
        request.call_id().unwrap_or(""),
        cseq_num,
    )
}

/// Send the RFC 3515 terminal NOTIFY that closes a REFER's implicit
/// subscription, carrying the transfer result as a message/sipfrag body.
async fn send_refer_notify(
    socket: &UdpSocket,
    request: &SipRequest,
    peer: SocketAddr,
    sipfrag: &str,
) {
    let Ok(local_addr) = socket.local_addr() else {
        return;
    };
    let target_uri = request
        .header("contact")
        .and_then(extract_sip_uri)
        .unwrap_or_else(|| format!("sip:{}", sip_sent_by(peer)));
    let refer_cseq = request
        .header("cseq")
        .and_then(|cseq| cseq.split_whitespace().next())
        .unwrap_or("1");
    // From/To are swapped relative to the REFER; the notifier (us) needs a
    // tag on From when the REFER's To had none.
    let mut from = request.header("to").unwrap_or("").to_string();
    if !from.contains("tag=") {
        from = format!("{};tag={}", from, request.derived_to_tag());
    }
    let to = request.header("from").unwrap_or("");
    let body = format!("{}\r\n", sipfrag);
    let notify = format!(
        "NOTIFY {} SIP/2.0\r\n\
         Via: SIP/2.0/UDP {};branch=z9hG4bK-pale-{}\r\n\
         Max-Forwards: 70\r\n\
         From: {}\r\n\
         To: {}\r\n\
         Call-ID: {}\r\n\
         CSeq: 1 NOTIFY\r\n\
         Event: refer;id={}\r\n\
         Subscription-State: terminated;reason=noresource\r\n\
         Contact: <sip:pale@{}>\r\n\
         Content-Type: message/sipfrag;version=2.0\r\n\
         Content-Length: {}\r\n\r\n{}",
        target_uri,
        sip_sent_by(local_addr),
        Uuid::new_v4(),
        from,
        to,
        request.call_id().unwrap_or(""),
        refer_cseq,
        sip_sent_by(local_addr),
        body.len(),
        body,
    );
    let _ = socket.send_to(notify.as_bytes(), peer).await;
}

/// True when a REFER's Refer-To targets the server's own park/pickup logic.
fn refer_targets_server(request: &SipRequest) -> bool {
    let refer_to = request
        .header("refer-to")
        .map(|v| v.trim().to_string());
    let target = refer_to
        .as_deref()
        .and_then(extract_sip_uri)
        .or(refer_to.clone());
    match target {
        Some(target) => {
            let user = crate::sip_user_part(&target);
            user.starts_with("park-") || user.starts_with("pickup-")
        }
        None => false,
    }
}

/// RFC 4488: Refer-Sub: false asks for no implicit subscription (no NOTIFY).
fn refer_sub_disabled(request: &SipRequest) -> bool {
    request
        .header("refer-sub")
        .map(|v| v.trim().eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

fn top_via_branch(request: &SipRequest) -> Option<String> {
    request.header("via").and_then(|via| {
        via.split(';')
            .find_map(|param| param.trim().strip_prefix("branch="))
            .map(|branch| branch.trim().to_string())
    })
}

fn max_forwards(request: &SipRequest) -> Option<u32> {
    request
        .header("max-forwards")
        .and_then(|value| value.trim().parse().ok())
}

/// Extract a header line's value from a raw response text (used where the
/// response is not worth fully parsing).
fn response_header_line(response: &str, name: &str) -> Option<String> {
    let lower = format!("{}:", name.to_ascii_lowercase());
    for line in response.split("\r\n") {
        if line.is_empty() {
            break;
        }
        if line.to_ascii_lowercase().starts_with(&lower) {
            return line.splitn(2, ':').nth(1).map(|v| v.trim().to_string());
        }
    }
    None
}

fn response_cseq_method(response: &str) -> Option<String> {
    response_header_line(response, "cseq")
        .and_then(|cseq| cseq.split_whitespace().nth(1).map(ToOwned::to_owned))
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

/// Rewrite a request for forwarding: retarget the R-URI, push our Via on top
/// of the existing chain, and decrement Max-Forwards (RFC 3261 §16.6,
/// inserting 69 when the original had none).
fn rewrite_request_for_proxy(
    packet: &str,
    target_uri: &str,
    proxy_addr: SocketAddr,
    branch: &str,
    header_actions: &[SipHeaderAction],
) -> Option<String> {
    let mut lines = packet.split_inclusive("\r\n");
    let first_line = lines.next()?;
    let mut parts = first_line.trim_end_matches("\r\n").split_whitespace();
    let method = parts.next()?;
    let _uri = parts.next()?;
    let version = parts.next().unwrap_or("SIP/2.0");

    let mut out = String::new();
    out.push_str(&format!("{method} {target_uri} {version}\r\n"));
    out.push_str(&format!(
        "Via: SIP/2.0/UDP {};branch={branch}\r\n",
        sip_sent_by(proxy_addr)
    ));
    let mut wrote_max_forwards = false;
    let mut in_headers = true;
    for line in lines {
        if in_headers {
            if line == "\r\n" {
                if !wrote_max_forwards {
                    out.push_str("Max-Forwards: 69\r\n");
                }
                for action in header_actions {
                    match action.kind {
                        SipHeaderActionKind::Add | SipHeaderActionKind::Set if !action.value.is_empty() => {
                            out.push_str(&format!("{}: {}\r\n", action.name, action.value));
                        }
                        SipHeaderActionKind::Add | SipHeaderActionKind::Set | SipHeaderActionKind::Remove => {}
                    }
                }
                in_headers = false;
                out.push_str(line);
                continue;
            }
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("max-forwards:") {
                let value: u32 = line
                    .splitn(2, ':')
                    .nth(1)
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(70);
                out.push_str(&format!("Max-Forwards: {}\r\n", value.saturating_sub(1)));
                wrote_max_forwards = true;
                continue;
            }
            if let Some((name, _)) = line.split_once(':') {
                if header_actions.iter().any(|action| {
                    matches!(action.kind, SipHeaderActionKind::Remove | SipHeaderActionKind::Set)
                        && name.trim().eq_ignore_ascii_case(&action.name)
                }) {
                    continue;
                }
            }
        }
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

    fn headers_for_routing(&self) -> Vec<(String, String)> {
        self.inner
            .headers
            .iter()
            .filter_map(|header| {
                let line = header.to_string();
                let (name, value) = line.split_once(':')?;
                Some((canonical_header_name(name), value.trim().to_string()))
            })
            .collect()
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

    /// Deterministic local tag for this request's To header (RFC 3261
    /// §8.2.6.2 requires one on every non-100 response). Stable across
    /// retransmissions of the same request.
    fn derived_to_tag(&self) -> String {
        let seed = format!(
            "{}|{}|{}",
            self.call_id().unwrap_or(""),
            self.header("from").unwrap_or(""),
            self.header("cseq").unwrap_or(""),
        );
        md5_hex(seed.as_bytes())[..16].to_string()
    }

    fn response(&self, code: u16, reason: &str, extra_headers: &[(&str, String)]) -> String {
        let mut headers = Vec::new();
        for header in ["via", "from", "to", "call-id", "cseq"] {
            if let Some(value) = self.header(header) {
                let value = if header == "to" && code != 100 && !value.contains("tag=") {
                    format!("{};tag={}", value, self.derived_to_tag())
                } else {
                    value.to_string()
                };
                headers.push(response_header(header, value));
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
            // The proxy must decrement Max-Forwards (or insert 69 when absent).
            assert!(forwarded.contains("Max-Forwards: 69\r\n"));
            let top_via = forwarded
                .lines()
                .find(|line| line.contains("z9hG4bK-pale-"))
                .unwrap()
                .to_string();
            let tail = "Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bKcaller\r\nFrom: <sip:alice@example.com>;tag=1\r\nTo: <sip:bob@example.com>;tag=2\r\nCall-ID: invite-proxy\r\nCSeq: 1 INVITE\r\nContent-Length: 0\r\n\r\n";
            // 180 first, then the 200 final — the proxy must relay BOTH.
            for status_line in ["SIP/2.0 180 Ringing", "SIP/2.0 200 OK"] {
                let response = format!("{}\r\n{}\r\n{}", status_line, top_via, tail);
                downstream
                    .send_to(response.as_bytes(), proxy_addr)
                    .await
                    .unwrap();
            }
        });

        let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        // A real caller socket so the mid-flight provisional relay can be observed.
        let caller_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer: SocketAddr = caller_socket.local_addr().unwrap();
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
        // The provisional was relayed to the caller out-of-band...
        let mut buf = vec![0_u8; 8192];
        let (len, _) = tokio::time::timeout(
            StdDuration::from_secs(2),
            caller_socket.recv_from(&mut buf),
        )
        .await
        .expect("provisional relayed to caller")
        .unwrap();
        let provisional = String::from_utf8_lossy(&buf[..len]).to_string();
        assert!(provisional.starts_with("SIP/2.0 180 Ringing"));
        assert!(!provisional.contains("z9hG4bK-pale-"));
        // ...and the FINAL response is what the transaction returns.
        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert!(!response.contains("z9hG4bK-pale-"));
        assert!(response.contains("z9hG4bKcaller"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Answered);
        // Peer addressing was captured for in-dialog relays.
        let dialog = state.dialog_for("invite-proxy").unwrap();
        assert!(dialog.to_source.is_some());
        assert_eq!(dialog.from_source.as_deref(), Some(peer.to_string().as_str()));
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
            peer: Default::default(),
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
            peer: Default::default(),
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
            peer: Default::default(),
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "INVITE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>;tag=2\r\n\
Call-ID: reinvite-1\r\n\
CSeq: 2 INVITE\r\n\
Authorization: {}\r\n\r\nv=0\r\nm=audio 5004 RTP/AVP 0\r\na=sendonly\r\n", auth);

        let response = handle_packet(&packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Held);
    }

    #[test]
    fn tagless_invite_with_stale_call_id_is_not_a_re_invite() {
        let state = test_state();
        // An Ended dialog whose Call-ID a new (tag-less) INVITE happens to reuse.
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "stale-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: None,
            status: SipDialogStatus::Ended,
            media_types: vec![],
            peer: Default::default(),
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "INVITE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: stale-1\r\n\
CSeq: 1 INVITE\r\n\
Authorization: {}\r\n\r\n", auth);

        let _response = handle_packet(&packet, peer, &state).unwrap();
        // Takes the initial-INVITE routing path: the re-INVITE shortcut would
        // have flipped the dialog to Answered/Held — that must not happen for
        // a tag-less INVITE, even when its Call-ID matches a stale dialog.
        let status = state.dialog_for("stale-1").unwrap().status;
        assert!(!matches!(
            status,
            SipDialogStatus::Answered | SipDialogStatus::Held
        ));
    }

    #[test]
    fn in_dialog_invite_for_dead_dialog_gets_481() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "INVITE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>;tag=2\r\n\
Call-ID: unknown-dialog\r\n\
CSeq: 2 INVITE\r\n\
Authorization: {}\r\n\r\n", auth);

        let response = handle_packet(&packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 481"));
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
            peer: Default::default(),
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
        // Subscription-State is only legal on NOTIFY, not on the 202.
        assert!(!response.contains("Subscription-State"));
        // RFC 3515: the REFER alone does not end the dialog — the BYE does.
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Ringing);
    }

    #[test]
    fn cancel_is_never_digest_challenged() {
        let state = test_state();
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "cancel-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: None,
            status: SipDialogStatus::Ringing,
            media_types: vec![],
            peer: Default::default(),
        });
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        // No Authorization header at all — RFC 3261 §22.1 forbids challenging CANCEL.
        let packet = "CANCEL sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: cancel-1\r\n\
CSeq: 1 CANCEL\r\n\r\n";

        let response = handle_packet(packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert_eq!(state.sip_dialogs()[0].status, SipDialogStatus::Cancelled);
    }

    #[test]
    fn bye_for_unknown_dialog_gets_481() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let nonce = state.issue_sip_nonce();
        let auth = authorization("alice", "example.com", "secret", "BYE", "sip:bob@example.com", &nonce);
        let packet = format!(
            "BYE sip:bob@example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>;tag=2\r\n\
Call-ID: nonexistent\r\n\
CSeq: 2 BYE\r\n\
Authorization: {}\r\n\r\n", auth);

        let response = handle_packet(&packet, peer, &state).unwrap();
        assert!(response.starts_with("SIP/2.0 481"));
    }

    #[test]
    fn non_100_responses_carry_a_stable_to_tag() {
        let state = test_state();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        // Unauthorized REGISTER → 401 challenge, which must carry a To-tag.
        let packet = "REGISTER sip:example.com SIP/2.0\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:alice@example.com>\r\n\
Call-ID: tag-test\r\n\
CSeq: 1 REGISTER\r\n\r\n";
        let first = handle_packet(packet, peer, &state).unwrap();
        let second = handle_packet(packet, peer, &state).unwrap();
        let tag_of = |response: &str| {
            response
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("to:"))
                .and_then(|line| line.split("tag=").nth(1))
                .map(|tag| tag.trim().to_string())
        };
        let first_tag = tag_of(&first).expect("To-tag on non-100 response");
        // Retransmissions get the same tag.
        assert_eq!(Some(first_tag), tag_of(&second));
    }

    #[tokio::test]
    async fn in_dialog_info_is_relayed_to_peer_leg() {
        let state = test_state();
        let callee = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let callee_addr = callee.local_addr().unwrap();
        // Established proxied dialog with peer addressing.
        state.upsert_sip_dialog(UpsertSipDialog {
            call_id: "dtmf-1".to_string(),
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            target_contact: Some(format!("sip:bob@{}", callee_addr)),
            status: SipDialogStatus::Answered,
            media_types: vec![],
            peer: crate::DialogPeerInfo {
                from_contact: Some("sip:alice@127.0.0.1:5062".to_string()),
                from_source: Some("127.0.0.1:5062".to_string()),
                to_source: Some(callee_addr.to_string()),
            },
        });

        let callee_task = tokio::spawn(async move {
            let mut buf = vec![0_u8; 8192];
            let (len, proxy_addr) = callee.recv_from(&mut buf).await.unwrap();
            let forwarded = String::from_utf8_lossy(&buf[..len]).to_string();
            // The DTMF INFO reached the callee leg with its body intact.
            assert!(forwarded.starts_with(&format!("INFO sip:bob@{} SIP/2.0", callee_addr)));
            assert!(forwarded.contains("Signal=5"));
            let top_via = forwarded
                .lines()
                .find(|line| line.contains("z9hG4bK-pale-"))
                .unwrap()
                .to_string();
            let response = format!(
                "SIP/2.0 200 OK\r\n{}\r\nFrom: <sip:alice@example.com>;tag=1\r\nTo: <sip:bob@example.com>;tag=2\r\nCall-ID: dtmf-1\r\nCSeq: 2 INFO\r\nContent-Length: 0\r\n\r\n",
                top_via
            );
            callee.send_to(response.as_bytes(), proxy_addr).await.unwrap();
        });

        let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer: SocketAddr = "127.0.0.1:5062".parse().unwrap();
        let body = "Signal=5\r\nDuration=160\r\n";
        let packet = format!(
            "INFO sip:bob@example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bKinfo\r\n\
Max-Forwards: 70\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>;tag=2\r\n\
Call-ID: dtmf-1\r\n\
CSeq: 2 INFO\r\n\
Content-Type: application/dtmf-relay\r\n\
Content-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = handle_udp_packet(&server_socket, &packet, peer, &state)
            .await
            .unwrap();
        callee_task.await.unwrap();
        // The peer's 200 OK came back to the sender, our Via stripped.
        assert!(response.starts_with("SIP/2.0 200 OK"));
        assert!(!response.contains("z9hG4bK-pale-"));
    }

    #[test]
    fn rewrite_decrements_max_forwards_and_handles_any_method() {
        let addr: SocketAddr = "127.0.0.1:5060".parse().unwrap();
        let packet = "INFO sip:bob@example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bKcaller\r\n\
Max-Forwards: 70\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>;tag=2\r\n\
Call-ID: info-1\r\n\
CSeq: 2 INFO\r\n\
Content-Length: 0\r\n\r\n";
        let rewritten =
            rewrite_request_for_proxy(packet, "sip:bob@10.0.0.9:5080", addr, "z9hG4bK-pale-x", &[])
                .unwrap();
        assert!(rewritten.starts_with("INFO sip:bob@10.0.0.9:5080 SIP/2.0"));
        assert!(rewritten.contains("Max-Forwards: 69\r\n"));
        // Original Via chain preserved below ours.
        assert!(rewritten.contains("z9hG4bKcaller"));
        let ours = rewritten.find("z9hG4bK-pale-x").unwrap();
        let theirs = rewritten.find("z9hG4bKcaller").unwrap();
        assert!(ours < theirs);
    }

    #[test]
    fn rewrite_applies_route_header_actions() {
        let addr: SocketAddr = "127.0.0.1:5060".parse().unwrap();
        let packet = "INVITE sip:bob@example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP 127.0.0.1:5062;branch=z9hG4bKcaller\r\n\
Max-Forwards: 70\r\n\
From: <sip:alice@example.com>;tag=1\r\n\
To: <sip:bob@example.com>\r\n\
Call-ID: invite-1\r\n\
CSeq: 1 INVITE\r\n\
P-Asserted-Identity: <sip:old@example.com>\r\n\
X-Legacy: remove-me\r\n\
Content-Length: 0\r\n\r\n";
        let actions = vec![
            SipHeaderAction {
                kind: SipHeaderActionKind::Set,
                name: "P-Asserted-Identity".to_string(),
                value: "<sip:main@example.com>".to_string(),
            },
            SipHeaderAction {
                kind: SipHeaderActionKind::Remove,
                name: "X-Legacy".to_string(),
                value: String::new(),
            },
            SipHeaderAction {
                kind: SipHeaderActionKind::Add,
                name: "X-Pale-Route".to_string(),
                value: "did-main".to_string(),
            },
        ];

        let rewritten = rewrite_request_for_proxy(
            packet,
            "sip:bob@10.0.0.9:5080",
            addr,
            "z9hG4bK-pale-x",
            &actions,
        )
        .unwrap();

        assert!(!rewritten.contains("sip:old@example.com"));
        assert!(!rewritten.contains("X-Legacy"));
        assert!(rewritten.contains("P-Asserted-Identity: <sip:main@example.com>\r\n"));
        assert!(rewritten.contains("X-Pale-Route: did-main\r\n"));
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
