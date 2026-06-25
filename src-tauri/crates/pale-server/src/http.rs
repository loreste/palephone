use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::sse::{Event as SseResponseEvent, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::{
    safe_filename, sip_ha1, AppState, AuthError, CallStatus, CreateCallRequest,
    CreateConferenceRequest, CreateRoutingRuleRequest, CreateSipAccountRequest, CreateUserRequest,
    AddRoomMemberRequest, CreateAgentProfileRequest, CreateBusinessHoursRequest,
    CreateCannedResponseRequest, CreateExtensionRequest, CreateHolidayRequest,
    CreateIvrRequest, CreatePagingGroupRequest, CreateQueueRequest, CreateRingGroupRequest,
    CreateRoomRequest, CreateScorecardRequest, CreateSpeedDialRequest, SetAgentStateRequest,
    StartMonitorRequest, ProvisionUserRequest, AssignExtensionRequest,
    FileRecord, JoinConferenceRequest,
    SendRoomMessageRequest, SetPresenceRequest, SyncCallHistoryRequest,
    UpdateCallStatusRequest, UpdateSipAccountStatusRequest,
    AgentTransitionRequest, CreateVipCallerRequest, RequestCallbackInput,
};

type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    let max_upload_bytes = state.max_upload_bytes().min(usize::MAX as u64) as usize;
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/v1/auth/login", post(user_login))
        .route("/v1/admin/login", post(admin_login))
        .route("/v1/admin/logout", post(admin_logout))
        .route("/v1/admin/refresh", post(admin_refresh))
        .route("/v1/auth/password", put(change_password))
        .route("/v1/admin/audit", get(list_audit_events))
        .route("/v1/users", get(list_users).post(create_user))
        .route("/v1/users/{id}", delete(delete_user))
        .route("/v1/users/{id}/role", put(update_user_role))
        .route("/v1/sip/accounts", get(list_sip_accounts).post(create_sip_account))
        .route(
            "/v1/sip/accounts/{username}/{domain}",
            put(update_sip_account_status).delete(delete_sip_account),
        )
        .route("/v1/sip/registrations", get(list_sip_registrations))
        .route("/v1/sip/dialogs", get(list_sip_dialogs))
        .route("/v1/sip/messages", get(list_sip_messages))
        .route("/v1/sip/transactions", get(list_sip_transactions))
        .route("/v1/conferences", get(list_conferences).post(create_conference))
        .route("/v1/conferences/{id}/participants", post(join_conference))
        .route("/v1/conferences/{id}/participants/{user_id}", delete(leave_conference))
        .route("/v1/media/config", get(media_config))
        .route("/v1/calls", get(list_calls).post(create_call))
        .route("/v1/calls/{id}/status", put(update_call_status))
        .route("/v1/routing/rules", get(list_routing_rules).post(create_routing_rule))
        .route("/v1/routing/rules/{id}", put(update_routing_rule).delete(delete_routing_rule))
        .route("/v1/files", get(list_files).post(upload_file))
        .route("/v1/files/{id}", get(download_file).delete(delete_file))
        .route("/v1/sip/subscriptions", get(list_sip_subscriptions))
        .route("/v1/sip/notifications", get(list_sip_notifications))
        .route("/v1/presence", get(list_presence).put(set_presence))
        .route("/v1/presence/{sip_uri}", get(get_presence))
        .route("/v1/call-history", get(get_call_history).post(sync_call_history))
        .route("/v1/rooms", get(list_rooms).post(create_room))
        .route("/v1/rooms/{id}", get(get_room))
        .route("/v1/rooms/{id}/messages", get(list_room_messages).post(send_room_message))
        .route("/v1/rooms/{id}/members", post(add_room_member).delete(leave_room))
        .route("/v1/rooms/{id}/typing", post(room_typing))
        .route("/v1/search/messages", get(search_messages))
        .route("/v1/messages/{id}/read", put(mark_message_read))
        .route("/v1/messages/{id}", put(edit_message).delete(delete_message))
        .route("/v1/messages/{id}/react", post(react_to_message))
        .route("/v1/messages/{id}/pin", put(pin_message_handler))
        .route("/v1/messages/{id}/reads", get(list_reads_handler))
        .route("/v1/rooms/{id}/pins", get(list_pinned_handler))
        .route("/v1/favorites", get(list_favorites_handler).post(add_favorite_handler))
        .route("/v1/favorites/{uri}", delete(remove_favorite_handler))
        .route("/v1/users/{id}/profile", put(update_profile_handler))
        .route("/v1/users/{id}/avatar", put(upload_avatar))
        .route("/v1/call-settings", get(get_call_settings).put(update_call_settings))
        .route("/v1/queues", get(list_queues).post(create_queue))
        .route("/v1/queues/{id}", get(get_queue).delete(delete_queue))
        .route("/v1/users/provision", post(provision_user_handler))
        .route("/v1/extensions", get(list_extensions).post(create_extension))
        .route("/v1/extensions/{ext}", delete(delete_extension))
        .route("/v1/extensions/{ext}/assign", put(assign_extension_handler))
        .route("/v1/extensions/{ext}/unassign", put(unassign_extension_handler))
        .route("/v1/business-hours", get(list_business_hours).post(create_business_hours))
        .route("/v1/business-hours/{id}", delete(delete_business_hours_entry))
        .route("/v1/holidays", get(list_holidays).post(create_holiday))
        .route("/v1/holidays/{id}", delete(delete_holiday))
        .route("/v1/park", get(list_parked).post(park_call))
        .route("/v1/park/{slot}", post(pickup_call))
        .route("/v1/speed-dials", get(list_speed_dials).post(create_speed_dial))
        .route("/v1/cdrs", get(list_cdrs))
        .route("/v1/paging-groups", get(list_paging_groups).post(create_paging_group))
        .route("/v1/paging-groups/{id}", delete(delete_paging_group))
        .route("/v1/agents", get(list_agents).post(create_agent))
        .route("/v1/agents/{uri}", get(get_agent).delete(delete_agent))
        .route("/v1/agents/{uri}/state", put(set_agent_state))
        .route("/v1/agents/{uri}/transition", post(transition_agent_state_handler))
        .route("/v1/agents/{uri}/history", get(agent_state_history))
        .route("/v1/queues/{id}/callers", get(list_queue_callers))
        .route("/v1/queues/{id}/callback", post(request_queue_callback))
        .route("/v1/queues/{id}/callbacks", get(list_queue_callbacks))
        .route("/v1/vip-callers", get(list_vip_callers).post(create_vip_caller))
        .route("/v1/vip-callers/{id}", delete(delete_vip_caller))
        .route("/v1/wallboard", get(get_wallboard))
        .route("/v1/monitor", get(list_monitors).post(start_monitor))
        .route("/v1/monitor/{id}", delete(end_monitor))
        .route("/v1/qa/scorecards", get(list_scorecards).post(create_scorecard))
        .route("/v1/canned-responses", get(list_canned).post(create_canned))
        .route("/v1/canned-responses/{id}", delete(delete_canned))
        .route("/v1/call-settings/{sip_uri}", get(get_user_call_settings_admin))
        .route("/v1/ldap/config", get(get_ldap_config).put(set_ldap_config))
        .route("/v1/ldap/test", post(test_ldap_connection))
        .route("/v1/ring-groups", get(list_ring_groups).post(create_ring_group))
        .route("/v1/ring-groups/{id}", get(get_ring_group).delete(delete_ring_group))
        .route("/v1/ivrs", get(list_ivrs).post(create_ivr))
        .route("/v1/ivrs/{id}", get(get_ivr).delete(delete_ivr))
        .route("/v1/routes/resolve/{uri}", get(resolve_route))
        .route("/v1/voicemail", get(list_voicemails))
        .route("/v1/voicemail/{id}/listen", put(mark_voicemail_listened))
        .route("/v1/voicemail/{id}", delete(delete_voicemail))
        .route("/v1/recordings", get(list_recordings))
        .route("/v1/recordings/{id}", delete(delete_recording))
        .route("/v1/events", get(sse_stream))
        .layer(from_fn(crate::metrics::request_metrics))
        .layer(from_fn(cors))
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .with_state(state)
}

