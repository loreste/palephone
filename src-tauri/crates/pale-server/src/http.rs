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
    FileRecord, JoinConferenceRequest, SetPresenceRequest, SyncCallHistoryRequest,
    UpdateCallStatusRequest, UpdateSipAccountStatusRequest,
};

type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    let max_upload_bytes = state.max_upload_bytes().min(usize::MAX as u64) as usize;
    Router::new()
        .route("/health", get(health))
        .route("/v1/admin/login", post(admin_login))
        .route("/v1/admin/logout", post(admin_logout))
        .route("/v1/admin/refresh", post(admin_refresh))
        .route("/v1/admin/audit", get(list_audit_events))
        .route("/v1/users", get(list_users).post(create_user))
        .route("/v1/users/{id}", delete(delete_user))
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
        .route("/v1/events", get(sse_stream))
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

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "service": "pale-server" }))
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

async fn list_audit_events(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::AdminAuditEvent>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.audit_events()))
}

async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateUserRequest>,
) -> Result<Json<crate::User>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state.create_user(input);
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
    let principal = authenticated_principal(&headers, &state)?;
    let user = state.delete_user(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "user.deleted", Some(id.to_string()));
    Ok(Json(user))
}

async fn create_sip_account(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<AdminCreateSipAccountRequest>,
) -> Result<Json<crate::SipAccount>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
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
    require_bearer(&headers, &state)?;
    Ok(Json(state.sip_accounts()))
}

async fn update_sip_account_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((username, domain)): Path<(String, String)>,
    Json(input): Json<UpdateSipAccountStatusRequest>,
) -> Result<Json<crate::SipAccount>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
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
    let principal = authenticated_principal(&headers, &state)?;
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
    require_bearer(&headers, &state)?;
    Ok(Json(state.registrations()))
}

async fn list_sip_dialogs(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipDialog>>, ApiError> {
    require_bearer(&headers, &state)?;
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
    require_bearer(&headers, &state)?;
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
    let principal = authenticated_principal(&headers, &state)?;
    let call = state.create_call(input);
    state.record_audit_event(&principal, "call.created", Some(call.id.to_string()));
    Ok(Json(call))
}

async fn list_calls(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CallSession>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.calls()))
}

async fn update_call_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCallStatusRequest>,
) -> Result<Json<crate::CallSession>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
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
    let principal = authenticated_principal(&headers, &state)?;
    let rule = state.create_routing_rule(input);
    state.record_audit_event(&principal, "routing_rule.created", Some(rule.id.to_string()));
    Ok(Json(rule))
}

async fn list_routing_rules(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::RoutingRule>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.routing_rules()))
}

async fn delete_routing_rule(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::RoutingRule>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
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
    let principal = authenticated_principal(&headers, &state)?;
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
    require_bearer(&headers, &state)?;
    Ok(Json(state.sip_subscriptions()))
}

async fn list_sip_notifications(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SipNotification>>, ApiError> {
    require_bearer(&headers, &state)?;
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
    if let Some(token) = &query.token {
        state
            .principal_for_bearer(token)
            .ok_or(ApiError::Unauthorized)?;
    } else {
        require_bearer(&headers, &state)?;
    }
    let rx = state.sse_subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
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

fn authenticated_principal(headers: &HeaderMap, state: &AppState) -> Result<String, ApiError> {
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    state
        .principal_for_bearer(provided)
        .ok_or(ApiError::Unauthorized)
}

fn request_source(headers: &HeaderMap) -> String {
    header_string(headers, "x-forwarded-for")
        .and_then(|value| value.split(',').next().map(str::trim).map(ToOwned::to_owned))
        .filter(|value| !value.is_empty())
        .or_else(|| header_string(headers, "x-real-ip"))
        .unwrap_or_else(|| "direct".to_string())
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
            ApiError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}