async fn cors(request: Request<axum::body::Body>, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let allowed_origin = origin.as_deref().filter(|origin| origin_allowed(origin));

    if request.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_cors_headers(response.headers_mut(), allowed_origin);
        return response;
    }

    let mut response = next.run(request).await;
    apply_cors_headers(response.headers_mut(), allowed_origin);
    response
}

fn apply_cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    if let Some(origin) = origin.and_then(|origin| HeaderValue::from_str(origin).ok()) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type, X-Pale-Filename"),
    );
}

fn origin_allowed(origin: &str) -> bool {
    allowed_origins().iter().any(|allowed| allowed == origin)
}

fn allowed_origins() -> Vec<String> {
    if env_bool("PALE_ALLOW_CONFIGURABLE_CORS", false) {
        if let Ok(value) = std::env::var("PALE_ALLOWED_ORIGINS") {
            return value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
    }

    vec![
        "http://localhost:1420".to_string(),
        "http://127.0.0.1:1420".to_string(),
        "tauri://localhost".to_string(),
    ]
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

async fn root() -> axum::response::Html<&'static str> {
    axum::response::Html(r#"<!DOCTYPE html>
<html><head><title>Pale Server</title><style>
body{font-family:system-ui;background:#09090b;color:#e4e4e7;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
.c{text-align:center;max-width:400px}h1{font-size:2rem;margin-bottom:.5rem}
p{color:#a1a1aa;font-size:.875rem}a{color:#6366f1;text-decoration:none}
.status{display:inline-block;width:8px;height:8px;border-radius:50%;background:#22c55e;margin-right:6px}
code{background:#27272a;padding:2px 6px;border-radius:4px;font-size:.8rem}
</style></head><body><div class="c">
<h1>Pale Server</h1>
<p><span class="status"></span>Running</p>
<p style="margin-top:1.5rem">API: <code>GET <a href="/health">/health</a></code></p>
<p>Docs: <code>GET <a href="/metrics">/metrics</a></code></p>
<p style="margin-top:1.5rem;color:#71717a">Connect the Pale desktop app via<br>Settings &rarr; Server &rarr; <code>http://localhost:8090</code></p>
</div></body></html>"#)
}

async fn health(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let pg_healthy = state.pg_healthy();
    Json(json!({
        "ok": pg_healthy,
        "service": "pale-server",
        "status": if pg_healthy { "healthy" } else { "degraded" },
    }))
}

async fn admin_login(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<AdminLoginRequest>,
) -> Result<Json<crate::AdminSession>, ApiError> {
    state
        .authenticate_admin(
            &input.username,
            input.password.expose(),
            &request_source(&headers),
        )
        .map(Json)
        .map_err(|err| match err {
            AuthError::Unauthorized => ApiError::Unauthorized,
            AuthError::Locked => ApiError::TooManyRequests,
        })
}

async fn admin_logout(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    state.revoke_session(token);
    Ok(Json(json!({ "ok": true })))
}

async fn admin_refresh(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::AdminSession>, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let principal = state
        .principal_for_bearer(token)
        .ok_or(ApiError::Unauthorized)?;
    let session = state
        .refresh_admin_session(token)
        .map_err(|_| ApiError::Unauthorized)?;
    state.record_audit_event(&principal, "admin.token.refreshed", None);
    Ok(Json(session))
}

// ─── Password Change ───

#[derive(serde::Deserialize)]
struct ChangePasswordRequest {
    old_password: SensitiveString,
    new_password: SensitiveString,
}

async fn change_password(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state
        .change_user_password(&principal, input.old_password.expose(), input.new_password.expose())
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "user.password_changed", None);
    Ok(Json(json!({ "ok": true })))
}

// ─── User Authentication (unified login) ───

#[derive(serde::Deserialize)]
struct UserLoginRequest {
    sip_uri: String,
    password: SensitiveString,
}

async fn user_login(
    State(state): State<SharedState>,
    Json(input): Json<UserLoginRequest>,
) -> Result<Json<crate::UserLoginResponse>, ApiError> {
    state
        .authenticate_user(&input.sip_uri, input.password.expose())
        .map(Json)
        .map_err(|err| match err {
            AuthError::Unauthorized => ApiError::Unauthorized,
            AuthError::Locked => ApiError::TooManyRequests,
        })
}

async fn list_audit_events(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::AdminAuditEvent>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.audit_events()))
}

async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateUserRequest>,
) -> Result<Json<crate::User>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let user = state.create_user(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "user.created", Some(user.id.to_string()));
    Ok(Json(user))
}

async fn list_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::User>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.users()))
}

async fn delete_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::User>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let user = state.delete_user(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "user.deleted", Some(id.to_string()));
    Ok(Json(user))
}

#[derive(serde::Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

async fn update_user_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRoleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state.update_user_role(id, &input.role).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "user.role_updated", Some(format!("{}:{}", id, input.role)));
    Ok(Json(json!({ "ok": true })))
}

async fn create_sip_account(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<AdminCreateSipAccountRequest>,
) -> Result<Json<crate::SipAccount>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let account = state.upsert_sip_account(CreateSipAccountRequest {
        password_ha1: sip_ha1(&input.username, &input.domain, input.password.expose()),
        username: input.username,
        domain: input.domain,
        display_name: input.display_name,
    });
    state.record_audit_event(&principal, "sip_account.upserted", Some(account.aor()));
    Ok(Json(account))
}

async fn list_sip_accounts(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipAccount>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.sip_accounts()))
}

async fn update_sip_account_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((username, domain)): Path<(String, String)>,
    Json(input): Json<UpdateSipAccountStatusRequest>,
) -> Result<Json<crate::SipAccount>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let account = state
        .update_sip_account_enabled(&username, &domain, input.enabled)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "sip_account.status_updated", Some(account.aor()));
    Ok(Json(account))
}

async fn delete_sip_account(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((username, domain)): Path<(String, String)>,
) -> Result<Json<crate::SipAccount>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let account = state
        .delete_sip_account(&username, &domain)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "sip_account.deleted", Some(account.aor()));
    Ok(Json(account))
}

async fn list_sip_registrations(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipRegistration>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.registrations()))
}

async fn list_sip_dialogs(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipDialog>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.sip_dialogs()))
}

#[derive(serde::Deserialize)]
struct MessageQuery {
    limit: Option<usize>,
    before: Option<String>,
    room_id: Option<String>,
}

async fn list_sip_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<MessageQuery>,
) -> Result<Json<Vec<crate::SipMessage>>, ApiError> {
    require_bearer(&headers, &state)?;
    let mut messages = state.sip_messages();

    if let Some(room_id) = &query.room_id {
        messages.retain(|m| m.from_uri == *room_id || m.to_uri == *room_id);
    }

    if let Some(before) = &query.before {
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(before) {
            let ts = ts.with_timezone(&Utc);
            messages.retain(|m| m.received_at < ts);
        }
    }

    messages.sort_by(|a, b| b.received_at.cmp(&a.received_at));

    let limit = query.limit.unwrap_or(100).min(500);
    messages.truncate(limit);

    Ok(Json(messages))
}

async fn list_sip_transactions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipTransaction>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.sip_transactions()))
}

async fn create_conference(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateConferenceRequest>,
) -> Result<Json<crate::Conference>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let conference = state.create_conference(input);
    state.record_audit_event(
        &principal,
        "conference.created",
        Some(conference.id.to_string()),
    );
    Ok(Json(conference))
}

async fn list_conferences(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Conference>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_conferences()))
}

async fn media_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::MediaConfig>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.media_config()))
}

async fn join_conference(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<JoinConferenceRequest>,
) -> Result<Json<crate::Conference>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let conference = state
        .join_conference(id, input)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "conference.participant_joined", Some(id.to_string()));
    Ok(Json(conference))
}

async fn leave_conference(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<crate::Conference>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let conference = state
        .leave_conference(id, user_id)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "conference.participant_left",
        Some(format!("{id}:{user_id}")),
    );
    Ok(Json(conference))
}

async fn create_call(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateCallRequest>,
) -> Result<Json<crate::CallSession>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let call = state.create_call(input);
    state.record_audit_event(&principal, "call.created", Some(call.id.to_string()));
    Ok(Json(call))
}

async fn list_calls(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CallSession>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.calls()))
}

async fn update_call_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCallStatusRequest>,
) -> Result<Json<crate::CallSession>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let status: CallStatus = input.status;
    let call = state
        .update_call_status(id, status)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "call.status_updated", Some(id.to_string()));
    Ok(Json(call))
}

async fn create_routing_rule(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateRoutingRuleRequest>,
) -> Result<Json<crate::RoutingRule>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let rule = state.create_routing_rule(input);
    state.record_audit_event(&principal, "routing_rule.created", Some(rule.id.to_string()));
    Ok(Json(rule))
}

async fn list_routing_rules(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::RoutingRule>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.routing_rules()))
}

async fn delete_routing_rule(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::RoutingRule>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let rule = state
        .delete_routing_rule(id)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "routing_rule.deleted", Some(id.to_string()));
    Ok(Json(rule))
}

async fn update_routing_rule(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateRoutingRuleRequest>,
) -> Result<Json<crate::RoutingRule>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let rule = state
        .update_routing_rule(id, input)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "routing_rule.updated", Some(id.to_string()));
    Ok(Json(rule))
}

async fn upload_file(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<FileRecord>, ApiError> {
    let owner = authenticated_principal(&headers, &state)?;
    if body.len() as u64 > state.max_upload_bytes() {
        return Err(ApiError::PayloadTooLarge);
    }
    let filename = safe_filename(
        &header_string(&headers, "x-pale-filename").unwrap_or_else(|| "file".to_string()),
    );
    let content_type = header_string(&headers, header::CONTENT_TYPE.as_str())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    tokio::fs::create_dir_all(state.files_dir()).await?;
    let id = Uuid::new_v4();
    let path = state.file_path(id);
    tokio::fs::write(&path, &body).await?;

    let mut hasher = Sha256::new();
    hasher.update(&body);
    let sha256 = to_hex(&hasher.finalize());

    let record = FileRecord {
        id,
        owner: owner.clone(),
        filename,
        content_type,
        size: body.len() as u64,
        sha256,
        created_at: Utc::now(),
    };
    state.put_file_record(record.clone());
    state.record_audit_event(&owner, "file.uploaded", Some(record.id.to_string()));

    Ok(Json(record))
}

async fn list_files(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<FileRecord>>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let records = state
        .file_records()
        .into_iter()
        .filter(|record| state.is_admin_principal(&requester) || record.owner == requester)
        .collect();
    Ok(Json(records))
}

async fn download_file(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let record = state.file_record(id).ok_or(ApiError::NotFound)?;
    if !state.is_admin_principal(&requester) && requester != record.owner {
        return Err(ApiError::Forbidden);
    }
    let bytes = tokio::fs::read(state.file_path(id)).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&record.content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    let disposition = format!("attachment; filename=\"{}\"", record.filename.replace('"', ""));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .unwrap_or(HeaderValue::from_static("attachment; filename=\"file\"")),
    );

    Ok((headers, bytes).into_response())
}

async fn delete_file(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<FileRecord>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let record = state.file_record(id).ok_or(ApiError::NotFound)?;
    if !state.is_admin_principal(&requester) && requester != record.owner {
        return Err(ApiError::Forbidden);
    }
    let record = state.delete_file_record(id).ok_or(ApiError::NotFound)?;
    match tokio::fs::remove_file(state.file_path(id)).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    state.record_audit_event(&requester, "file.deleted", Some(id.to_string()));
    Ok(Json(record))
}

// ─── Subscriptions, Notifications, Presence ───

async fn list_sip_subscriptions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipSubscription>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.sip_subscriptions()))
}

async fn list_sip_notifications(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipNotification>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.sip_notifications()))
}

async fn list_presence(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::UserPresence>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.all_presence()))
}

async fn get_presence(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(sip_uri): Path<String>,
) -> Result<Json<crate::UserPresence>, ApiError> {
    require_bearer(&headers, &state)?;
    let uri = if sip_uri.starts_with("sip:") {
        sip_uri
    } else {
        format!("sip:{}", sip_uri)
    };
    state.presence(&uri).map(Json).ok_or(ApiError::NotFound)
}

async fn set_presence(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<SetPresenceRequest>,
) -> Result<Json<crate::UserPresence>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let presence = state.update_presence(&principal, input.status, input.note);
    Ok(Json(presence))
}

// ─── Call History ───

async fn get_call_history(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CallHistoryEntry>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.call_history_for_user(&principal)))
}

async fn sync_call_history(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<SyncCallHistoryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let merged = state.merge_call_history(&principal, input.entries);
    Ok(Json(json!({ "merged": merged })))
}

// ─── Group Chat Rooms ───

async fn list_rooms(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Room>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.list_rooms_for_user(&principal)))
}

async fn create_room(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateRoomRequest>,
) -> Result<Json<crate::Room>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let room = state.create_room(&principal, input);
    state.record_audit_event(&principal, "room.created", Some(room.id.to_string()));
    Ok(Json(room))
}

async fn get_room(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::Room>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    state.room(id).map(Json).ok_or(ApiError::NotFound)
}

async fn list_room_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::RoomMessage>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    Ok(Json(state.room_messages(id)))
}

async fn send_room_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SendRoomMessageRequest>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    Ok(Json(state.send_room_message(id, &principal, &input.body, input.reply_to)))
}

async fn add_room_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<AddRoomMemberRequest>,
) -> Result<Json<crate::Room>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    state
        .add_room_member(id, &input.user_sip_uri)
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn leave_room(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::Room>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state
        .remove_room_member(id, &principal)
        .map(Json)
        .ok_or(ApiError::NotFound)
}

// ─── Typing Indicators ───

#[derive(serde::Deserialize)]
struct TypingRequest {
    typing: bool,
}

async fn room_typing(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<TypingRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "typing".to_string(),
        payload: json!({
            "room_id": id,
            "user": principal,
            "typing": input.typing,
        }),
    });
    Ok(Json(json!({ "ok": true })))
}

// ─── Search ───

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<usize>,
}

async fn search_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<crate::SearchResult>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let term = query.q.to_lowercase();
    let limit = query.limit.unwrap_or(50).min(200);

    // Search SIP messages (in-memory for now; Postgres GIN index used when PG is primary)
    let mut results: Vec<crate::SearchResult> = state
        .sip_messages()
        .into_iter()
        .filter(|m| m.body.to_lowercase().contains(&term))
        .take(limit)
        .map(|m| crate::SearchResult {
            id: m.id,
            source: "sip".to_string(),
            from_uri: m.from_uri,
            body: m.body,
            timestamp: m.received_at,
            room_id: None,
        })
        .collect();

    // Search room messages
    let room_results: Vec<crate::SearchResult> = state
        .room_messages
        .read()
        .expect("room messages lock poisoned")
        .iter()
        .filter(|m| room_member(&state, m.room_id, &principal))
        .filter(|m| m.body.to_lowercase().contains(&term))
        .take(limit)
        .map(|m| crate::SearchResult {
            id: m.id,
            source: "room".to_string(),
            from_uri: m.sender_uri.clone(),
            body: m.body.clone(),
            timestamp: m.created_at,
            room_id: Some(m.room_id),
        })
        .collect();

    results.extend(room_results);
    results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    results.truncate(limit);

    Ok(Json(results))
}

// ─── Read Receipts ───

async fn mark_message_read(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let msg = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, msg.room_id, &principal)?;
    // For now, broadcast the read event via SSE (full persistence via PG)
    state.broadcast_sse(crate::SseEvent {
        event_type: "read_receipt".to_string(),
        payload: json!({
            "message_id": id,
            "reader": principal,
            "read_at": Utc::now(),
        }),
    });
    Ok(Json(json!({ "ok": true })))
}

// ─── Message Edit & Delete ───

#[derive(serde::Deserialize)]
struct EditMessageRequest {
    body: String,
}

async fn edit_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<EditMessageRequest>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let existing = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, existing.room_id, &principal)?;
    let msg = state.edit_room_message(id, &input.body).ok_or(ApiError::NotFound)?;
    // Persist to PG
    let body_clone = input.body.clone();
    state.pg_spawn(move |pg| Box::pin(async move { pg.update_room_message_body(id, &body_clone).await }));
    Ok(Json(msg))
}

async fn delete_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let msg = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, msg.room_id, &principal)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "message_deleted".to_string(),
        payload: json!({
            "message_id": id,
            "deleted_by": principal,
            "deleted_at": Utc::now(),
        }),
    });
    state.record_audit_event(&principal, "message.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Reactions ───

#[derive(serde::Deserialize)]
struct ReactionRequest {
    emoji: String,
}

async fn react_to_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ReactionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let msg = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, msg.room_id, &principal)?;
    state.add_reaction(id, &principal, &input.emoji);
    // Persist to PG (toggle: try insert, if exists delete)
    let emoji = input.emoji.clone();
    let user = principal.clone();
    state.pg_spawn(move |pg| Box::pin(async move {
        // Try insert; if conflict, delete instead
        if pg.insert_reaction(id, &user, &emoji).await.is_err() {
            pg.delete_reaction(id, &user, &emoji).await?;
        }
        Ok(())
    }));
    Ok(Json(json!({ "ok": true })))
}

// ─── Pin, Favorites, Profile ───

async fn pin_message_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let existing = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, existing.room_id, &principal)?;
    let msg = state.pin_room_message(id).ok_or(ApiError::NotFound)?;
    let pinned = msg.pinned;
    state.pg_spawn(move |pg| Box::pin(async move { pg.toggle_pin(id, pinned).await }));
    Ok(Json(msg))
}

async fn list_pinned_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::RoomMessage>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    Ok(Json(state.pinned_messages(id)))
}

async fn list_reads_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let msg = state.room_message(_id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, msg.room_id, &principal)?;
    // Read receipts are currently broadcast-only via SSE; return empty for now
    Ok(Json(vec![]))
}

async fn list_favorites_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.list_favorites(&principal)))
}

#[derive(serde::Deserialize)]
struct AddFavoriteRequest {
    favorite_uri: String,
}

async fn add_favorite_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<AddFavoriteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state.add_favorite(&principal, &input.favorite_uri);
    let user = principal.clone();
    let fav = input.favorite_uri.clone();
    state.pg_spawn(move |pg| Box::pin(async move { pg.insert_favorite(&user, &fav).await }));
    Ok(Json(json!({ "ok": true })))
}

async fn remove_favorite_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    // URI path param may be percent-encoded; decode %xx sequences
    let decoded = percent_decode(&uri);
    state.remove_favorite(&principal, &decoded);
    let user = principal.clone();
    state.pg_spawn(move |pg| Box::pin(async move { pg.delete_favorite(&user, &decoded).await }));
    Ok(Json(json!({ "ok": true })))
}

#[derive(serde::Deserialize)]
struct UpdateProfileRequest {
    email: Option<String>,
    title: Option<String>,
    department: Option<String>,
    phone_number: Option<String>,
}

async fn update_profile_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateProfileRequest>,
) -> Result<Json<crate::User>, ApiError> {
    let _principal = authenticated_principal(&headers, &state)?;
    let user = state
        .update_user_profile(id, input.email.clone(), input.title.clone(), input.department.clone(), input.phone_number.clone())
        .ok_or(ApiError::NotFound)?;
    let email = input.email;
    let title = input.title;
    let dept = input.department;
    let phone = input.phone_number;
    state.pg_spawn(move |pg| Box::pin(async move { pg.update_user_profile(id, email, title, dept, phone).await }));
    Ok(Json(user))
}

// ─── Avatar Upload ───

async fn upload_avatar(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    if body.len() as u64 > state.max_upload_bytes() {
        return Err(ApiError::PayloadTooLarge);
    }

    let content_type = header_string(&headers, header::CONTENT_TYPE.as_str())
        .unwrap_or_else(|| "image/png".to_string());

    // Store as a file
    tokio::fs::create_dir_all(state.files_dir()).await?;
    let file_id = Uuid::new_v4();
    let path = state.file_path(file_id);
    tokio::fs::write(&path, &body).await?;

    let mut hasher = Sha256::new();
    hasher.update(&body);
    let sha256 = to_hex(&hasher.finalize());

    let record = FileRecord {
        id: file_id,
        owner: principal.clone(),
        filename: format!("avatar-{}.png", user_id),
        content_type,
        size: body.len() as u64,
        sha256,
        created_at: Utc::now(),
    };
    state.put_file_record(record);
    state.record_audit_event(&principal, "user.avatar_updated", Some(user_id.to_string()));

    Ok(Json(json!({
        "file_id": file_id,
        "url": format!("/v1/files/{}", file_id),
    })))
}

// ─── Call Center ───

async fn list_agents(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::AgentProfile>>, ApiError> {
    require_bearer(&headers, &state)?; Ok(Json(state.list_agent_profiles()))
}
async fn create_agent(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateAgentProfileRequest>) -> Result<Json<crate::AgentProfile>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    let a = state.create_agent_profile(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&p, "agent.created", Some(a.user_sip_uri.clone()));
    Ok(Json(a))
}
async fn get_agent(State(state): State<SharedState>, headers: HeaderMap, Path(uri): Path<String>) -> Result<Json<crate::AgentProfile>, ApiError> {
    require_bearer(&headers, &state)?;
    let full = if uri.starts_with("sip:") { uri } else { format!("sip:{}", uri) };
    state.agent_profile(&full).map(Json).ok_or(ApiError::NotFound)
}
async fn delete_agent(State(state): State<SharedState>, headers: HeaderMap, Path(uri): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    let full = if uri.starts_with("sip:") { uri } else { format!("sip:{}", uri) };
    state.delete_agent_profile(&full).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "agent.deleted", Some(full)); Ok(Json(json!({"ok":true})))
}
async fn set_agent_state(State(state): State<SharedState>, headers: HeaderMap, Path(uri): Path<String>, Json(input): Json<SetAgentStateRequest>) -> Result<Json<crate::AgentProfile>, ApiError> {
    let _p = authenticated_principal(&headers, &state)?;
    let full = if uri.starts_with("sip:") { uri } else { format!("sip:{}", uri) };
    let agent = state.transition_agent_state(&full, &input.state, input.reason)
        .map_err(|e| ApiError::Conflict(e))?;
    Ok(Json(agent))
}

async fn transition_agent_state_handler(State(state): State<SharedState>, headers: HeaderMap, Path(uri): Path<String>, Json(input): Json<AgentTransitionRequest>) -> Result<Json<crate::AgentProfile>, ApiError> {
    let _p = authenticated_principal(&headers, &state)?;
    let full = if uri.starts_with("sip:") { uri } else { format!("sip:{}", uri) };
    let agent = state.transition_agent_state(&full, &input.state, input.reason)
        .map_err(|e| ApiError::Conflict(e))?;
    // Start wrap-up timer if transitioning to wrap_up
    if input.state == "wrap_up" {
        // Find wrap_up_time from agent's queues
        let wrap_secs = agent.queues.iter()
            .filter_map(|qid| state.queue(*qid))
            .map(|q| q.wrap_up_time)
            .max()
            .unwrap_or(10);
        crate::start_wrap_up_timer(state.clone(), full, wrap_secs);
    }
    Ok(Json(agent))
}

async fn agent_state_history(State(state): State<SharedState>, headers: HeaderMap, Path(uri): Path<String>) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_bearer(&headers, &state)?;
    let full = if uri.starts_with("sip:") { uri } else { format!("sip:{}", uri) };
    let pg = state.pg_store().ok_or(ApiError::NotFound)?;
    let history = pg.list_agent_state_log(&full, 100).await.map_err(|_| ApiError::NotFound)?;
    Ok(Json(history))
}

async fn list_queue_callers(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<Vec<crate::QueueCallerEntry>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.queue_callers_waiting(id)))
}

async fn request_queue_callback(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>, Json(input): Json<RequestCallbackInput>) -> Result<Json<crate::QueueCallback>, ApiError> {
    require_bearer(&headers, &state)?;
    state.queue(id).ok_or(ApiError::NotFound)?;
    Ok(Json(state.request_callback(id, input)))
}

async fn list_queue_callbacks(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<Vec<crate::QueueCallback>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_queue_callbacks(id)))
}

async fn list_vip_callers(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::VipCaller>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_vip_callers()))
}

async fn create_vip_caller(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateVipCallerRequest>) -> Result<Json<crate::VipCaller>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let vip = state.create_vip_caller(input);
    state.record_audit_event(&p, "vip_caller.created", Some(vip.caller_pattern.clone()));
    Ok(Json(vip))
}

async fn delete_vip_caller(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_vip_caller(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "vip_caller.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok": true})))
}

async fn get_wallboard(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<serde_json::Value>, ApiError> {
    authenticated_admin(&headers, &state)?;
    let metrics = state.queue_wallboard();
    let agents = state.list_agent_profiles();
    let available = agents.iter().filter(|a| a.state == "available").count();
    let on_call = agents.iter().filter(|a| a.state == "on_call").count();
    let wrap_up = agents.iter().filter(|a| a.state == "wrap_up").count();
    let on_break = agents.iter().filter(|a| a.state == "break" || a.state == "training" || a.state == "meeting").count();
    let offline = agents.iter().filter(|a| a.state == "offline").count();

    Ok(Json(json!({
        "queues": metrics,
        "agents": {
            "total": agents.len(),
            "available": available,
            "on_call": on_call,
            "wrap_up": wrap_up,
            "on_break": on_break,
            "offline": offline,
        },
        "agent_list": agents,
    })))
}

async fn list_monitors(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::MonitorSession>>, ApiError> {
    authenticated_admin(&headers, &state)?; Ok(Json(state.list_monitor_sessions()))
}
async fn start_monitor(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<StartMonitorRequest>) -> Result<Json<crate::MonitorSession>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let session = state.start_monitor(&p, input);
    state.record_audit_event(&p, "monitor.started", Some(format!("{}:{}", session.mode, session.target_call_id)));
    Ok(Json(session))
}
async fn end_monitor(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.end_monitor(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "monitor.ended", Some(id.to_string())); Ok(Json(json!({"ok":true})))
}

async fn list_scorecards(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::QaScorecard>>, ApiError> {
    authenticated_admin(&headers, &state)?; Ok(Json(state.list_scorecards()))
}
async fn create_scorecard(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateScorecardRequest>) -> Result<Json<crate::QaScorecard>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let sc = state.create_scorecard(&p, input);
    state.record_audit_event(&p, "scorecard.created", Some(sc.id.to_string()));
    Ok(Json(sc))
}

async fn list_canned(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::CannedResponse>>, ApiError> {
    require_bearer(&headers, &state)?; Ok(Json(state.list_canned_responses()))
}
async fn create_canned(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateCannedResponseRequest>) -> Result<Json<crate::CannedResponse>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let cr = state.create_canned_response(input);
    state.record_audit_event(&p, "canned_response.created", Some(cr.id.to_string()));
    Ok(Json(cr))
}
async fn delete_canned(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_canned_response(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "canned_response.deleted", Some(id.to_string())); Ok(Json(json!({"ok":true})))
}

// ─── PBX Features ───

async fn list_queues(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::CallQueue>>, ApiError> {
    require_bearer(&headers, &state)?; Ok(Json(state.list_queues()))
}
async fn create_queue(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateQueueRequest>) -> Result<Json<crate::CallQueue>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let q = state.create_queue(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&p, "queue.created", Some(q.id.to_string()));
    Ok(Json(q))
}
async fn get_queue(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<crate::CallQueue>, ApiError> {
    require_bearer(&headers, &state)?; state.queue(id).map(Json).ok_or(ApiError::NotFound)
}
async fn delete_queue(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_queue(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "queue.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
}

#[derive(serde::Deserialize)]
struct ListExtensionsQuery {
    unassigned: Option<bool>,
}

async fn list_extensions(State(state): State<SharedState>, headers: HeaderMap, Query(q): Query<ListExtensionsQuery>) -> Result<Json<Vec<crate::Extension>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_extensions_filtered(q.unassigned.unwrap_or(false))))
}
async fn create_extension(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateExtensionRequest>) -> Result<Json<crate::Extension>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let e = state.create_extension(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&p, "extension.created", Some(e.extension.clone()));
    Ok(Json(e))
}
async fn delete_extension(State(state): State<SharedState>, headers: HeaderMap, Path(ext): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_extension(&ext).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "extension.deleted", Some(ext)); Ok(Json(json!({"ok":true})))
}

async fn provision_user_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<ProvisionUserRequest>,
) -> Result<Json<crate::ProvisionUserResponse>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let response = state.provision_user(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "user.provisioned", Some(response.user.id.to_string()));
    Ok(Json(response))
}

async fn assign_extension_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(ext): Path<String>,
    Json(input): Json<AssignExtensionRequest>,
) -> Result<Json<crate::Extension>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let extension = state.assign_extension(&ext, input.user_id).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "extension.assigned", Some(format!("{}:{}", ext, input.user_id)));
    Ok(Json(extension))
}

async fn unassign_extension_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(ext): Path<String>,
) -> Result<Json<crate::Extension>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let extension = state.unassign_extension(&ext).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "extension.unassigned", Some(ext));
    Ok(Json(extension))
}

async fn list_business_hours(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::BusinessHours>>, ApiError> {
    authenticated_admin(&headers, &state)?; Ok(Json(state.list_business_hours()))
}
async fn create_business_hours(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateBusinessHoursRequest>) -> Result<Json<crate::BusinessHours>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let bh = state.create_business_hours(input);
    state.record_audit_event(&p, "business_hours.created", Some(bh.id.to_string()));
    Ok(Json(bh))
}
async fn delete_business_hours_entry(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_business_hours(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "business_hours.deleted", Some(id.to_string())); Ok(Json(json!({"ok":true})))
}

async fn list_holidays(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::Holiday>>, ApiError> {
    authenticated_admin(&headers, &state)?; Ok(Json(state.list_holidays()))
}
async fn create_holiday(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateHolidayRequest>) -> Result<Json<crate::Holiday>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let h = state.create_holiday(input);
    state.record_audit_event(&p, "holiday.created", Some(h.id.to_string()));
    Ok(Json(h))
}
async fn delete_holiday(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_holiday(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "holiday.deleted", Some(id.to_string())); Ok(Json(json!({"ok":true})))
}

#[derive(serde::Deserialize)]
struct ParkRequest { call_id: String, caller_uri: String, caller_name: Option<String>, slot: String }
async fn park_call(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<ParkRequest>) -> Result<Json<crate::ParkedCall>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    Ok(Json(state.park_call(&input.slot, &input.call_id, &p, &input.caller_uri, input.caller_name.as_deref().unwrap_or(""))))
}
async fn pickup_call(State(state): State<SharedState>, headers: HeaderMap, Path(slot): Path<String>) -> Result<Json<crate::ParkedCall>, ApiError> {
    require_bearer(&headers, &state)?; state.pickup_parked_call(&slot).map(Json).ok_or(ApiError::NotFound)
}
async fn list_parked(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::ParkedCall>>, ApiError> {
    require_bearer(&headers, &state)?; Ok(Json(state.list_parked_calls()))
}

async fn list_speed_dials(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::SpeedDial>>, ApiError> {
    let p = authenticated_principal(&headers, &state)?; Ok(Json(state.speed_dials_for_user(&p)))
}
async fn create_speed_dial(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreateSpeedDialRequest>) -> Result<Json<crate::SpeedDial>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    Ok(Json(state.set_speed_dial(Some(&p), input)))
}

#[derive(serde::Deserialize)]
struct CdrQuery { limit: Option<usize> }
async fn list_cdrs(State(state): State<SharedState>, headers: HeaderMap, Query(q): Query<CdrQuery>) -> Result<Json<Vec<crate::CallDetailRecord>>, ApiError> {
    authenticated_admin(&headers, &state)?; Ok(Json(state.list_cdrs(q.limit.unwrap_or(100))))
}

async fn list_paging_groups(State(state): State<SharedState>, headers: HeaderMap) -> Result<Json<Vec<crate::PagingGroup>>, ApiError> {
    require_bearer(&headers, &state)?; Ok(Json(state.list_paging_groups()))
}
async fn create_paging_group(State(state): State<SharedState>, headers: HeaderMap, Json(input): Json<CreatePagingGroupRequest>) -> Result<Json<crate::PagingGroup>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let pg = state.create_paging_group(input);
    state.record_audit_event(&p, "paging_group.created", Some(pg.id.to_string()));
    Ok(Json(pg))
}
async fn delete_paging_group(State(state): State<SharedState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_paging_group(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "paging_group.deleted", Some(id.to_string())); Ok(Json(json!({"ok":true})))
}

// ─── User Call Settings (Voicemail + Follow-Me) ───

async fn get_call_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::UserCallSettings>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.get_user_call_settings(&principal)))
}

async fn update_call_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(settings): Json<crate::UserCallSettings>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let mut s = settings;
    s.user_sip_uri = principal; // Enforce ownership
    state.set_user_call_settings(s);
    Ok(Json(json!({ "ok": true })))
}

async fn get_user_call_settings_admin(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(sip_uri): Path<String>,
) -> Result<Json<crate::UserCallSettings>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.is_admin_principal(&principal) {
        return Err(ApiError::Forbidden);
    }
    let uri = if sip_uri.starts_with("sip:") { sip_uri } else { format!("sip:{}", sip_uri) };
    Ok(Json(state.get_user_call_settings(&uri)))
}

// ─── LDAP / Active Directory ───

async fn get_ldap_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::ldap_auth::LdapConfig>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.is_admin_principal(&principal) {
        return Err(ApiError::Forbidden);
    }
    let mut config = state.ldap_config();
    config.bind_password = "***".to_string(); // Never expose password
    Ok(Json(config))
}

async fn set_ldap_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(config): Json<crate::ldap_auth::LdapConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.is_admin_principal(&principal) {
        return Err(ApiError::Forbidden);
    }
    state.set_ldap_config(config);
    state.record_audit_event(&principal, "ldap.config_updated", None);
    Ok(Json(json!({ "ok": true })))
}

async fn test_ldap_connection(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.is_admin_principal(&principal) {
        return Err(ApiError::Forbidden);
    }
    let config = state.ldap_config();
    match crate::ldap_auth::ldap_authenticate(&config, "test", "test").await {
        Ok(_) => Ok(Json(json!({ "ok": true, "message": "Connection successful" }))),
        Err(e) => {
            if e.contains("connection failed") || e.contains("bind failed") {
                Ok(Json(json!({ "ok": false, "message": e })))
            } else {
                // Connection works, auth failed (expected for test user)
                Ok(Json(json!({ "ok": true, "message": "Connection successful (test auth rejected as expected)" })))
            }
        }
    }
}

// ─── Ring Groups ───

async fn list_ring_groups(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::RingGroup>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_ring_groups()))
}

async fn create_ring_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateRingGroupRequest>,
) -> Result<Json<crate::RingGroup>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let group = state.create_ring_group(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "ring_group.created", Some(group.id.to_string()));
    Ok(Json(group))
}

async fn get_ring_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::RingGroup>, ApiError> {
    authenticated_admin(&headers, &state)?;
    state.ring_group(id).map(Json).ok_or(ApiError::NotFound)
}

async fn delete_ring_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state.delete_ring_group(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "ring_group.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── IVR ───

async fn list_ivrs(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Ivr>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_ivrs()))
}

async fn create_ivr(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateIvrRequest>,
) -> Result<Json<crate::Ivr>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let ivr = state.create_ivr(input).map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "ivr.created", Some(ivr.id.to_string()));
    Ok(Json(ivr))
}

async fn get_ivr(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::Ivr>, ApiError> {
    authenticated_admin(&headers, &state)?;
    state.ivr(id).map(Json).ok_or(ApiError::NotFound)
}

async fn delete_ivr(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state.delete_ivr(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "ivr.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Route Resolution ───

async fn resolve_route(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
) -> Result<Json<crate::ResolvedRoute>, ApiError> {
    authenticated_admin(&headers, &state)?;
    let full_uri = if uri.starts_with("sip:") { uri } else { format!("sip:{}", uri) };
    Ok(Json(state.resolve_inbound_route(&full_uri)))
}

// ─── Voicemail ───

async fn list_voicemails(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Voicemail>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.voicemails_for_user(&principal)))
}

async fn mark_voicemail_listened(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::Voicemail>, ApiError> {
    require_bearer(&headers, &state)?;
    state.mark_voicemail_listened(id).map(Json).ok_or(ApiError::NotFound)
}

async fn delete_voicemail(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state.delete_voicemail(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "voicemail.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Call Recordings ───

async fn list_recordings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CallRecording>>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    Ok(Json(state.recordings_for_user(&principal)))
}

async fn delete_recording(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state.delete_recording(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "recording.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Server-Sent Events ───

#[derive(serde::Deserialize)]
struct SseQuery {
    token: Option<String>,
}

async fn sse_stream(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<SseQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<SseResponseEvent, Infallible>>>, ApiError>
{
    // EventSource can't set Authorization headers, so accept token from query param too
    let principal = if let Some(token) = &query.token {
        let principal = state
            .principal_for_bearer(token)
            .ok_or(ApiError::Unauthorized)?;
        if !state.check_rate_limit(&principal) {
            return Err(ApiError::TooManyRequests);
        }
        principal
    } else {
        authenticated_principal(&headers, &state)?
    };
    let rx = state.sse_subscribe();
    let visible_state = state.clone();
    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(event) => {
            if !event_visible_to(&visible_state, &event, &principal) {
                return None;
            }
            let data = serde_json::to_string(&event.payload).unwrap_or_default();
            Some(Ok(SseResponseEvent::default()
                .event(event.event_type)
                .data(data)))
        }
        Err(_) => None,
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn require_bearer(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    authenticated_principal(headers, state).map(|_| ())
}

fn room_member(state: &AppState, room_id: Uuid, principal: &str) -> bool {
    state.room(room_id).is_some_and(|room| {
        room.members
            .iter()
            .any(|member| member.user_sip_uri == principal)
    })
}

fn require_room_member(
    state: &AppState,
    room_id: Uuid,
    principal: &str,
) -> Result<(), ApiError> {
    let room = state.room(room_id).ok_or(ApiError::NotFound)?;
    if room
        .members
        .iter()
        .any(|member| member.user_sip_uri == principal)
    {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn event_visible_to(state: &AppState, event: &crate::SseEvent, principal: &str) -> bool {
    match event.event_type.as_str() {
        "room_created" => event
            .payload
            .get("members")
            .and_then(|members| members.as_array())
            .is_some_and(|members| {
                members.iter().any(|member| {
                    member
                        .get("user_sip_uri")
                        .and_then(|uri| uri.as_str())
                        == Some(principal)
                })
            }),
        "room_message" | "typing" => event
            .payload
            .get("room_id")
            .and_then(|id| id.as_str())
            .and_then(|id| Uuid::parse_str(id).ok())
            .is_some_and(|room_id| room_member(state, room_id, principal)),
        "message_edited" | "message_pinned" => event
            .payload
            .get("room_id")
            .and_then(|id| id.as_str())
            .and_then(|id| Uuid::parse_str(id).ok())
            .is_some_and(|room_id| room_member(state, room_id, principal)),
        "message_deleted" | "read_receipt" | "reaction" => event
            .payload
            .get("message_id")
            .and_then(|id| id.as_str())
            .and_then(|id| Uuid::parse_str(id).ok())
            .and_then(|message_id| state.room_message(message_id))
            .is_some_and(|message| room_member(state, message.room_id, principal)),
        _ => true,
    }
}

fn bearer_token(headers: &HeaderMap) -> &str {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("")
}

fn authenticated_principal(headers: &HeaderMap, state: &AppState) -> Result<String, ApiError> {
    authenticated_principal_role(headers, state).map(|(principal, _)| principal)
}

/// Authenticate the request and return `(principal, role)`.
fn authenticated_principal_role(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<(String, String), ApiError> {
    let (principal, role) = state
        .principal_role_for_bearer(bearer_token(headers))
        .ok_or(ApiError::Unauthorized)?;

    // Rate limit per principal
    if !state.check_rate_limit(&principal) {
        return Err(ApiError::TooManyRequests);
    }

    Ok((principal, role))
}

/// Authenticate the request and require the admin role. Admin-only
/// management endpoints must use this instead of `authenticated_principal`:
/// a valid user-role session token is authenticated but NOT authorized for
/// administration and gets 403.
fn authenticated_admin(headers: &HeaderMap, state: &AppState) -> Result<String, ApiError> {
    let (principal, role) = authenticated_principal_role(headers, state)?;
    if role != crate::ROLE_ADMIN {
        return Err(ApiError::Forbidden);
    }
    Ok(principal)
}

fn request_source(headers: &HeaderMap) -> String {
    header_string(headers, "x-forwarded-for")
        .and_then(|value| value.split(',').next().map(str::trim).map(ToOwned::to_owned))
        .filter(|value| !value.is_empty())
        .or_else(|| header_string(headers, "x-real-ip"))
        .unwrap_or_else(|| "direct".to_string())
}

/// Simple percent-decode for URI path parameters.
fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &input[i + 1..i + 3],
                16,
            ) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[derive(serde::Deserialize)]
struct AdminCreateSipAccountRequest {
    username: String,
    domain: String,
    password: SensitiveString,
    display_name: Option<String>,
}

#[derive(serde::Deserialize)]
struct AdminLoginRequest {
    username: String,
    password: SensitiveString,
}

struct SensitiveString(String);

impl SensitiveString {
    fn expose(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for SensitiveString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self)
    }
}

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("too many requests")]
    TooManyRequests,
    #[error("{0}")]
    Conflict(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;
    use std::path::PathBuf;

    fn bearer_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        headers
    }

    #[test]
    fn user_role_token_is_forbidden_on_admin_endpoints() {
        let state = crate::AppState::new(
            PathBuf::from("/tmp/pale-test-http-auth"),
            "012345678901234567890123".to_string(),
            crate::hash_password("admin-password-equally-long"),
        );
        state
            .create_user(crate::CreateUserRequest {
                display_name: "Bob".to_string(),
                sip_uri: "sip:bob@example.com".to_string(),
                matrix_user_id: None,
                password: Some("user-password".to_string()),
                role: None,
            })
            .expect("create user");
        let login = state
            .authenticate_user("sip:bob@example.com", "user-password")
            .expect("user login");

        let headers = bearer_headers(&login.token);
        // Authenticated as a user...
        assert!(authenticated_principal(&headers, &state).is_ok());
        // ...but NOT authorized for admin-only management endpoints.
        assert!(matches!(
            authenticated_admin(&headers, &state),
            Err(ApiError::Forbidden)
        ));

        // The static server token retains full admin access.
        let admin_headers = bearer_headers("012345678901234567890123");
        assert!(authenticated_admin(&admin_headers, &state).is_ok());
    }

    #[test]
    fn authenticated_users_can_discover_directory() {
        let state = crate::AppState::new(
            PathBuf::from("/tmp/pale-test-http-users"),
            "012345678901234567890125".to_string(),
            crate::hash_password("admin-password-equally-long"),
        );
        state
            .create_user(crate::CreateUserRequest {
                display_name: "Alice".to_string(),
                sip_uri: "sip:alice@example.com".to_string(),
                matrix_user_id: None,
                password: Some("alice-password".to_string()),
                role: None,
            })
            .expect("create alice");
        state
            .create_user(crate::CreateUserRequest {
                display_name: "Bob".to_string(),
                sip_uri: "sip:bob@example.com".to_string(),
                matrix_user_id: None,
                password: Some("bob-password".to_string()),
                role: None,
            })
            .expect("create bob");

        let login = state
            .authenticate_user("sip:alice@example.com", "alice-password")
            .expect("alice login");
        let headers = bearer_headers(&login.token);

        assert!(require_bearer(&headers, &state).is_ok());
        let users = state.users();
        assert_eq!(users.len(), 2);
        assert!(users.iter().any(|u| u.sip_uri == "sip:bob@example.com"));
    }

    #[test]
    fn authenticated_users_can_discover_call_groups() {
        let state = crate::AppState::new(
            PathBuf::from("/tmp/pale-test-http-groups"),
            "012345678901234567890127".to_string(),
            crate::hash_password("admin-password-equally-long"),
        );
        state
            .create_user(crate::CreateUserRequest {
                display_name: "Alice".to_string(),
                sip_uri: "sip:alice@example.com".to_string(),
                matrix_user_id: None,
                password: Some("alice-password".to_string()),
                role: None,
            })
            .expect("create alice");
        let login = state
            .authenticate_user("sip:alice@example.com", "alice-password")
            .expect("alice login");
        let headers = bearer_headers(&login.token);

        state
            .create_ring_group(crate::CreateRingGroupRequest {
                name: "Support Ring".to_string(),
                extension: "700".to_string(),
                strategy: None,
                ring_timeout: None,
                members: vec!["sip:alice@example.com".to_string()],
                fallback_uri: None,
            })
            .expect("create ring group");
        state
            .create_queue(crate::CreateQueueRequest {
                name: "Support Queue".to_string(),
                extension: "710".to_string(),
                strategy: None,
                max_wait_time: None,
                max_queue_size: None,
                wrap_up_time: None,
                hold_music_file_id: None,
                overflow_destination: None,
                agents: vec![crate::QueueAgentInput {
                    agent_uri: "sip:alice@example.com".to_string(),
                    priority: None,
                    skills: None,
                }],
                callback_enabled: None,
                callback_threshold_secs: None,
                sla_target_secs: None,
            })
            .expect("create queue");
        state.create_paging_group(crate::CreatePagingGroupRequest {
            name: "Operations Page".to_string(),
            extension: "720".to_string(),
            members: vec!["sip:alice@example.com".to_string()],
        });
        state.create_conference(crate::CreateConferenceRequest {
            title: "Daily Standup".to_string(),
            mode: crate::ConferenceMode::Audio,
        });

        assert!(require_bearer(&headers, &state).is_ok());
        assert!(state.list_ring_groups().iter().any(|group| group.extension == "700"));
        assert!(state.list_queues().iter().any(|queue| queue.extension == "710"));
        assert!(state.list_paging_groups().iter().any(|group| group.extension == "720"));
        assert!(state.list_conferences().iter().any(|conference| conference.title == "Daily Standup"));
    }

    #[test]
    fn room_membership_controls_visibility() {
        let state = crate::AppState::new(
            PathBuf::from("/tmp/pale-test-http-room-visibility"),
            "012345678901234567890126".to_string(),
            crate::hash_password("admin-password-equally-long"),
        );
        let room = state.create_room(
            "sip:alice@example.com",
            crate::CreateRoomRequest {
                name: "Bob".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(true),
            },
        );
        let message = state.send_room_message(room.id, "sip:alice@example.com", "hello", None);

        assert!(room_member(&state, room.id, "sip:alice@example.com"));
        assert!(room_member(&state, room.id, "sip:bob@example.com"));
        assert!(!room_member(&state, room.id, "sip:mallory@example.com"));
        assert!(require_room_member(&state, room.id, "sip:bob@example.com").is_ok());
        assert!(matches!(
            require_room_member(&state, room.id, "sip:mallory@example.com"),
            Err(ApiError::Forbidden)
        ));

        let room_event = crate::SseEvent {
            event_type: "room_message".to_string(),
            payload: serde_json::to_value(&message).unwrap(),
        };
        assert!(event_visible_to(&state, &room_event, "sip:bob@example.com"));
        assert!(!event_visible_to(&state, &room_event, "sip:mallory@example.com"));

        let reaction_event = crate::SseEvent {
            event_type: "reaction".to_string(),
            payload: serde_json::json!({ "message_id": message.id }),
        };
        assert!(event_visible_to(&state, &reaction_event, "sip:alice@example.com"));
        assert!(!event_visible_to(&state, &reaction_event, "sip:mallory@example.com"));
    }

    #[test]
    fn ldap_enabled_but_unverifiable_still_requires_local_password() {
        let state = crate::AppState::new(
            PathBuf::from("/tmp/pale-test-http-ldap"),
            "012345678901234567890124".to_string(),
            crate::hash_password("admin-password-equally-long"),
        );
        state.set_ldap_config(crate::ldap_auth::LdapConfig {
            enabled: true,
            server_url: "ldap://127.0.0.1:1".to_string(), // unreachable
            bind_dn: String::new(),
            bind_password: String::new(),
            base_dn: String::new(),
            user_search_filter: "(sAMAccountName={username})".to_string(),
            user_dn_attribute: "sAMAccountName".to_string(),
            display_name_attribute: "displayName".to_string(),
            email_attribute: "mail".to_string(),
            group_attribute: "memberOf".to_string(),
            admin_group: String::new(),
            sip_domain: "example.com".to_string(),
        });
        state
            .create_user(crate::CreateUserRequest {
                display_name: "Carol".to_string(),
                sip_uri: "sip:carol@example.com".to_string(),
                matrix_user_id: None,
                password: Some("carol-password".to_string()),
                role: None,
            })
            .expect("create user");

        // LDAP cannot verify (unreachable) — the wrong local password MUST
        // still be rejected (fail closed, no auth bypass)...
        assert!(state
            .authenticate_user("sip:carol@example.com", "wrong-password")
            .is_err());
        // ...and the correct local password still works.
        assert!(state
            .authenticate_user("sip:carol@example.com", "carol-password")
            .is_ok());
    }
}
