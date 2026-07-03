use std::collections::HashMap;
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
use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::{
    safe_filename, sip_ha1, AddRoomMemberRequest, AddTeamMemberRequest, AgentTransitionRequest,
    AppState, AssignExtensionRequest, AuthError, CallStatus, CreateAgentProfileRequest,
    CreateBusinessHoursRequest, CreateCallRequest, CreateCannedResponseRequest,
    CreateConferenceRequest, CreateExtensionRequest, CreateHolidayRequest, CreateIvrRequest,
    CreatePagingGroupRequest, CreateQueueRequest, CreateRingGroupRequest, CreateRoomRequest,
    CreateRoutingRuleRequest, CreateScheduledMeetingRequest, CreateScorecardRequest,
    CreateSipAccountRequest, CreateSpeedDialRequest, CreateTagRequest, CreateTeamRequest,
    CreateUserRequest, CreateVipCallerRequest, FileRecord, JoinConferenceRequest, ProvisionUserRequest,
    RequestCallbackInput, RoomCallMode, ScheduleRoomMessageRequest, SendRoomMessageRequest,
    SetAgentStateRequest,
    SetPresenceRequest, StartMonitorRequest, SyncCallHistoryRequest, UpdateCallStatusRequest,
    UpdateCollaborationPolicyRequest, UpdateConferenceParticipantRequest,
    UpdateScheduledMeetingRequest, UpdateSipAccountStatusRequest, UpsertRetentionPolicyRequest,
    CreateMeetingTemplateRequest, UpdateMeetingTemplateRequest,
    SetSpotlightRequest, SendMeetingReactionRequest,
    SetOutOfOfficeRequest,
    UpdateNotificationPreferenceRequest, UpdateTagRequest,
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
        .route("/v1/admin/audit/export.csv", get(export_audit_events_csv))
        .route("/v1/users", get(list_users).post(create_user))
        .route("/v1/users/{id}", delete(delete_user))
        .route("/v1/users/{id}/active", put(update_user_active))
        .route("/v1/users/{id}/role", put(update_user_role))
        .route(
            "/v1/sip/accounts",
            get(list_sip_accounts).post(create_sip_account),
        )
        .route(
            "/v1/sip/accounts/{username}/{domain}",
            put(update_sip_account_status).delete(delete_sip_account),
        )
        .route("/v1/sip/registrations", get(list_sip_registrations))
        .route("/v1/sip/dialogs", get(list_sip_dialogs))
        .route("/v1/sip/messages", get(list_sip_messages))
        .route("/v1/sip/transactions", get(list_sip_transactions))
        .route(
            "/v1/conferences",
            get(list_conferences).post(create_conference),
        )
        .route("/v1/conferences/{id}/participants", post(join_conference))
        .route("/v1/conferences/{id}/lock", put(set_conference_lock))
        .route(
            "/v1/conferences/{id}/participants/{user_id}",
            put(update_conference_participant).delete(leave_conference),
        )
        .route(
            "/v1/conferences/{id}/attendance",
            get(get_conference_attendance),
        )
        .route(
            "/v1/conferences/{id}/attendance/export",
            get(export_conference_attendance_csv),
        )
        .route(
            "/v1/conferences/{id}/spotlight",
            post(set_spotlight),
        )
        .route(
            "/v1/conferences/{id}/reactions",
            post(send_meeting_reaction),
        )
        .route(
            "/v1/conferences/{id}/chat-room",
            get(get_meeting_chat_room),
        )
        .route(
            "/v1/conferences/{id}/green-room",
            get(get_green_room).put(set_green_room_enabled),
        )
        .route(
            "/v1/conferences/{id}/green-room/join",
            post(join_green_room),
        )
        .route(
            "/v1/conferences/{id}/green-room/ready",
            post(green_room_ready),
        )
        .route("/v1/media/config", get(media_config))
        .route("/v1/calls", get(list_calls).post(create_call))
        .route("/v1/calls/{id}/status", put(update_call_status))
        .route(
            "/v1/routing/rules",
            get(list_routing_rules).post(create_routing_rule),
        )
        .route(
            "/v1/routing/rules/{id}",
            put(update_routing_rule).delete(delete_routing_rule),
        )
        .route("/v1/files", get(list_files).post(upload_file))
        .route("/v1/files/{id}", get(download_file).delete(delete_file))
        .route("/v1/sip/subscriptions", get(list_sip_subscriptions))
        .route("/v1/sip/notifications", get(list_sip_notifications))
        .route("/v1/presence", get(list_presence).put(set_presence))
        .route("/v1/presence/{sip_uri}", get(get_presence))
        .route(
            "/v1/call-history",
            get(get_call_history).post(sync_call_history),
        )
        .route("/v1/teams", get(list_teams).post(create_team))
        .route("/v1/teams/{id}", get(get_team))
        .route("/v1/teams/{id}/members", post(add_team_member))
        .route("/v1/teams/{id}/channels", post(create_team_channel))
        .route(
            "/v1/teams/{id}/tags",
            get(list_tags).post(create_tag),
        )
        .route(
            "/v1/teams/{id}/tags/{tag_id}",
            put(update_tag).delete(delete_tag),
        )
        .route("/v1/gif/search", get(gif_search))
        .route("/v1/meetings", get(list_meetings).post(create_meeting))
        .route(
            "/v1/meetings/{id}",
            put(update_meeting).delete(cancel_meeting),
        )
        .route("/v1/meetings/{id}/ics", get(export_meeting_ics))
        .route("/v1/meetings/{id}/start", post(start_meeting))
        .route(
            "/v1/admin/governance/retention",
            get(list_retention_policies).put(upsert_retention_policy),
        )
        .route(
            "/v1/admin/governance/retention/{id}",
            delete(delete_retention_policy),
        )
        .route(
            "/v1/admin/governance/retention/enforce",
            get(preview_retention_enforcement).post(apply_retention_enforcement),
        )
        .route(
            "/v1/admin/collaboration/policy",
            get(get_collaboration_policy).put(update_collaboration_policy),
        )
        .route("/v1/admin/ediscovery/export", get(discovery_export))
        .route("/v1/admin/ediscovery/search", get(discovery_search))
        .route(
            "/v1/scim/v2/Users",
            get(scim_list_users).post(scim_create_user),
        )
        .route(
            "/v1/scim/v2/Users/{id}",
            put(scim_update_user).delete(scim_delete_user),
        )
        .route("/v1/rooms", get(list_rooms).post(create_room))
        .route("/v1/rooms/{id}", get(get_room))
        .route(
            "/v1/rooms/{id}/webhooks",
            get(list_channel_webhooks).post(create_channel_webhook),
        )
        .route(
            "/v1/rooms/{id}/webhooks/{webhook_id}",
            put(update_channel_webhook).delete(delete_channel_webhook),
        )
        .route("/v1/webhooks/{token}", post(post_channel_webhook))
        .route(
            "/v1/rooms/{id}/messages",
            get(list_room_messages).post(send_room_message),
        )
        .route(
            "/v1/rooms/{id}/messages/schedule",
            post(schedule_room_message),
        )
        .route(
            "/v1/rooms/{id}/notifications",
            get(get_notification_preference).put(set_notification_preference),
        )
        .route("/v1/rooms/{id}/message-state", get(list_room_message_state))
        .route(
            "/v1/rooms/{id}/members",
            post(add_room_member).delete(leave_room),
        )
        .route(
            "/v1/rooms/{id}/call",
            post(start_room_call).delete(end_room_call),
        )
        .route("/v1/rooms/{id}/typing", post(room_typing))
        .route("/v1/search/messages", get(search_messages))
        .route("/v1/search/collaboration", get(search_collaboration))
        .route("/v1/messages/{id}/read", put(mark_message_read))
        .route(
            "/v1/messages/{id}",
            put(edit_message).delete(delete_message),
        )
        .route("/v1/messages/{id}/react", post(react_to_message))
        .route("/v1/messages/{id}/pin", put(pin_message_handler))
        .route("/v1/messages/{id}/saved", put(save_message_handler))
        .route("/v1/messages/{id}/reads", get(list_reads_handler))
        .route("/v1/rooms/{id}/pins", get(list_pinned_handler))
        .route(
            "/v1/favorites",
            get(list_favorites_handler).post(add_favorite_handler),
        )
        .route("/v1/favorites/{uri}", delete(remove_favorite_handler))
        .route("/v1/users/{id}/profile", put(update_profile_handler))
        .route("/v1/users/{id}/avatar", put(upload_avatar))
        .route(
            "/v1/call-settings",
            get(get_call_settings).put(update_call_settings),
        )
        .route("/v1/queues", get(list_queues).post(create_queue))
        .route("/v1/queues/{id}", get(get_queue).delete(delete_queue))
        .route("/v1/users/provision", post(provision_user_handler))
        .route(
            "/v1/extensions",
            get(list_extensions).post(create_extension),
        )
        .route("/v1/extensions/{ext}", delete(delete_extension))
        .route("/v1/extensions/{ext}/assign", put(assign_extension_handler))
        .route(
            "/v1/extensions/{ext}/unassign",
            put(unassign_extension_handler),
        )
        .route("/v1/dids", get(list_dids).post(create_did))
        .route("/v1/dids/{did}", delete(delete_did))
        .route(
            "/v1/business-hours",
            get(list_business_hours).post(create_business_hours),
        )
        .route(
            "/v1/business-hours/{id}",
            delete(delete_business_hours_entry),
        )
        .route("/v1/holidays", get(list_holidays).post(create_holiday))
        .route("/v1/holidays/{id}", delete(delete_holiday))
        .route("/v1/park", get(list_parked).post(park_call))
        .route("/v1/park/{slot}", post(pickup_call))
        .route(
            "/v1/speed-dials",
            get(list_speed_dials).post(create_speed_dial),
        )
        .route("/v1/cdrs", get(list_cdrs))
        .route(
            "/v1/paging-groups",
            get(list_paging_groups).post(create_paging_group),
        )
        .route("/v1/paging-groups/{id}", delete(delete_paging_group))
        .route("/v1/agents", get(list_agents).post(create_agent))
        .route("/v1/agents/{uri}", get(get_agent).delete(delete_agent))
        .route("/v1/agents/{uri}/state", put(set_agent_state))
        .route(
            "/v1/agents/{uri}/transition",
            post(transition_agent_state_handler),
        )
        .route("/v1/agents/{uri}/history", get(agent_state_history))
        .route("/v1/queues/{id}/callers", get(list_queue_callers))
        .route("/v1/queues/{id}/callback", post(request_queue_callback))
        .route("/v1/queues/{id}/callbacks", get(list_queue_callbacks))
        .route(
            "/v1/vip-callers",
            get(list_vip_callers).post(create_vip_caller),
        )
        .route("/v1/vip-callers/{id}", delete(delete_vip_caller))
        .route("/v1/wallboard", get(get_wallboard))
        .route("/v1/monitor", get(list_monitors).post(start_monitor))
        .route("/v1/monitor/{id}", delete(end_monitor))
        .route(
            "/v1/qa/scorecards",
            get(list_scorecards).post(create_scorecard),
        )
        .route("/v1/canned-responses", get(list_canned).post(create_canned))
        .route("/v1/canned-responses/{id}", delete(delete_canned))
        .route(
            "/v1/call-settings/{sip_uri}",
            get(get_user_call_settings_admin).put(update_user_call_settings_admin),
        )
        .route("/v1/ldap/config", get(get_ldap_config).put(set_ldap_config))
        .route("/v1/ldap/test", post(test_ldap_connection))
        .route(
            "/v1/ring-groups",
            get(list_ring_groups).post(create_ring_group),
        )
        .route(
            "/v1/ring-groups/{id}",
            get(get_ring_group).delete(delete_ring_group),
        )
        .route("/v1/ivrs", get(list_ivrs).post(create_ivr))
        .route("/v1/ivrs/{id}", get(get_ivr).delete(delete_ivr))
        .route("/v1/routes/resolve/{uri}", get(resolve_route))
        .route("/v1/routes/preview", get(preview_route))
        .route("/v1/voicemail", get(list_voicemails))
        .route("/v1/voicemail/{id}/listen", put(mark_voicemail_listened))
        .route("/v1/voicemail/{id}", delete(delete_voicemail))
        .route("/v1/recordings", get(list_recordings))
        .route("/v1/recordings/{id}", delete(delete_recording))
        // Meeting lobby
        .route(
            "/v1/conferences/{id}/lobby",
            get(get_lobby).put(set_lobby_settings),
        )
        .route("/v1/conferences/{id}/lobby/join", post(join_lobby))
        .route(
            "/v1/conferences/{id}/lobby/admit",
            post(admit_lobby_participant),
        )
        .route(
            "/v1/conferences/{id}/lobby/admit-all",
            post(admit_all_lobby),
        )
        // Raise hand
        .route(
            "/v1/conferences/{id}/hands",
            get(get_raised_hands).post(raise_hand),
        )
        .route(
            "/v1/conferences/{id}/hands/lower-all",
            post(lower_all_hands),
        )
        // Polls
        .route(
            "/v1/conferences/{id}/polls",
            get(list_polls).post(create_poll),
        )
        .route("/v1/polls/{id}/launch", post(launch_poll))
        .route("/v1/polls/{id}/close", post(close_poll))
        .route("/v1/polls/{id}/vote", post(cast_vote))
        // Q&A
        .route(
            "/v1/conferences/{id}/questions",
            get(list_questions).post(ask_question),
        )
        .route("/v1/questions/{id}/upvote", post(upvote_question))
        .route("/v1/questions/{id}/answer", post(answer_question))
        // Breakout rooms
        .route(
            "/v1/conferences/{id}/breakouts",
            get(list_breakouts).post(create_breakout),
        )
        .route("/v1/breakouts/{id}/start", post(start_breakout))
        .route("/v1/breakouts/{id}/close", post(close_breakout))
        // Live captions / transcription
        .route(
            "/v1/conferences/{id}/transcript",
            get(get_transcript).post(post_transcript),
        )
        .route(
            "/v1/conferences/{id}/transcript/export",
            get(export_transcript),
        )
        // Call quality
        .route(
            "/v1/call-quality",
            get(list_call_quality).post(post_call_quality),
        )
        .route("/v1/call-quality/export.csv", get(export_call_quality_csv))
        .route("/v1/call-quality/summary", get(call_quality_summary))
        // DLP
        .route(
            "/v1/admin/dlp/policies",
            get(list_dlp_policies).post(create_dlp_policy),
        )
        .route(
            "/v1/admin/dlp/policies/{id}",
            put(update_dlp_policy).delete(delete_dlp_policy),
        )
        .route("/v1/admin/dlp/violations", get(list_dlp_violations))
        .route(
            "/v1/admin/dlp/violations/export.csv",
            get(export_dlp_violations_csv),
        )
        .route("/v1/admin/dlp/scan", post(scan_content_dlp))
        // MFA / TOTP
        .route("/v1/mfa/status", get(mfa_status))
        .route("/v1/mfa/setup", post(mfa_setup))
        .route("/v1/mfa/verify", post(mfa_verify))
        .route("/v1/mfa/validate", post(mfa_validate))
        .route("/v1/mfa/disable", post(mfa_disable))
        // Session management
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions/{id}", delete(revoke_session))
        .route("/v1/sessions/revoke-all", post(revoke_all_sessions))
        // Information Barriers
        .route(
            "/v1/admin/barriers",
            get(list_barriers).post(create_barrier),
        )
        .route(
            "/v1/admin/barriers/{id}",
            put(update_barrier).delete(delete_barrier),
        )
        .route("/v1/admin/barriers/check", get(check_barrier))
        // Sensitivity Labels
        .route(
            "/v1/admin/labels",
            get(list_labels).post(create_label),
        )
        .route(
            "/v1/admin/labels/{id}",
            put(update_label).delete(delete_label),
        )
        // Custom RBAC Roles
        .route(
            "/v1/admin/roles",
            get(list_roles).post(create_role),
        )
        .route(
            "/v1/admin/roles/{id}",
            put(update_role).delete(delete_role),
        )
        .route("/v1/admin/roles/permissions", get(list_permissions))
        // Policy Packages
        .route(
            "/v1/admin/policy-packages",
            get(list_policy_packages).post(create_policy_package),
        )
        .route(
            "/v1/admin/policy-packages/{id}",
            put(update_policy_package).delete(delete_policy_package),
        )
        .route(
            "/v1/admin/policy-packages/{id}/assign",
            post(assign_policy_package),
        )
        // Bulk User Operations
        .route("/v1/admin/users/import", post(import_users_csv))
        .route("/v1/admin/users/export", get(export_users_csv))
        // Usage Analytics
        .route("/v1/admin/analytics", get(get_analytics))
        // Meeting templates
        .route(
            "/v1/admin/meeting-templates",
            get(list_meeting_templates).post(create_meeting_template),
        )
        .route(
            "/v1/admin/meeting-templates/{id}",
            put(update_meeting_template).delete(delete_meeting_template),
        )
        // Out-of-office
        .route(
            "/v1/users/out-of-office",
            get(get_out_of_office).put(set_out_of_office),
        )
        // File versioning
        .route("/v1/files/{id}/versions", get(list_file_versions))
        .route(
            "/v1/files/{id}/versions/{version}",
            get(download_file_version),
        )
        // File lock/unlock
        .route("/v1/files/{id}/lock", post(lock_file_handler))
        .route("/v1/files/{id}/unlock", post(unlock_file_handler))
        // Folders
        .route(
            "/v1/rooms/{id}/folders",
            get(list_folders).post(create_folder),
        )
        .route("/v1/folders/{id}", delete(delete_folder))
        // Approvals
        .route("/v1/approvals", get(list_approvals).post(create_approval))
        .route("/v1/approvals/{id}/respond", post(respond_to_approval))
        // Recording policies
        .route(
            "/v1/admin/recording-policies",
            get(list_recording_policies).post(create_recording_policy),
        )
        .route(
            "/v1/admin/recording-policies/{id}",
            put(update_recording_policy).delete(delete_recording_policy_handler),
        )
        // Hold music
        .route(
            "/v1/admin/hold-music",
            get(list_hold_music).post(upload_hold_music),
        )
        .route(
            "/v1/admin/hold-music/{id}",
            delete(delete_hold_music_handler),
        )
        // Call groups
        .route(
            "/v1/call-groups",
            get(list_call_groups).post(create_call_group),
        )
        .route(
            "/v1/call-groups/{id}",
            put(update_call_group).delete(delete_call_group),
        )
        // Per-user call analytics
        .route("/v1/users/{id}/call-analytics", get(user_call_analytics))
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
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

async fn root() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!DOCTYPE html>
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
</div></body></html>"#,
    )
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
        .change_user_password(
            &principal,
            input.old_password.expose(),
            input.new_password.expose(),
        )
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

#[derive(serde::Deserialize)]
struct AuditQueryParams {
    principal: Option<String>,
    action: Option<String>,
    target: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
}

impl AuditQueryParams {
    fn into_query(self) -> Result<crate::AdminAuditQuery, ApiError> {
        Ok(crate::AdminAuditQuery {
            principal: self.principal,
            action: self.action,
            target: self.target,
            from: parse_discovery_time(self.from.as_deref())?,
            to: parse_discovery_time(self.to.as_deref())?,
            limit: self.limit,
        })
    }
}

async fn list_audit_events(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<AuditQueryParams>,
) -> Result<Json<Vec<crate::AdminAuditEvent>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.search_audit_events(query.into_query()?)))
}

async fn export_audit_events_csv(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<AuditQueryParams>,
) -> Result<Response, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let events = state.search_audit_events(query.into_query()?);
    let mut csv = "created_at,principal,action,target,event_id\n".to_string();
    for event in &events {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_escape(&event.created_at.to_rfc3339()),
            csv_escape(&event.principal),
            csv_escape(&event.action),
            csv_escape(event.target.as_deref().unwrap_or("")),
            event.id
        ));
    }
    state.record_audit_event(
        &principal,
        "audit.exported",
        Some(format!("records={}", events.len())),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"audit-log.csv\""),
    );
    Ok((headers, csv).into_response())
}

async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateUserRequest>,
) -> Result<Json<crate::User>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let user = state
        .create_user(input)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "user.created", Some(user.id.to_string()));
    Ok(Json(user))
}

async fn list_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::User>>, ApiError> {
    let (_, role) = authenticated_principal_role(&headers, &state)?;
    if role == crate::ROLE_ADMIN {
        Ok(Json(state.all_users()))
    } else {
        Ok(Json(state.users()))
    }
}

async fn delete_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::User>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let user = state
        .set_user_active(id, false, &principal)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "user.deactivated", Some(id.to_string()));
    Ok(Json(user))
}

#[derive(serde::Deserialize)]
struct UpdateUserActiveRequest {
    active: bool,
}

async fn update_user_active(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateUserActiveRequest>,
) -> Result<Json<crate::User>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let user = state
        .set_user_active(id, input.active, &principal)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        if input.active {
            "user.activated"
        } else {
            "user.deactivated"
        },
        Some(id.to_string()),
    );
    Ok(Json(user))
}

#[derive(serde::Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

#[derive(serde::Deserialize)]
struct UpdateConferenceLockRequest {
    locked: bool,
}

async fn update_user_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRoleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state
        .update_user_role(id, &input.role)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "user.role_updated",
        Some(format!("{}:{}", id, input.role)),
    );
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
    state.record_audit_event(
        &principal,
        "sip_account.status_updated",
        Some(account.aor()),
    );
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
    Json(mut input): Json<JoinConferenceRequest>,
) -> Result<Json<crate::Conference>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if input.sip_uri != principal && role != crate::ROLE_ADMIN {
        return Err(ApiError::Forbidden);
    }
    if matches!(
        input.role,
        Some(crate::ParticipantRole::Host | crate::ParticipantRole::Moderator)
    ) && role != crate::ROLE_ADMIN
    {
        input.role = Some(crate::ParticipantRole::Member);
    }
    let bypass_lock =
        role == crate::ROLE_ADMIN || state.can_moderate_conference(id, &principal, false);
    let conference = state
        .join_conference(id, input, bypass_lock)
        .map_err(|err| match err {
            crate::JoinConferenceError::NotFound => ApiError::NotFound,
            crate::JoinConferenceError::Locked => {
                ApiError::Conflict("meeting is locked".to_string())
            }
        })?;
    state.record_audit_event(
        &principal,
        "conference.participant_joined",
        Some(id.to_string()),
    );
    Ok(Json(conference))
}

async fn set_conference_lock(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateConferenceLockRequest>,
) -> Result<Json<crate::Conference>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let conference = state
        .set_conference_locked(id, input.locked)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        if input.locked {
            "conference.locked"
        } else {
            "conference.unlocked"
        },
        Some(id.to_string()),
    );
    Ok(Json(conference))
}

async fn leave_conference(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<crate::Conference>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    let target = state
        .conference_participant(id, user_id)
        .ok_or(ApiError::NotFound)?;
    let self_leave = target.sip_uri == principal;
    if !self_leave && !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
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

async fn update_conference_participant(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateConferenceParticipantRequest>,
) -> Result<Json<crate::Conference>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let conference = state
        .update_conference_participant(id, user_id, input, &principal)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "conference.participant_updated",
        Some(format!("{id}:{user_id}")),
    );
    Ok(Json(conference))
}

async fn get_conference_attendance(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::ConferenceAttendanceRecord>>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let records = state.conference_attendance(id);
    state.record_audit_event(
        &principal,
        "conference.attendance_viewed",
        Some(format!("{}:{}", id, records.len())),
    );
    Ok(Json(records))
}

// ── Attendance CSV export ──────────────────────────────────────────

async fn export_conference_attendance_csv(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let format = params.get("format").map(|s| s.as_str()).unwrap_or("csv");
    if format != "csv" {
        return Err(ApiError::Conflict("only csv format supported".to_string()));
    }
    let csv = state.export_attendance_csv(id);
    state.record_audit_event(
        &principal,
        "conference.attendance_exported_csv",
        Some(id.to_string()),
    );
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    resp_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"attendance-{}.csv\"", id))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"attendance.csv\"")),
    );
    Ok((resp_headers, csv).into_response())
}

// ── Spotlight ─────────────────────────────────────────────────────

async fn set_spotlight(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SetSpotlightRequest>,
) -> Result<Json<crate::Conference>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let conference = state
        .set_spotlight(id, input.participant_id)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "conference.spotlight_set",
        Some(format!("{id}:{:?}", input.participant_id)),
    );
    Ok(Json(conference))
}

// ── Meeting reactions ─────────────────────────────────────────────

async fn send_meeting_reaction(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SendMeetingReactionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state.broadcast_meeting_reaction(id, &principal, &input.emoji);
    Ok(Json(json!({ "ok": true })))
}

// ── Meeting chat room ─────────────────────────────────────────────

async fn get_meeting_chat_room(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let conference = state
        .get_conference(id)
        .ok_or(ApiError::NotFound)?;
    let chat_room_id = state.ensure_meeting_chat_room(
        id,
        &conference.title,
        &principal,
    );
    Ok(Json(json!({ "chat_room_id": chat_room_id })))
}

// ── Green room handlers ───────────────────────────────────────────

async fn get_green_room(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::GreenRoomState>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.get_green_room(id)))
}

async fn set_green_room_enabled(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<crate::Conference>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !state.can_moderate_conference(id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let enabled = input.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let conference = state
        .set_green_room_enabled(id, enabled)
        .ok_or(ApiError::NotFound)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "green_room_updated".to_string(),
        payload: serde_json::to_value(&state.get_green_room(id)).unwrap_or_default(),
    });
    Ok(Json(conference))
}

async fn join_green_room(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::JoinConferenceRequest>,
) -> Result<Json<crate::GreenRoomState>, ApiError> {
    require_bearer(&headers, &state)?;
    let green_room = state.join_green_room(id, input.user_id, input.sip_uri);
    state.broadcast_sse(crate::SseEvent {
        event_type: "green_room_updated".to_string(),
        payload: serde_json::to_value(&green_room).unwrap_or_default(),
    });
    Ok(Json(green_room))
}

async fn green_room_ready(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<crate::GreenRoomState>, ApiError> {
    require_bearer(&headers, &state)?;
    let user_id = input
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(ApiError::Conflict("user_id required".to_string()))?;
    let green_room = state.set_green_room_ready(id, user_id);
    state.broadcast_sse(crate::SseEvent {
        event_type: "green_room_updated".to_string(),
        payload: serde_json::to_value(&green_room).unwrap_or_default(),
    });
    Ok(Json(green_room))
}

// ── Meeting template handlers ─────────────────────────────────────

async fn list_meeting_templates(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::MeetingTemplate>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_meeting_templates()))
}

async fn create_meeting_template(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateMeetingTemplateRequest>,
) -> Result<Json<crate::MeetingTemplate>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let template = state.create_meeting_template(&principal, input);
    state.record_audit_event(
        &principal,
        "meeting_template.created",
        Some(template.id.to_string()),
    );
    Ok(Json(template))
}

async fn update_meeting_template(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateMeetingTemplateRequest>,
) -> Result<Json<crate::MeetingTemplate>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let template = state
        .update_meeting_template(id, input)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "meeting_template.updated",
        Some(id.to_string()),
    );
    Ok(Json(template))
}

async fn delete_meeting_template(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_meeting_template(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(
        &principal,
        "meeting_template.deleted",
        Some(id.to_string()),
    );
    Ok(Json(json!({ "ok": true })))
}

// ── Out-of-office handlers ────────────────────────────────────────

async fn get_out_of_office(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::OutOfOfficeSettings>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.get_out_of_office(&principal)))
}

async fn set_out_of_office(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<SetOutOfOfficeRequest>,
) -> Result<Json<crate::OutOfOfficeSettings>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let result = state.set_out_of_office(&principal, input);
    state.record_audit_event(
        &principal,
        "user.out_of_office_updated",
        None,
    );
    Ok(Json(result))
}

// ── Meeting lobby handlers ─────────────────────────────────────────

async fn get_lobby(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::ConferenceLobby>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.get_lobby(id)))
}

async fn set_lobby_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::LobbySettingsRequest>,
) -> Result<Json<crate::ConferenceLobby>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let lobby = state.set_lobby_enabled(id, input.enabled);
    state.record_audit_event(&principal, "lobby.settings_updated", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "lobby_updated".to_string(),
        payload: serde_json::to_value(&lobby).unwrap(),
    });
    Ok(Json(lobby))
}

async fn join_lobby(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::JoinConferenceRequest>,
) -> Result<Json<crate::ConferenceLobby>, ApiError> {
    require_bearer(&headers, &state)?;
    let display = input.sip_uri.clone();
    let lobby = state.join_lobby(id, input.user_id, input.sip_uri, display);
    state.broadcast_sse(crate::SseEvent {
        event_type: "lobby_updated".to_string(),
        payload: serde_json::to_value(&lobby).unwrap(),
    });
    Ok(Json(lobby))
}

async fn admit_lobby_participant(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::LobbyAdmitRequest>,
) -> Result<Json<crate::ConferenceLobby>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let lobby = state
        .admit_lobby_participant(id, input.user_id, input.admit)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        if input.admit {
            "lobby.participant_admitted"
        } else {
            "lobby.participant_rejected"
        },
        Some(format!("{id}:{}", input.user_id)),
    );
    state.broadcast_sse(crate::SseEvent {
        event_type: "lobby_updated".to_string(),
        payload: serde_json::to_value(&lobby).unwrap(),
    });
    Ok(Json(lobby))
}

async fn admit_all_lobby(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::ConferenceLobby>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let lobby = state.admit_all_lobby(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "lobby.all_admitted", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "lobby_updated".to_string(),
        payload: serde_json::to_value(&lobby).unwrap(),
    });
    Ok(Json(lobby))
}

// ── Raise hand handlers ───────────────────────────────────────────

async fn get_raised_hands(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::HandRaise>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.get_raised_hands(id)))
}

async fn raise_hand(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::RaiseHandRequest>,
) -> Result<Json<Vec<crate::HandRaise>>, ApiError> {
    require_bearer(&headers, &state)?;
    let hands = if input.raised {
        state.raise_hand(id, input.user_id, input.sip_uri)
    } else {
        state.lower_hand(id, input.user_id)
    };
    state.broadcast_sse(crate::SseEvent {
        event_type: "hand_raised".to_string(),
        payload: serde_json::json!({
            "conference_id": id,
            "hands": hands,
        }),
    });
    Ok(Json(hands))
}

async fn lower_all_hands(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::HandRaise>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let hands = state.lower_all_hands(id);
    state.record_audit_event(&principal, "hands.lowered_all", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "hand_raised".to_string(),
        payload: serde_json::json!({
            "conference_id": id,
            "hands": hands,
        }),
    });
    Ok(Json(hands))
}

// ── Poll handlers ─────────────────────────────────────────────────

async fn list_polls(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::MeetingPoll>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_polls(id)))
}

async fn create_poll(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CreatePollRequest>,
) -> Result<Json<crate::MeetingPoll>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let poll = state.create_poll(id, &principal, input);
    state.record_audit_event(&principal, "poll.created", Some(poll.id.to_string()));
    Ok(Json(poll))
}

async fn launch_poll(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MeetingPoll>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let poll = state.launch_poll(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "poll.launched", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "poll_updated".to_string(),
        payload: serde_json::to_value(&poll).unwrap(),
    });
    Ok(Json(poll))
}

async fn close_poll(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MeetingPoll>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let poll = state.close_poll(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "poll.closed", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "poll_updated".to_string(),
        payload: serde_json::to_value(&poll).unwrap(),
    });
    Ok(Json(poll))
}

async fn cast_vote(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CastVoteRequest>,
) -> Result<Json<crate::MeetingPoll>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let poll = state
        .cast_vote(id, &principal, input.option_ids)
        .ok_or(ApiError::NotFound)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "poll_updated".to_string(),
        payload: serde_json::to_value(&poll).unwrap(),
    });
    Ok(Json(poll))
}

// ── Q&A handlers ──────────────────────────────────────────────────

async fn list_questions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::QaQuestion>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_questions(id)))
}

async fn ask_question(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::AskQuestionRequest>,
) -> Result<Json<crate::QaQuestion>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let q = state.ask_question(id, &principal, input.text);
    state.broadcast_sse(crate::SseEvent {
        event_type: "qa_updated".to_string(),
        payload: serde_json::to_value(&q).unwrap(),
    });
    Ok(Json(q))
}

async fn upvote_question(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::QaQuestion>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let q = state
        .upvote_question(id, &principal)
        .ok_or(ApiError::NotFound)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "qa_updated".to_string(),
        payload: serde_json::to_value(&q).unwrap(),
    });
    Ok(Json(q))
}

async fn answer_question(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::AnswerQuestionRequest>,
) -> Result<Json<crate::QaQuestion>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let q = state
        .answer_question(id, input.answer)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "qa.answered", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "qa_updated".to_string(),
        payload: serde_json::to_value(&q).unwrap(),
    });
    Ok(Json(q))
}

// ── Breakout room handlers ────────────────────────────────────────

async fn list_breakouts(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::BreakoutSession>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_breakouts(id)))
}

async fn create_breakout(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CreateBreakoutRequest>,
) -> Result<Json<crate::BreakoutSession>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let session = state.create_breakout_session(id, input);
    state.record_audit_event(&principal, "breakout.created", Some(session.id.to_string()));
    Ok(Json(session))
}

async fn start_breakout(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::BreakoutSession>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let session = state.start_breakout(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "breakout.started", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "breakout_updated".to_string(),
        payload: serde_json::to_value(&session).unwrap(),
    });
    Ok(Json(session))
}

async fn close_breakout(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::BreakoutSession>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let session = state.close_breakout(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "breakout.closed", Some(id.to_string()));
    state.broadcast_sse(crate::SseEvent {
        event_type: "breakout_updated".to_string(),
        payload: serde_json::to_value(&session).unwrap(),
    });
    Ok(Json(session))
}

// ── Transcript / live captions handlers ───────────────────────────

async fn get_transcript(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::TranscriptSegment>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.get_transcript(id)))
}

async fn post_transcript(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::PostTranscriptRequest>,
) -> Result<Json<crate::TranscriptSegment>, ApiError> {
    require_bearer(&headers, &state)?;
    if !state.collaboration_policy().meeting_recording_enabled {
        return Err(ApiError::Conflict(
            "meeting recording is disabled by policy".to_string(),
        ));
    }
    let segment = state.post_transcript(id, input);
    state.broadcast_sse(crate::SseEvent {
        event_type: "live_caption".to_string(),
        payload: serde_json::to_value(&segment).unwrap(),
    });
    Ok(Json(segment))
}

async fn export_transcript(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::TranscriptExport>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.export_transcript(id)))
}

// ── Call quality handlers ─────────────────────────────────────────

#[derive(serde::Deserialize)]
struct CallQualityQueryParams {
    user_sip_uri: Option<String>,
    call_id: Option<Uuid>,
    rating: Option<crate::CallQualityRating>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
}

impl CallQualityQueryParams {
    fn into_query(self) -> Result<crate::CallQualityQuery, ApiError> {
        Ok(crate::CallQualityQuery {
            user_sip_uri: self.user_sip_uri,
            call_id: self.call_id,
            rating: self.rating,
            from: parse_discovery_time(self.from.as_deref())?,
            to: parse_discovery_time(self.to.as_deref())?,
            limit: self.limit,
        })
    }
}

async fn list_call_quality(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<CallQualityQueryParams>,
) -> Result<Json<Vec<crate::CallQualityReport>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.search_call_quality(query.into_query()?)))
}

async fn post_call_quality(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::PostCallQualityRequest>,
) -> Result<Json<crate::CallQualityReport>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let report = state.post_call_quality(&principal, input);
    Ok(Json(report))
}

async fn call_quality_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::CallQualitySummary>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.call_quality_summary()))
}

async fn export_call_quality_csv(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<CallQualityQueryParams>,
) -> Result<Response, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let records = state.search_call_quality(query.into_query()?);
    let mut csv =
        "reported_at,user_sip_uri,call_id,rating,codec,mos,jitter_ms,packet_loss_pct,round_trip_ms,bytes_sent,bytes_received,issues,recommended_action\n"
            .to_string();
    for record in &records {
        csv.push_str(&format!(
            "{},{},{},{},{},{:.2},{:.1},{:.2},{:.0},{},{},{},{}\n",
            csv_escape(&record.reported_at.to_rfc3339()),
            csv_escape(&record.user_sip_uri),
            record.call_id,
            call_quality_rating_label(record.rating),
            csv_escape(&record.codec),
            record.mos_score,
            record.jitter_ms,
            record.packet_loss_pct,
            record.round_trip_ms,
            record.bytes_sent,
            record.bytes_received,
            csv_escape(&record.issues.join(";")),
            csv_escape(record.recommended_action.as_deref().unwrap_or(""))
        ));
    }
    state.record_audit_event(
        &principal,
        "call_quality.exported",
        Some(format!("records={}", records.len())),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"call-quality.csv\""),
    );
    Ok((headers, csv).into_response())
}

fn call_quality_rating_label(rating: crate::CallQualityRating) -> &'static str {
    match rating {
        crate::CallQualityRating::Good => "good",
        crate::CallQualityRating::Warning => "warning",
        crate::CallQualityRating::Poor => "poor",
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

// ── DLP handlers ──────────────────────────────────────────────────

async fn list_dlp_policies(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::DlpPolicy>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_dlp_policies()))
}

async fn create_dlp_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateDlpPolicyRequest>,
) -> Result<Json<crate::DlpPolicy>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let policy = state
        .create_dlp_policy(&principal, input)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(
        &principal,
        "dlp.policy_created",
        Some(policy.id.to_string()),
    );
    Ok(Json(policy))
}

async fn delete_dlp_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_dlp_policy(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "dlp.policy_deleted", Some(id.to_string()));
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn update_dlp_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::UpdateDlpPolicyRequest>,
) -> Result<Json<crate::DlpPolicy>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let policy = state
        .update_dlp_policy(id, input)
        .map_err(ApiError::Conflict)?
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "dlp.policy_updated",
        Some(policy.id.to_string()),
    );
    Ok(Json(policy))
}

#[derive(serde::Deserialize)]
struct DlpViolationQueryParams {
    policy: Option<String>,
    user_uri: Option<String>,
    action: Option<crate::DlpAction>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
}

impl DlpViolationQueryParams {
    fn into_query(self) -> Result<crate::DlpViolationQuery, ApiError> {
        Ok(crate::DlpViolationQuery {
            policy: self.policy,
            user_uri: self.user_uri,
            action: self.action,
            from: parse_discovery_time(self.from.as_deref())?,
            to: parse_discovery_time(self.to.as_deref())?,
            limit: self.limit,
        })
    }
}

async fn list_dlp_violations(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<DlpViolationQueryParams>,
) -> Result<Json<Vec<crate::DlpViolation>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.search_dlp_violations(query.into_query()?)))
}

async fn export_dlp_violations_csv(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<DlpViolationQueryParams>,
) -> Result<Response, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let violations = state.search_dlp_violations(query.into_query()?);
    let mut csv =
        "detected_at,user_uri,policy_name,policy_id,action_taken,content_snippet,violation_id\n"
            .to_string();
    for violation in &violations {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_escape(&violation.detected_at.to_rfc3339()),
            csv_escape(&violation.user_uri),
            csv_escape(&violation.policy_name),
            violation.policy_id,
            dlp_action_label(&violation.action_taken),
            csv_escape(&violation.content_snippet),
            violation.id
        ));
    }
    state.record_audit_event(
        &principal,
        "dlp.violations_exported",
        Some(format!("records={}", violations.len())),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"dlp-violations.csv\""),
    );
    Ok((headers, csv).into_response())
}

fn dlp_action_label(action: &crate::DlpAction) -> &'static str {
    match action {
        crate::DlpAction::Block => "block",
        crate::DlpAction::Warn => "warn",
        crate::DlpAction::Audit => "audit",
    }
}

async fn scan_content_dlp(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<crate::DlpScanResult>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let result = state.preview_content_dlp(&principal, content);
    state.record_audit_event(
        &principal,
        "dlp.scan_tested",
        Some(format!("matches={}", result.violations.len())),
    );
    Ok(Json(result))
}

// ─── MFA / TOTP Handlers ───

async fn mfa_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::MfaStatusResponse>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state
        .user_by_sip_uri(&principal)
        .ok_or(ApiError::NotFound)?;
    let enabled = state.is_mfa_enabled(user.id);
    Ok(Json(crate::MfaStatusResponse { enabled }))
}

async fn mfa_setup(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::MfaSetupResponse>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state
        .user_by_sip_uri(&principal)
        .ok_or(ApiError::NotFound)?;
    let response = state
        .mfa_setup(user.id, &user.sip_uri)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "mfa.setup_initiated", None);
    Ok(Json(response))
}

async fn mfa_verify(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::MfaVerifyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state
        .user_by_sip_uri(&principal)
        .ok_or(ApiError::NotFound)?;
    state
        .mfa_verify_enable(user.id, &input.code)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "mfa.enabled", None);
    Ok(Json(json!({ "ok": true, "mfa_enabled": true })))
}

async fn mfa_validate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::MfaValidateRequest>,
) -> Result<Json<crate::UserLoginResponse>, ApiError> {
    // The bearer token here is the temporary mfa_pending token
    let token = bearer_token(&headers);
    let session = state
        .admin_sessions
        .get(&token.to_string())
        .ok_or(ApiError::Unauthorized)?;
    if session.role != "mfa_pending" {
        return Err(ApiError::Conflict(
            "Token is not an MFA pending token".to_string(),
        ));
    }
    let user = state
        .user_by_sip_uri(&session.principal)
        .ok_or(ApiError::Unauthorized)?;

    // Validate the TOTP code
    let valid = state
        .mfa_validate(user.id, &input.code)
        .map_err(|e| ApiError::Conflict(e))?;
    if !valid {
        return Err(ApiError::Unauthorized);
    }

    // Remove the MFA pending session
    state.admin_sessions.remove(&token.to_string());

    // Create a real session
    let real_session = crate::AdminSession {
        token: Uuid::new_v4().to_string(),
        principal: user.sip_uri.clone(),
        role: user.role.clone(),
        expires_at: Utc::now() + chrono::Duration::hours(12),
    };
    state
        .admin_sessions
        .insert(real_session.token.clone(), real_session.clone());

    // Track session
    let ip = request_source(&headers);
    state.track_session(user.id, &real_session.token, "Desktop", "desktop", &ip);

    // Set presence to online
    state.update_presence(&user.sip_uri, crate::PresenceStatus::Online, None);

    // Build SIP credentials
    let sip_creds = crate::split_sip_aor_simple(&user.sip_uri).map(|(username, domain)| {
        crate::SipCredentials {
            sip_uri: user.sip_uri.clone(),
            registrar_uri: None,
            registration_available: state.sip_registration_available(),
            username,
            password: String::new(), // Password not available after MFA flow
            transport: "udp".to_string(),
            domain,
        }
    });

    state.record_audit_event(&user.sip_uri, "mfa.validated", None);

    Ok(Json(crate::UserLoginResponse {
        token: real_session.token,
        user,
        sip_credentials: sip_creds,
        expires_at: real_session.expires_at,
        mfa_required: false,
    }))
}

async fn mfa_disable(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state
        .user_by_sip_uri(&principal)
        .ok_or(ApiError::NotFound)?;
    state.mfa_disable(user.id);
    state.record_audit_event(&principal, "mfa.disabled", None);
    Ok(Json(json!({ "ok": true, "mfa_enabled": false })))
}

// ─── Session Management Handlers ───

async fn list_sessions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::UserSessionInfo>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state
        .user_by_sip_uri(&principal)
        .ok_or(ApiError::NotFound)?;
    let current_token = bearer_token(&headers).to_string();
    let sessions = state.list_sessions(user.id, &current_token);
    Ok(Json(sessions))
}

async fn revoke_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let revoked = state.revoke_session_by_id(id);
    if !revoked {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "session.revoked", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

async fn revoke_all_sessions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let user = state
        .user_by_sip_uri(&principal)
        .ok_or(ApiError::NotFound)?;
    let current_token = bearer_token(&headers).to_string();
    let count = state.revoke_all_sessions(user.id, &current_token);
    state.record_audit_event(
        &principal,
        "sessions.revoked_all",
        Some(format!("revoked={}", count)),
    );
    Ok(Json(json!({ "ok": true, "revoked": count })))
}

// ── Information Barriers handlers ────────────────────────────────

async fn list_barriers(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::InformationBarrier>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_barriers()))
}

async fn create_barrier(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateInformationBarrierRequest>,
) -> Result<Json<crate::InformationBarrier>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let barrier = state.create_barrier(input);
    state.record_audit_event(&principal, "barrier.created", Some(barrier.id.to_string()));
    Ok(Json(barrier))
}

async fn update_barrier(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::UpdateInformationBarrierRequest>,
) -> Result<Json<crate::InformationBarrier>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let barrier = state.update_barrier(id, input).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "barrier.updated", Some(id.to_string()));
    Ok(Json(barrier))
}

async fn delete_barrier(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_barrier(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "barrier.deleted", Some(id.to_string()));
    Ok(Json(json!({ "deleted": true })))
}

#[derive(serde::Deserialize)]
struct BarrierCheckParams {
    user_a: String,
    user_b: String,
    #[serde(default)]
    is_call: bool,
}

async fn check_barrier(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(params): Query<BarrierCheckParams>,
) -> Result<Json<crate::BarrierCheckResult>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(
        state.check_barrier(&params.user_a, &params.user_b, params.is_call),
    ))
}

// ── Sensitivity Labels handlers ─────────────────────────────────

async fn list_labels(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SensitivityLabel>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_labels()))
}

async fn create_label(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateSensitivityLabelRequest>,
) -> Result<Json<crate::SensitivityLabel>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let label = state.create_label(input);
    state.record_audit_event(&principal, "label.created", Some(label.id.to_string()));
    Ok(Json(label))
}

async fn update_label(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::UpdateSensitivityLabelRequest>,
) -> Result<Json<crate::SensitivityLabel>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let label = state.update_label(id, input).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "label.updated", Some(id.to_string()));
    Ok(Json(label))
}

async fn delete_label(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_label(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "label.deleted", Some(id.to_string()));
    Ok(Json(json!({ "deleted": true })))
}

// ── Custom RBAC Roles handlers ──────────────────────────────────

async fn list_roles(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CustomRole>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_custom_roles()))
}

async fn create_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateCustomRoleRequest>,
) -> Result<Json<crate::CustomRole>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let role = state
        .create_custom_role(input)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "role.created", Some(role.id.to_string()));
    Ok(Json(role))
}

async fn update_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::UpdateCustomRoleRequest>,
) -> Result<Json<crate::CustomRole>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let role = state
        .update_custom_role(id, input)
        .map_err(ApiError::Conflict)?
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "role.updated", Some(id.to_string()));
    Ok(Json(role))
}

async fn delete_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_custom_role(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "role.deleted", Some(id.to_string()));
    Ok(Json(json!({ "deleted": true })))
}

async fn list_permissions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<&'static str>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(crate::permissions::all()))
}

// ── Policy Packages handlers ────────────────────────────────────

async fn list_policy_packages(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::PolicyPackage>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_policy_packages()))
}

async fn create_policy_package(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreatePolicyPackageRequest>,
) -> Result<Json<crate::PolicyPackage>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let pkg = state.create_policy_package(input);
    state.record_audit_event(&principal, "package.created", Some(pkg.id.to_string()));
    Ok(Json(pkg))
}

async fn update_policy_package(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::UpdatePolicyPackageRequest>,
) -> Result<Json<crate::PolicyPackage>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let pkg = state
        .update_policy_package(id, input)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "package.updated", Some(id.to_string()));
    Ok(Json(pkg))
}

async fn delete_policy_package(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_policy_package(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "package.deleted", Some(id.to_string()));
    Ok(Json(json!({ "deleted": true })))
}

async fn assign_policy_package(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::AssignPolicyPackageRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    // Verify package exists
    let packages = state.list_policy_packages();
    if !packages.iter().any(|p| p.id == id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(
        &principal,
        "package.assigned",
        Some(format!("package={} users={}", id, input.user_ids.len())),
    );
    Ok(Json(json!({
        "assigned": true,
        "package_id": id.to_string(),
        "user_count": input.user_ids.len()
    })))
}

// ── Bulk User Operations handlers ───────────────────────────────

async fn import_users_csv(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<crate::BulkImportResult>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let csv_data =
        String::from_utf8(body.to_vec()).map_err(|_| ApiError::Conflict("invalid UTF-8".into()))?;
    let result = state.import_users_csv(&csv_data);
    state.record_audit_event(
        &principal,
        "users.imported",
        Some(format!(
            "imported={} skipped={} errors={}",
            result.imported,
            result.skipped,
            result.errors.len()
        )),
    );
    Ok(Json(result))
}

async fn export_users_csv(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let csv = state.export_users_csv();
    state.record_audit_event(&principal, "users.exported", None);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"users-export.csv\""),
    );
    Ok((response_headers, csv).into_response())
}

// ── Usage Analytics handler ─────────────────────────────────────

async fn get_analytics(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::UsageAnalytics>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.usage_analytics()))
}

async fn create_call(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateCallRequest>,
) -> Result<Json<crate::CallSession>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let call = state.create_call(input).map_err(ApiError::Conflict)?;
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
    state.record_audit_event(
        &principal,
        "routing_rule.created",
        Some(rule.id.to_string()),
    );
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
    let rule = state.delete_routing_rule(id).ok_or(ApiError::NotFound)?;
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
    let governance = state.file_governance_for_upload(&owner, &filename, &content_type, &body);
    if !governance.allowed {
        state.record_audit_event(&owner, "file.upload_blocked_dlp", Some(filename));
        return Err(ApiError::Conflict("file blocked by DLP policy".to_string()));
    }

    let folder_id = header_string(&headers, "x-pale-folder-id")
        .and_then(|s| Uuid::parse_str(&s).ok());
    let room_id = header_string(&headers, "x-pale-room-id")
        .and_then(|s| Uuid::parse_str(&s).ok());

    tokio::fs::create_dir_all(state.files_dir()).await?;

    let mut hasher = Sha256::new();
    hasher.update(&body);
    let sha256 = to_hex(&hasher.finalize());

    // Check if a file with the same name exists in the same room/folder — if so, create a version
    let existing = room_id.and_then(|_rid| {
        state
            .file_records()
            .into_iter()
            .find(|f| f.filename == filename && f.folder_id == folder_id && f.owner == owner)
    });

    if let Some(existing_file) = existing {
        // Check file lock before allowing overwrite
        if let Some(locker) = &existing_file.locked_by {
            if *locker != owner {
                return Err(ApiError::Conflict(format!(
                    "file is locked by {}",
                    locker
                )));
            }
        }
        // Create a new version of the existing file
        let versions = state.file_versions(existing_file.id);
        let next_version = versions
            .last()
            .map(|v| v.version_number + 1)
            .unwrap_or(2); // first versioned upload is v2

        // Save current file as a version if this is the first time versioning
        if versions.is_empty() {
            let v1_id = Uuid::new_v4();
            let v1_path = state.file_version_path(v1_id);
            // Copy current file content to version storage
            if let Ok(current_bytes) = tokio::fs::read(state.file_path(existing_file.id)).await {
                let _ = tokio::fs::write(&v1_path, &current_bytes).await;
            }
            state.add_file_version(crate::FileVersion {
                id: v1_id,
                file_id: existing_file.id,
                version_number: 1,
                uploader: existing_file.owner.clone(),
                size: existing_file.size as i64,
                sha256: existing_file.sha256.clone(),
                created_at: existing_file.created_at,
                storage_path: v1_path.to_string_lossy().to_string(),
            });
        }

        // Save the new upload as the latest version
        let ver_id = Uuid::new_v4();
        let ver_path = state.file_version_path(ver_id);
        tokio::fs::write(&ver_path, &body).await?;
        state.add_file_version(crate::FileVersion {
            id: ver_id,
            file_id: existing_file.id,
            version_number: next_version,
            uploader: owner.clone(),
            size: body.len() as i64,
            sha256: sha256.clone(),
            created_at: Utc::now(),
            storage_path: ver_path.to_string_lossy().to_string(),
        });

        // Update the main file record with new content
        let path = state.file_path(existing_file.id);
        tokio::fs::write(&path, &body).await?;

        let mut updated = existing_file;
        updated.size = body.len() as u64;
        updated.sha256 = sha256;
        updated.dlp_status = governance.dlp_status;
        updated.dlp_violation_count = governance.dlp_violation_count;
        updated.legal_hold = governance.legal_hold;
        state.put_file_record(updated.clone());
        state.record_audit_event(
            &owner,
            "file.version_uploaded",
            Some(format!("{}:v{}", updated.id, next_version)),
        );
        return Ok(Json(updated));
    }

    let id = Uuid::new_v4();
    let path = state.file_path(id);
    tokio::fs::write(&path, &body).await?;

    let record = FileRecord {
        id,
        owner: owner.clone(),
        filename,
        content_type,
        size: body.len() as u64,
        sha256,
        created_at: Utc::now(),
        dlp_status: governance.dlp_status,
        dlp_violation_count: governance.dlp_violation_count,
        legal_hold: governance.legal_hold,
        deleted_at: None,
        deleted_by: None,
        folder_id,
        locked_by: None,
        locked_at: None,
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
    if record.deleted_at.is_some() {
        return Err(ApiError::NotFound);
    }
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
    let disposition = format!(
        "attachment; filename=\"{}\"",
        record.filename.replace('"', "")
    );
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
    if record.deleted_at.is_some() {
        return Err(ApiError::NotFound);
    }
    if !state.is_admin_principal(&requester) && requester != record.owner {
        return Err(ApiError::Forbidden);
    }
    let record = if record.legal_hold || state.file_on_legal_hold() {
        state
            .mark_file_deleted(id, &requester)
            .ok_or(ApiError::NotFound)?
    } else {
        let record = state.delete_file_record(id).ok_or(ApiError::NotFound)?;
        match tokio::fs::remove_file(state.file_path(id)).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        record
    };
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

// ─── Teams, Channels, Meetings ───

async fn list_teams(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Team>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.list_teams_for_user(&principal)))
}

async fn create_team(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateTeamRequest>,
) -> Result<Json<crate::Team>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state
        .authorize_external_participants(&principal, &input.members)
        .map_err(ApiError::Conflict)?;
    let team = state.create_team(&principal, input);
    state.record_audit_event(&principal, "team.created", Some(team.id.to_string()));
    Ok(Json(team))
}

async fn get_team(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::Team>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let team = state.team(id).ok_or(ApiError::NotFound)?;
    if !team
        .members
        .iter()
        .any(|member| member.user_sip_uri == principal)
    {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(team))
}

async fn add_team_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<AddTeamMemberRequest>,
) -> Result<Json<crate::Team>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let team = state.team(id).ok_or(ApiError::NotFound)?;
    if !team.members.iter().any(|member| {
        member.user_sip_uri == principal && (member.role == "owner" || member.role == "admin")
    }) {
        return Err(ApiError::Forbidden);
    }
    state
        .authorize_external_participants(&principal, std::slice::from_ref(&input.user_sip_uri))
        .map_err(ApiError::Conflict)?;
    let team = state
        .add_team_member(id, &input.user_sip_uri, input.role)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "team.member_added", Some(id.to_string()));
    Ok(Json(team))
}

async fn create_team_channel(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateRoomRequest>,
) -> Result<Json<crate::Room>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state
        .authorize_external_participants(&principal, &input.members)
        .map_err(ApiError::Conflict)?;
    let room = state
        .create_team_channel(&principal, id, input)
        .ok_or(ApiError::Forbidden)?;
    state.record_audit_event(
        &principal,
        "team.channel_created",
        Some(room.id.to_string()),
    );
    Ok(Json(room))
}

// ─── Tags ───

async fn list_tags(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::Tag>>, ApiError> {
    authenticated_principal(&headers, &state)?;
    Ok(Json(state.list_tags(id)))
}

async fn create_tag(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateTagRequest>,
) -> Result<Json<crate::Tag>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let tag = state
        .create_tag(id, &input.name, input.members)
        .map_err(ApiError::Conflict)?;
    if let Some(pg) = state.pg_store() {
        let tag_for_pg = tag.clone();
        let pg = pg.clone();
        tokio::spawn(async move {
            if let Err(e) = pg.upsert_tag(&tag_for_pg).await {
                log::warn!("Failed to persist tag: {}", e);
            }
        });
    }
    state.record_audit_event(&principal, "tag.created", Some(tag.id.to_string()));
    Ok(Json(tag))
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct TagPath {
    id: Uuid,
    tag_id: Uuid,
}

async fn update_tag(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(path): Path<TagPath>,
    Json(input): Json<UpdateTagRequest>,
) -> Result<Json<crate::Tag>, ApiError> {
    authenticated_principal(&headers, &state)?;
    let tag = state
        .update_tag(path.tag_id, input.name, input.members)
        .ok_or(ApiError::NotFound)?;
    if let Some(pg) = state.pg_store() {
        let tag_for_pg = tag.clone();
        let pg = pg.clone();
        tokio::spawn(async move {
            if let Err(e) = pg.upsert_tag(&tag_for_pg).await {
                log::warn!("Failed to persist tag: {}", e);
            }
        });
    }
    Ok(Json(tag))
}

async fn delete_tag(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(path): Path<TagPath>,
) -> Result<Json<crate::Tag>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let tag = state.delete_tag(path.tag_id).ok_or(ApiError::NotFound)?;
    if let Some(pg) = state.pg_store() {
        let pg = pg.clone();
        let tag_id = path.tag_id;
        tokio::spawn(async move {
            if let Err(e) = pg.delete_tag(tag_id).await {
                log::warn!("Failed to delete tag from PG: {}", e);
            }
        });
    }
    state.record_audit_event(&principal, "tag.deleted", Some(tag.id.to_string()));
    Ok(Json(tag))
}

// ─── GIF Search Proxy ───

#[derive(serde::Deserialize)]
struct GifSearchQuery {
    q: String,
    limit: Option<usize>,
}

async fn gif_search(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<GifSearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authenticated_principal(&headers, &state)?;
    let api_key = std::env::var("PALE_TENOR_API_KEY")
        .or_else(|_| std::env::var("PALE_GIPHY_API_KEY"))
        .unwrap_or_default();
    let limit = query.limit.unwrap_or(20).min(50);

    if api_key.is_empty() {
        // Return empty results if no API key configured
        return Ok(Json(json!({ "results": [] })));
    }

    // Determine provider from env
    let is_tenor = std::env::var("PALE_TENOR_API_KEY").is_ok();
    let url = if is_tenor {
        format!(
            "https://tenor.googleapis.com/v2/search?q={}&key={}&limit={}&media_filter=gif",
            urlencoding::encode(&query.q),
            urlencoding::encode(&api_key),
            limit
        )
    } else {
        format!(
            "https://api.giphy.com/v1/gifs/search?q={}&api_key={}&limit={}",
            urlencoding::encode(&query.q),
            urlencoding::encode(&api_key),
            limit
        )
    };

    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => {
                // Normalize response to a uniform shape
                let results = if is_tenor {
                    body.get("results")
                        .and_then(|r| r.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|item| {
                                    let title = item.get("content_description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let url = item.pointer("/media_formats/gif/url")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let preview = item.pointer("/media_formats/tinygif/url")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(url);
                                    if url.is_empty() { return None; }
                                    Some(json!({ "title": title, "url": url, "preview": preview }))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    body.get("data")
                        .and_then(|d| d.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|item| {
                                    let title = item.get("title")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let url = item.pointer("/images/original/url")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let preview = item.pointer("/images/fixed_height_small/url")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(url);
                                    if url.is_empty() { return None; }
                                    Some(json!({ "title": title, "url": url, "preview": preview }))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                Ok(Json(json!({ "results": results })))
            }
            Err(_) => Ok(Json(json!({ "results": [] }))),
        },
        Err(_) => Ok(Json(json!({ "results": [] }))),
    }
}

async fn list_meetings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::ScheduledMeeting>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    Ok(Json(state.list_meetings_for_user(&principal)))
}

async fn create_meeting(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateScheduledMeetingRequest>,
) -> Result<Json<crate::ScheduledMeeting>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    state
        .authorize_external_participants(&principal, &input.participants)
        .map_err(ApiError::Conflict)?;
    let meeting = state
        .create_scheduled_meeting(&principal, input)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(
        &principal,
        "meeting.scheduled",
        Some(meeting.id.to_string()),
    );
    Ok(Json(meeting))
}

async fn update_meeting(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateScheduledMeetingRequest>,
) -> Result<Json<crate::ScheduledMeeting>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let meeting = state
        .update_scheduled_meeting(id, &principal, input)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "meeting.updated", Some(id.to_string()));
    Ok(Json(meeting))
}

async fn cancel_meeting(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::ScheduledMeeting>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let meeting = state
        .cancel_scheduled_meeting(id, &principal)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "meeting.cancelled", Some(id.to_string()));
    Ok(Json(meeting))
}

async fn export_meeting_ics(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let ics = state
        .meeting_ics(id, &principal)
        .ok_or(ApiError::NotFound)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"meeting.ics\""),
    );
    Ok((headers, ics).into_response())
}

async fn start_meeting(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::RoomCallTarget>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let target = state
        .start_scheduled_meeting(id, &principal)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "meeting.started", Some(id.to_string()));
    Ok(Json(target))
}

async fn list_retention_policies(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::RetentionPolicy>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.retention_policies()))
}

async fn upsert_retention_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<UpsertRetentionPolicyRequest>,
) -> Result<Json<crate::RetentionPolicy>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let policy = state.upsert_retention_policy(&principal, input);
    state.record_audit_event(
        &principal,
        "retention_policy.upserted",
        Some(policy.id.to_string()),
    );
    Ok(Json(policy))
}

async fn delete_retention_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.delete_retention_policy(id) {
        return Err(ApiError::NotFound);
    }
    state.record_audit_event(&principal, "retention_policy.deleted", Some(id.to_string()));
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn preview_retention_enforcement(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::RetentionEnforcementResult>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.enforce_retention(true)))
}

async fn apply_retention_enforcement(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::RetentionEnforcementResult>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let result = state.enforce_retention(false);
    state.record_audit_event(
        &principal,
        "retention.enforced",
        Some(format!(
            "deleted={},matched={}",
            result.deleted_messages, result.matched_messages
        )),
    );
    Ok(Json(result))
}

async fn get_collaboration_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<crate::CollaborationPolicy>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.collaboration_policy()))
}

async fn update_collaboration_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<UpdateCollaborationPolicyRequest>,
) -> Result<Json<crate::CollaborationPolicy>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let policy = state.update_collaboration_policy(&principal, input);
    state.record_audit_event(
        &principal,
        "collaboration_policy.updated",
        Some(policy.id.clone()),
    );
    Ok(Json(policy))
}

#[derive(serde::Deserialize)]
struct DiscoveryExportQuery {
    room_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
struct DiscoverySearchQueryParams {
    q: Option<String>,
    user_uri: Option<String>,
    room_id: Option<Uuid>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
    export: Option<bool>,
}

async fn discovery_export(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<DiscoveryExportQuery>,
) -> Result<Json<crate::DiscoveryExport>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state.record_audit_event(
        &principal,
        "ediscovery.exported",
        query.room_id.map(|id| id.to_string()),
    );
    Ok(Json(state.discovery_export(query.room_id)))
}

async fn discovery_search(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<DiscoverySearchQueryParams>,
) -> Result<Json<crate::DiscoveryExport>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let from = parse_discovery_time(query.from.as_deref())?;
    let to = parse_discovery_time(query.to.as_deref())?;
    let result = state.discovery_search(crate::DiscoverySearchQuery {
        q: query.q.clone(),
        user_uri: query.user_uri.clone(),
        room_id: query.room_id,
        from,
        to,
        limit: query.limit,
    });
    let count = result.messages.len() + result.files.len() + result.recordings.len();
    state.record_audit_event(
        &principal,
        if query.export.unwrap_or(false) {
            "ediscovery.exported"
        } else {
            "ediscovery.searched"
        },
        Some(format!("matches={count}")),
    );
    Ok(Json(result))
}

fn parse_discovery_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(value)
        .map(|date| Some(date.with_timezone(&Utc)))
        .map_err(|_| ApiError::Conflict(format!("invalid RFC3339 timestamp: {value}")))
}

// ─── SCIM Provisioning ───

#[derive(serde::Deserialize)]
struct ScimUserInput {
    #[serde(rename = "userName")]
    user_name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    roles: Vec<ScimRole>,
}

#[derive(serde::Deserialize)]
struct ScimRole {
    value: String,
}

fn scim_user(user: crate::User) -> serde_json::Value {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
        "id": user.id,
        "userName": user.sip_uri,
        "displayName": user.display_name,
        "active": user.active,
        "roles": [{"value": user.role}],
    })
}

async fn scim_list_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    authenticated_admin(&headers, &state)?;
    let resources: Vec<_> = state.all_users().into_iter().map(scim_user).collect();
    Ok(Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": resources.len(),
        "Resources": resources,
        "startIndex": 1,
        "itemsPerPage": resources.len(),
    })))
}

async fn scim_create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<ScimUserInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let role = input.roles.first().map(|role| role.value.clone());
    if let Some(existing) = state.user_by_sip_uri(&input.user_name) {
        let mut user = state
            .set_user_active(existing.id, input.active.unwrap_or(true), &principal)
            .ok_or(ApiError::NotFound)?;
        if let Some(role) = role {
            user = state
                .update_user_role(user.id, &role)
                .ok_or(ApiError::NotFound)?;
        }
        state.record_audit_event(&principal, "scim.user_upserted", Some(user.id.to_string()));
        return Ok(Json(scim_user(user)));
    }
    let user = state
        .create_user(CreateUserRequest {
            display_name: input
                .display_name
                .unwrap_or_else(|| input.user_name.clone()),
            sip_uri: input.user_name,
            matrix_user_id: None,
            password: None,
            role,
        })
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "scim.user_created", Some(user.id.to_string()));
    Ok(Json(scim_user(user)))
}

async fn scim_update_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ScimUserInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if input.active == Some(false) {
        let user = state
            .set_user_active(id, false, &principal)
            .ok_or(ApiError::NotFound)?;
        state.record_audit_event(&principal, "scim.user_deactivated", Some(id.to_string()));
        return Ok(Json(scim_user(user)));
    }
    if input.active == Some(true) {
        state
            .set_user_active(id, true, &principal)
            .ok_or(ApiError::NotFound)?;
    }
    if let Some(role) = input.roles.first() {
        state
            .update_user_role(id, &role.value)
            .ok_or(ApiError::NotFound)?;
    }
    let user = state
        .all_users()
        .into_iter()
        .find(|user| user.id == id)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "scim.user_updated", Some(id.to_string()));
    Ok(Json(scim_user(user)))
}

async fn scim_delete_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    state
        .set_user_active(id, false, &principal)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "scim.user_deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
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
    state
        .authorize_external_participants(&principal, &input.members)
        .map_err(ApiError::Conflict)?;
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

fn can_manage_room_connectors(
    state: &AppState,
    room_id: Uuid,
    principal: &str,
    is_admin: bool,
) -> bool {
    if is_admin {
        return true;
    }
    state.room(room_id).is_some_and(|room| {
        room.created_by == principal
            || room.channel_owners.iter().any(|owner| owner == principal)
            || room.members.iter().any(|member| {
                member.user_sip_uri == principal
                    && matches!(member.role.as_str(), "owner" | "admin")
            })
    })
}

async fn list_channel_webhooks(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::ChannelWebhookSummary>>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !can_manage_room_connectors(&state, id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(
        state
            .list_channel_webhooks(id)
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

async fn create_channel_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CreateChannelWebhookRequest>,
) -> Result<Json<crate::CreateChannelWebhookResponse>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !can_manage_room_connectors(&state, id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let response = state
        .create_channel_webhook(id, &principal, input)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(
        &principal,
        "channel_webhook.created",
        Some(format!("{}:{}", id, response.webhook.id)),
    );
    Ok(Json(response))
}

async fn update_channel_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, webhook_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<crate::UpdateChannelWebhookRequest>,
) -> Result<Json<crate::ChannelWebhookSummary>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !can_manage_room_connectors(&state, id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let webhook = state
        .set_channel_webhook_enabled(id, webhook_id, input.enabled)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        if input.enabled {
            "channel_webhook.enabled"
        } else {
            "channel_webhook.disabled"
        },
        Some(format!("{}:{}", id, webhook_id)),
    );
    Ok(Json(webhook.into()))
}

async fn delete_channel_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, webhook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<crate::ChannelWebhookSummary>, ApiError> {
    let (principal, role) = authenticated_principal_role(&headers, &state)?;
    if !can_manage_room_connectors(&state, id, &principal, role == crate::ROLE_ADMIN) {
        return Err(ApiError::Forbidden);
    }
    let webhook = state
        .delete_channel_webhook(id, webhook_id)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(
        &principal,
        "channel_webhook.deleted",
        Some(format!("{}:{}", id, webhook_id)),
    );
    Ok(Json(webhook.into()))
}

async fn post_channel_webhook(
    State(state): State<SharedState>,
    Path(token): Path<String>,
    Json(input): Json<crate::PostChannelWebhookRequest>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let message = state
        .post_channel_webhook(&token, input)
        .map_err(ApiError::Conflict)?;
    state.record_audit_event(
        &format!("webhook:{}", message.sender_uri),
        "channel_webhook.posted",
        Some(format!("{}:{}", message.room_id, message.id)),
    );
    Ok(Json(message))
}

async fn list_room_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<MessageQuery>,
) -> Result<Json<Vec<crate::RoomMessage>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    let mut messages = state.room_messages(id);

    if let Some(before) = &query.before {
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(before) {
            let ts = ts.with_timezone(&Utc);
            messages.retain(|m| m.created_at < ts);
        }
    }

    messages.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let limit = query.limit.unwrap_or(100).min(500);
    messages.truncate(limit);
    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    Ok(Json(messages))
}

async fn list_room_message_state(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::RoomMessageState>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    Ok(Json(state.room_message_state(id)))
}

async fn send_room_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SendRoomMessageRequest>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    state
        .send_room_message(id, &principal, &input.body, input.reply_to, input.priority)
        .map(Json)
        .map_err(ApiError::Conflict)
}

async fn schedule_room_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ScheduleRoomMessageRequest>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    state
        .schedule_room_message(
            id,
            &principal,
            &input.body,
            input.scheduled_at,
            input.reply_to,
            input.priority,
        )
        .map(Json)
        .map_err(ApiError::Conflict)
}

// ─── Notification Preferences ───

async fn get_notification_preference(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::NotificationPreference>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    Ok(Json(state.get_notification_preference(id, &principal)))
}

async fn set_notification_preference(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateNotificationPreferenceRequest>,
) -> Result<Json<crate::NotificationPreference>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    let pref = state.set_notification_preference(id, &principal, &input.notification_level);
    if let Some(pg) = state.pg_store() {
        let pref_for_pg = pref.clone();
        let pg = pg.clone();
        tokio::spawn(async move {
            if let Err(e) = pg.upsert_notification_preference(&pref_for_pg).await {
                log::warn!("Failed to persist notification preference: {}", e);
            }
        });
    }
    Ok(Json(pref))
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

#[derive(serde::Deserialize)]
struct StartRoomCallRequest {
    mode: Option<RoomCallMode>,
}

async fn start_room_call(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<StartRoomCallRequest>,
) -> Result<Json<crate::RoomCallTarget>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    let mode = input.mode.unwrap_or(RoomCallMode::Audio);
    let target = state
        .join_room_call(id, &principal, mode)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "room.call_started", Some(id.to_string()));
    Ok(Json(target))
}

async fn end_room_call(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::RoomCallEnded>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    require_room_member(&state, id, &principal)?;
    let ended = state.end_room_call(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "room.call_ended", Some(id.to_string()));
    Ok(Json(ended))
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

async fn search_collaboration(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<crate::CollaborationSearchResult>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let limit = query.limit.unwrap_or(25).min(100);
    Ok(Json(
        state.search_collaboration(&principal, &query.q, limit),
    ))
}

// ─── Read Receipts ───

async fn mark_message_read(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MessageRead>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let msg = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, msg.room_id, &principal)?;
    state
        .mark_room_message_read(id, &principal)
        .map(Json)
        .ok_or(ApiError::NotFound)
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
    let msg = state
        .edit_room_message(id, &principal, &input.body)
        .map_err(ApiError::Conflict)?;
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
    state.delete_room_message(id).ok_or(ApiError::NotFound)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "message_deleted".to_string(),
        payload: json!({
            "message_id": id,
            "room_id": msg.room_id,
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
    let toggle = state
        .toggle_message_reaction(id, &principal, &input.emoji)
        .ok_or(ApiError::NotFound)?;
    Ok(Json(
        serde_json::to_value(toggle).unwrap_or_else(|_| json!({ "ok": true })),
    ))
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
    Ok(Json(msg))
}

#[derive(serde::Deserialize)]
struct SaveMessageRequest {
    saved: bool,
}

async fn save_message_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SaveMessageRequest>,
) -> Result<Json<crate::RoomMessage>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let existing = state.room_message(id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, existing.room_id, &principal)?;
    let msg = state
        .set_message_saved(id, &principal, input.saved)
        .ok_or(ApiError::NotFound)?;
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
) -> Result<Json<Vec<crate::MessageRead>>, ApiError> {
    let principal = authenticated_principal(&headers, &state)?;
    let msg = state.room_message(_id).ok_or(ApiError::NotFound)?;
    require_room_member(&state, msg.room_id, &principal)?;
    Ok(Json(state.message_reads(_id)))
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
        .update_user_profile(
            id,
            input.email.clone(),
            input.title.clone(),
            input.department.clone(),
            input.phone_number.clone(),
        )
        .ok_or(ApiError::NotFound)?;
    let email = input.email;
    let title = input.title;
    let dept = input.department;
    let phone = input.phone_number;
    state.pg_spawn(move |pg| {
        Box::pin(async move { pg.update_user_profile(id, email, title, dept, phone).await })
    });
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
        dlp_status: "clean".to_string(),
        dlp_violation_count: 0,
        legal_hold: state.file_on_legal_hold(),
        deleted_at: None,
        deleted_by: None,
        folder_id: None,
        locked_by: None,
        locked_at: None,
    };
    state.put_file_record(record);
    state.record_audit_event(&principal, "user.avatar_updated", Some(user_id.to_string()));

    Ok(Json(json!({
        "file_id": file_id,
        "url": format!("/v1/files/{}", file_id),
    })))
}

// ─── Call Center ───

async fn list_agents(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::AgentProfile>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_agent_profiles()))
}
async fn create_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateAgentProfileRequest>,
) -> Result<Json<crate::AgentProfile>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    let a = state
        .create_agent_profile(input)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&p, "agent.created", Some(a.user_sip_uri.clone()));
    Ok(Json(a))
}
async fn get_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
) -> Result<Json<crate::AgentProfile>, ApiError> {
    require_bearer(&headers, &state)?;
    let full = if uri.starts_with("sip:") {
        uri
    } else {
        format!("sip:{}", uri)
    };
    state
        .agent_profile(&full)
        .map(Json)
        .ok_or(ApiError::NotFound)
}
async fn delete_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    let full = if uri.starts_with("sip:") {
        uri
    } else {
        format!("sip:{}", uri)
    };
    state
        .delete_agent_profile(&full)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "agent.deleted", Some(full));
    Ok(Json(json!({"ok":true})))
}
async fn set_agent_state(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
    Json(input): Json<SetAgentStateRequest>,
) -> Result<Json<crate::AgentProfile>, ApiError> {
    let _p = authenticated_principal(&headers, &state)?;
    let full = if uri.starts_with("sip:") {
        uri
    } else {
        format!("sip:{}", uri)
    };
    let agent = state
        .transition_agent_state(&full, &input.state, input.reason)
        .map_err(|e| ApiError::Conflict(e))?;
    Ok(Json(agent))
}

async fn transition_agent_state_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
    Json(input): Json<AgentTransitionRequest>,
) -> Result<Json<crate::AgentProfile>, ApiError> {
    let _p = authenticated_principal(&headers, &state)?;
    let full = if uri.starts_with("sip:") {
        uri
    } else {
        format!("sip:{}", uri)
    };
    let agent = state
        .transition_agent_state(&full, &input.state, input.reason)
        .map_err(|e| ApiError::Conflict(e))?;
    // Start wrap-up timer if transitioning to wrap_up
    if input.state == "wrap_up" {
        // Find wrap_up_time from agent's queues
        let wrap_secs = agent
            .queues
            .iter()
            .filter_map(|qid| state.queue(*qid))
            .map(|q| q.wrap_up_time)
            .max()
            .unwrap_or(10);
        crate::start_wrap_up_timer(state.clone(), full, wrap_secs);
    }
    Ok(Json(agent))
}

async fn agent_state_history(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(uri): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_bearer(&headers, &state)?;
    let full = if uri.starts_with("sip:") {
        uri
    } else {
        format!("sip:{}", uri)
    };
    let pg = state.pg_store().ok_or(ApiError::NotFound)?;
    let history = pg
        .list_agent_state_log(&full, 100)
        .await
        .map_err(|_| ApiError::NotFound)?;
    Ok(Json(history))
}

async fn list_queue_callers(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::QueueCallerEntry>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.queue_callers_waiting(id)))
}

async fn request_queue_callback(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<RequestCallbackInput>,
) -> Result<Json<crate::QueueCallback>, ApiError> {
    require_bearer(&headers, &state)?;
    state.queue(id).ok_or(ApiError::NotFound)?;
    Ok(Json(state.request_callback(id, input)))
}

async fn list_queue_callbacks(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::QueueCallback>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_queue_callbacks(id)))
}

async fn list_vip_callers(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::VipCaller>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_vip_callers()))
}

async fn create_vip_caller(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateVipCallerRequest>,
) -> Result<Json<crate::VipCaller>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let vip = state.create_vip_caller(input);
    state.record_audit_event(&p, "vip_caller.created", Some(vip.caller_pattern.clone()));
    Ok(Json(vip))
}

async fn delete_vip_caller(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_vip_caller(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "vip_caller.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok": true})))
}

async fn get_wallboard(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    authenticated_admin(&headers, &state)?;
    let metrics = state.queue_wallboard();
    let agents = state.list_agent_profiles();
    let available = agents.iter().filter(|a| a.state == "available").count();
    let on_call = agents.iter().filter(|a| a.state == "on_call").count();
    let wrap_up = agents.iter().filter(|a| a.state == "wrap_up").count();
    let on_break = agents
        .iter()
        .filter(|a| a.state == "break" || a.state == "training" || a.state == "meeting")
        .count();
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

async fn list_monitors(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::MonitorSession>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_monitor_sessions()))
}
async fn start_monitor(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<StartMonitorRequest>,
) -> Result<Json<crate::MonitorSession>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let session = state.start_monitor(&p, input);
    state.record_audit_event(
        &p,
        "monitor.started",
        Some(format!("{}:{}", session.mode, session.target_call_id)),
    );
    Ok(Json(session))
}
async fn end_monitor(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.end_monitor(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "monitor.ended", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
}

async fn list_scorecards(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::QaScorecard>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_scorecards()))
}
async fn create_scorecard(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateScorecardRequest>,
) -> Result<Json<crate::QaScorecard>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let sc = state.create_scorecard(&p, input);
    state.record_audit_event(&p, "scorecard.created", Some(sc.id.to_string()));
    Ok(Json(sc))
}

async fn list_canned(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CannedResponse>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_canned_responses()))
}
async fn create_canned(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateCannedResponseRequest>,
) -> Result<Json<crate::CannedResponse>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let cr = state.create_canned_response(input);
    state.record_audit_event(&p, "canned_response.created", Some(cr.id.to_string()));
    Ok(Json(cr))
}
async fn delete_canned(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_canned_response(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "canned_response.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
}

// ─── PBX Features ───

async fn list_queues(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::CallQueue>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_queues()))
}
async fn create_queue(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateQueueRequest>,
) -> Result<Json<crate::CallQueue>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let q = state
        .create_queue(input)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&p, "queue.created", Some(q.id.to_string()));
    Ok(Json(q))
}
async fn get_queue(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::CallQueue>, ApiError> {
    require_bearer(&headers, &state)?;
    state.queue(id).map(Json).ok_or(ApiError::NotFound)
}
async fn delete_queue(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_queue(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "queue.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
}

#[derive(serde::Deserialize)]
struct ListExtensionsQuery {
    unassigned: Option<bool>,
}

async fn list_extensions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListExtensionsQuery>,
) -> Result<Json<Vec<crate::Extension>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(
        state.list_extensions_filtered(q.unassigned.unwrap_or(false)),
    ))
}
async fn create_extension(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateExtensionRequest>,
) -> Result<Json<crate::Extension>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let e = state
        .create_extension(input)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&p, "extension.created", Some(e.extension.clone()));
    Ok(Json(e))
}
async fn delete_extension(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(ext): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_extension(&ext).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "extension.deleted", Some(ext));
    Ok(Json(json!({"ok":true})))
}

async fn list_dids(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Extension>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_dids()))
}

async fn create_did(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateDidRequest>,
) -> Result<Json<crate::Extension>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let did = state.create_did(input).map_err(ApiError::Conflict)?;
    state.record_audit_event(&principal, "did.created", Some(did.extension.clone()));
    Ok(Json(did))
}

async fn delete_did(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(did): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let extension = state.delete_extension(&did).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "did.deleted", Some(extension.extension));
    Ok(Json(json!({"ok": true})))
}

async fn provision_user_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<ProvisionUserRequest>,
) -> Result<Json<crate::ProvisionUserResponse>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let response = state
        .provision_user(input)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(
        &principal,
        "user.provisioned",
        Some(response.user.id.to_string()),
    );
    Ok(Json(response))
}

async fn assign_extension_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(ext): Path<String>,
    Json(input): Json<AssignExtensionRequest>,
) -> Result<Json<crate::Extension>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let extension = state
        .assign_extension(&ext, input.user_id)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(
        &principal,
        "extension.assigned",
        Some(format!("{}:{}", ext, input.user_id)),
    );
    Ok(Json(extension))
}

async fn unassign_extension_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(ext): Path<String>,
) -> Result<Json<crate::Extension>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    let extension = state
        .unassign_extension(&ext)
        .map_err(|e| ApiError::Conflict(e))?;
    state.record_audit_event(&principal, "extension.unassigned", Some(ext));
    Ok(Json(extension))
}

async fn list_business_hours(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::BusinessHours>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_business_hours()))
}
async fn create_business_hours(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateBusinessHoursRequest>,
) -> Result<Json<crate::BusinessHours>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let bh = state.create_business_hours(input);
    state.record_audit_event(&p, "business_hours.created", Some(bh.id.to_string()));
    Ok(Json(bh))
}
async fn delete_business_hours_entry(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_business_hours(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "business_hours.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
}

async fn list_holidays(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::Holiday>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_holidays()))
}
async fn create_holiday(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateHolidayRequest>,
) -> Result<Json<crate::Holiday>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let h = state.create_holiday(input);
    state.record_audit_event(&p, "holiday.created", Some(h.id.to_string()));
    Ok(Json(h))
}
async fn delete_holiday(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_holiday(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "holiday.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
}

#[derive(serde::Deserialize)]
struct ParkRequest {
    call_id: String,
    caller_uri: String,
    caller_name: Option<String>,
    slot: String,
}
async fn park_call(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<ParkRequest>,
) -> Result<Json<crate::ParkedCall>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    Ok(Json(state.park_call(
        &input.slot,
        &input.call_id,
        &p,
        &input.caller_uri,
        input.caller_name.as_deref().unwrap_or(""),
    )))
}
async fn pickup_call(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(slot): Path<String>,
) -> Result<Json<crate::ParkedCall>, ApiError> {
    require_bearer(&headers, &state)?;
    state
        .pickup_parked_call(&slot)
        .map(Json)
        .ok_or(ApiError::NotFound)
}
async fn list_parked(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::ParkedCall>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_parked_calls()))
}

async fn list_speed_dials(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::SpeedDial>>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    Ok(Json(state.speed_dials_for_user(&p)))
}
async fn create_speed_dial(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateSpeedDialRequest>,
) -> Result<Json<crate::SpeedDial>, ApiError> {
    let p = authenticated_principal(&headers, &state)?;
    Ok(Json(state.set_speed_dial(Some(&p), input)))
}

#[derive(serde::Deserialize)]
struct CdrQuery {
    limit: Option<usize>,
}
async fn list_cdrs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<CdrQuery>,
) -> Result<Json<Vec<crate::CallDetailRecord>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.list_cdrs(q.limit.unwrap_or(100))))
}

async fn list_paging_groups(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::PagingGroup>>, ApiError> {
    require_bearer(&headers, &state)?;
    Ok(Json(state.list_paging_groups()))
}
async fn create_paging_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreatePagingGroupRequest>,
) -> Result<Json<crate::PagingGroup>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    let pg = state.create_paging_group(input);
    state.record_audit_event(&p, "paging_group.created", Some(pg.id.to_string()));
    Ok(Json(pg))
}
async fn delete_paging_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = authenticated_admin(&headers, &state)?;
    state.delete_paging_group(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&p, "paging_group.deleted", Some(id.to_string()));
    Ok(Json(json!({"ok":true})))
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
    let uri = if sip_uri.starts_with("sip:") {
        sip_uri
    } else {
        format!("sip:{}", sip_uri)
    };
    Ok(Json(state.get_user_call_settings(&uri)))
}

async fn update_user_call_settings_admin(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(sip_uri): Path<String>,
    Json(settings): Json<crate::UserCallSettings>,
) -> Result<Json<crate::UserCallSettings>, ApiError> {
    let principal = authenticated_admin(&headers, &state)?;
    if !state.is_admin_principal(&principal) {
        return Err(ApiError::Forbidden);
    }
    let uri = if sip_uri.starts_with("sip:") {
        sip_uri
    } else {
        format!("sip:{}", sip_uri)
    };
    let mut settings = settings;
    settings.user_sip_uri = uri.clone();
    state.set_user_call_settings(settings);
    let updated = state.get_user_call_settings(&uri);
    state.record_audit_event(&principal, "call_settings.updated", Some(uri));
    Ok(Json(updated))
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
        Ok(_) => Ok(Json(
            json!({ "ok": true, "message": "Connection successful" }),
        )),
        Err(e) => {
            if e.contains("connection failed") || e.contains("bind failed") {
                Ok(Json(json!({ "ok": false, "message": e })))
            } else {
                // Connection works, auth failed (expected for test user)
                Ok(Json(
                    json!({ "ok": true, "message": "Connection successful (test auth rejected as expected)" }),
                ))
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
    let group = state
        .create_ring_group(input)
        .map_err(|e| ApiError::Conflict(e))?;
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
    let full_uri = if uri.starts_with("sip:") {
        uri
    } else {
        format!("sip:{}", uri)
    };
    Ok(Json(state.resolve_inbound_route(&full_uri)))
}

#[derive(serde::Deserialize)]
struct RoutePreviewQuery {
    direction: Option<String>,
    source: Option<String>,
    destination: String,
    method: Option<String>,
    headers: Option<String>,
}

#[derive(serde::Deserialize)]
struct PreviewHeader {
    name: String,
    value: String,
}

async fn preview_route(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<RoutePreviewQuery>,
) -> Result<Json<crate::RoutePreview>, ApiError> {
    authenticated_admin(&headers, &state)?;
    let preview_headers = query
        .headers
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<PreviewHeader>>(raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|header| (header.name, header.value))
        .collect::<Vec<_>>();
    Ok(Json(state.preview_route(
        query.direction.as_deref().unwrap_or("inbound"),
        query.source.as_deref().unwrap_or("*"),
        &query.destination,
        query.method.as_deref().unwrap_or("INVITE"),
        &preview_headers,
    )))
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
    state
        .mark_voicemail_listened(id)
        .map(Json)
        .ok_or(ApiError::NotFound)
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
    state
        .delete_recording(id, &principal)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&principal, "recording.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── File Versioning ───

async fn list_file_versions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::FileVersion>>, ApiError> {
    authenticated_principal(&headers, &state)?;
    let versions = state.file_versions(id);
    Ok(Json(versions))
}

async fn download_file_version(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, version)): Path<(Uuid, i32)>,
) -> Result<Response, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let record = state.file_record(id).ok_or(ApiError::NotFound)?;
    if !state.is_admin_principal(&requester) && requester != record.owner {
        return Err(ApiError::Forbidden);
    }
    let versions = state.file_versions(id);
    let ver = versions
        .iter()
        .find(|v| v.version_number == version)
        .ok_or(ApiError::NotFound)?;
    let bytes = tokio::fs::read(&ver.storage_path).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&record.content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    let disposition = format!(
        "attachment; filename=\"v{}_{}\"\r\n",
        version,
        record.filename.replace('"', "")
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(disposition.trim())
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    Ok((headers, bytes).into_response())
}

// ─── File Lock ───

async fn lock_file_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::FileRecord>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let record = state
        .lock_file(id, &requester)
        .ok_or(ApiError::Conflict("file is already locked".to_string()))?;
    state.persist(&record);
    state.record_audit_event(&requester, "file.locked", Some(id.to_string()));
    Ok(Json(record))
}

async fn unlock_file_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::FileRecord>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    // Admins can force-unlock
    let record = if state.is_admin_principal(&requester) {
        state
            .files
            .with_write(&id, |files| {
                let file = files.get_mut(&id)?;
                file.locked_by = None;
                file.locked_at = None;
                Some(file.clone())
            })
            .ok_or(ApiError::NotFound)?
    } else {
        state
            .unlock_file(id, &requester)
            .ok_or(ApiError::Conflict(
                "file not locked by you".to_string(),
            ))?
    };
    state.persist(&record);
    state.record_audit_event(&requester, "file.unlocked", Some(id.to_string()));
    Ok(Json(record))
}

// ─── Folders ───

async fn list_folders(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<crate::Folder>>, ApiError> {
    authenticated_principal(&headers, &state)?;
    let parent_id = params
        .get("parent_id")
        .and_then(|s| Uuid::parse_str(s).ok());
    let folders = state.folders_for_room(id, parent_id);
    Ok(Json(folders))
}

async fn create_folder(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CreateFolderRequest>,
) -> Result<Json<crate::Folder>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let folder = crate::Folder {
        id: Uuid::new_v4(),
        room_id: id,
        parent_id: input.parent_id,
        name: input.name,
        created_by: requester.clone(),
        created_at: Utc::now(),
    };
    state.put_folder(folder.clone());
    state.record_audit_event(&requester, "folder.created", Some(folder.id.to_string()));
    Ok(Json(folder))
}

async fn delete_folder(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    state.delete_folder(id).ok_or(ApiError::NotFound)?;
    state.record_audit_event(&requester, "folder.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Approvals ───

async fn list_approvals(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::ApprovalRequest>>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let approvals: Vec<_> = state
        .approvals()
        .into_iter()
        .filter(|a| {
            state.is_admin_principal(&requester)
                || a.requestor == requester
                || a.approvers.contains(&requester)
        })
        .collect();
    Ok(Json(approvals))
}

async fn create_approval(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateApprovalRequest>,
) -> Result<Json<crate::ApprovalRequest>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let approval = crate::ApprovalRequest {
        id: Uuid::new_v4(),
        title: input.title,
        description: input.description.unwrap_or_default(),
        requestor: requester.clone(),
        approvers: input.approvers,
        status: "pending".to_string(),
        responses: serde_json::json!([]),
        room_id: input.room_id,
        created_at: Utc::now(),
        resolved_at: None,
    };
    state.put_approval(approval.clone());
    state.broadcast_sse(crate::SseEvent {
        event_type: "approval_created".to_string(),
        payload: serde_json::to_value(&approval).unwrap_or_default(),
    });
    state.record_audit_event(&requester, "approval.created", Some(approval.id.to_string()));
    Ok(Json(approval))
}

async fn respond_to_approval(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::ApprovalResponseInput>,
) -> Result<Json<crate::ApprovalRequest>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let approval = state
        .update_approval(id, |a| {
            let response = serde_json::json!({
                "user": requester,
                "decision": input.decision,
                "comment": input.comment,
                "responded_at": Utc::now().to_rfc3339(),
            });
            let mut responses = a.responses.as_array().cloned().unwrap_or_default();
            responses.push(response);
            a.responses = serde_json::Value::Array(responses.clone());

            // Check if all approvers have responded
            let approver_count = a.approvers.len();
            if responses.len() >= approver_count {
                let all_approved = responses.iter().all(|r| {
                    r.get("decision")
                        .and_then(|d| d.as_str())
                        .map(|d| d == "approve")
                        .unwrap_or(false)
                });
                a.status = if all_approved {
                    "approved".to_string()
                } else {
                    "rejected".to_string()
                };
                a.resolved_at = Some(Utc::now());
            }
        })
        .ok_or(ApiError::NotFound)?;
    state.broadcast_sse(crate::SseEvent {
        event_type: "approval_response".to_string(),
        payload: serde_json::to_value(&approval).unwrap_or_default(),
    });
    Ok(Json(approval))
}

// ─── Recording Policies ───

async fn list_recording_policies(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::RecordingPolicy>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.recording_policies_list()))
}

async fn create_recording_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreateRecordingPolicyRequest>,
) -> Result<Json<crate::RecordingPolicy>, ApiError> {
    let admin = authenticated_admin(&headers, &state)?;
    let policy = crate::RecordingPolicy {
        id: Uuid::new_v4(),
        name: input.name,
        trigger: input.trigger,
        target_ids: input.target_ids.unwrap_or_default(),
        enabled: input.enabled.unwrap_or(true),
        created_at: Utc::now(),
    };
    state.put_recording_policy(policy.clone());
    state.record_audit_event(&admin, "recording_policy.created", Some(policy.id.to_string()));
    Ok(Json(policy))
}

async fn update_recording_policy(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CreateRecordingPolicyRequest>,
) -> Result<Json<crate::RecordingPolicy>, ApiError> {
    let admin = authenticated_admin(&headers, &state)?;
    let _existing = state.recording_policy(id).ok_or(ApiError::NotFound)?;
    let policy = crate::RecordingPolicy {
        id,
        name: input.name,
        trigger: input.trigger,
        target_ids: input.target_ids.unwrap_or_default(),
        enabled: input.enabled.unwrap_or(true),
        created_at: _existing.created_at,
    };
    state.put_recording_policy(policy.clone());
    state.record_audit_event(&admin, "recording_policy.updated", Some(id.to_string()));
    Ok(Json(policy))
}

async fn delete_recording_policy_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let admin = authenticated_admin(&headers, &state)?;
    state
        .delete_recording_policy(id)
        .ok_or(ApiError::NotFound)?;
    state.record_audit_event(&admin, "recording_policy.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Hold Music ───

async fn list_hold_music(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::HoldMusic>>, ApiError> {
    authenticated_admin(&headers, &state)?;
    Ok(Json(state.hold_music_list()))
}

async fn upload_hold_music(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<crate::HoldMusic>, ApiError> {
    let admin = authenticated_admin(&headers, &state)?;
    let name =
        header_string(&headers, "x-pale-filename").unwrap_or_else(|| "hold_music".to_string());
    let queue_id = header_string(&headers, "x-pale-queue-id")
        .and_then(|s| Uuid::parse_str(&s).ok());
    let is_default = header_string(&headers, "x-pale-default")
        .map(|s| s == "true")
        .unwrap_or(false);

    tokio::fs::create_dir_all(state.files_dir()).await?;
    let id = Uuid::new_v4();
    let file_path = state.files_dir().join(format!("hold_music_{}", id));
    tokio::fs::write(&file_path, &body).await?;

    let music = crate::HoldMusic {
        id,
        name: safe_filename(&name),
        file_path: file_path.to_string_lossy().to_string(),
        queue_id,
        is_default,
        uploaded_by: admin.clone(),
        created_at: Utc::now(),
    };
    state.put_hold_music(music.clone());
    state.record_audit_event(&admin, "hold_music.uploaded", Some(id.to_string()));
    Ok(Json(music))
}

async fn delete_hold_music_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let admin = authenticated_admin(&headers, &state)?;
    let music = state.delete_hold_music(id).ok_or(ApiError::NotFound)?;
    // Try to delete the file
    let _ = tokio::fs::remove_file(&music.file_path).await;
    state.record_audit_event(&admin, "hold_music.deleted", Some(id.to_string()));
    Ok(Json(json!({ "ok": true })))
}

// ─── Personal Call Groups ───

async fn list_call_groups(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::PersonalCallGroup>>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    Ok(Json(state.personal_call_groups_for_user(&requester)))
}

async fn create_call_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<crate::CreatePersonalCallGroupRequest>,
) -> Result<Json<crate::PersonalCallGroup>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let group = crate::PersonalCallGroup {
        id: Uuid::new_v4(),
        user_id: requester.clone(),
        name: input.name,
        numbers: input.numbers,
        ring_duration: input.ring_duration.unwrap_or(20),
        enabled: input.enabled.unwrap_or(true),
    };
    state.put_personal_call_group(group.clone());
    state.record_audit_event(&requester, "call_group.created", Some(group.id.to_string()));
    Ok(Json(group))
}

async fn update_call_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::CreatePersonalCallGroupRequest>,
) -> Result<Json<crate::PersonalCallGroup>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let existing = state
        .personal_call_group(id)
        .ok_or(ApiError::NotFound)?;
    if existing.user_id != requester {
        return Err(ApiError::Forbidden);
    }
    let group = crate::PersonalCallGroup {
        id,
        user_id: requester.clone(),
        name: input.name,
        numbers: input.numbers,
        ring_duration: input.ring_duration.unwrap_or(existing.ring_duration),
        enabled: input.enabled.unwrap_or(existing.enabled),
    };
    state.put_personal_call_group(group.clone());
    Ok(Json(group))
}

async fn delete_call_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let existing = state
        .personal_call_group(id)
        .ok_or(ApiError::NotFound)?;
    if existing.user_id != requester && !state.is_admin_principal(&requester) {
        return Err(ApiError::Forbidden);
    }
    state
        .delete_personal_call_group(id)
        .ok_or(ApiError::NotFound)?;
    Ok(Json(json!({ "ok": true })))
}

// ─── Per-User Call Analytics ───

async fn user_call_analytics(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let requester = authenticated_principal(&headers, &state)?;
    let user = state
        .all_users()
        .into_iter()
        .find(|u| u.id == id)
        .ok_or(ApiError::NotFound)?;
    // Users can only view own analytics unless admin
    if !state.is_admin_principal(&requester) && user.sip_uri != requester {
        return Err(ApiError::Forbidden);
    }
    let analytics = state.user_call_analytics(&user.sip_uri);
    Ok(Json(analytics))
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
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<SseResponseEvent, Infallible>>>, ApiError> {
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

fn require_room_member(state: &AppState, room_id: Uuid, principal: &str) -> Result<(), ApiError> {
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
                    member.get("user_sip_uri").and_then(|uri| uri.as_str()) == Some(principal)
                })
            }),
        "room_message" | "typing" | "room_call_started" | "room_call_ended"
        | "scheduled_message_delivered" => event
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
        "meeting_scheduled" => {
            let is_organizer = event
                .payload
                .get("organizer_uri")
                .and_then(|uri| uri.as_str())
                == Some(principal);
            let is_participant = event
                .payload
                .get("participants")
                .and_then(|participants| participants.as_array())
                .is_some_and(|participants| {
                    participants
                        .iter()
                        .any(|participant| participant.as_str() == Some(principal))
                });
            let is_room_member = event
                .payload
                .get("room_id")
                .and_then(|id| id.as_str())
                .and_then(|id| Uuid::parse_str(id).ok())
                .is_some_and(|room_id| room_member(state, room_id, principal));
            is_organizer || is_participant || is_room_member
        }
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

    // Reject mfa_pending tokens — user must complete TOTP challenge first
    if role == "mfa_pending" {
        return Err(ApiError::Unauthorized);
    }

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
        .and_then(|value| {
            value
                .split(',')
                .next()
                .map(str::trim)
                .map(ToOwned::to_owned)
        })
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
            if let Ok(byte) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
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
        assert!(state
            .list_ring_groups()
            .iter()
            .any(|group| group.extension == "700"));
        assert!(state
            .list_queues()
            .iter()
            .any(|queue| queue.extension == "710"));
        assert!(state
            .list_paging_groups()
            .iter()
            .any(|group| group.extension == "720"));
        assert!(state
            .list_conferences()
            .iter()
            .any(|conference| conference.title == "Daily Standup"));
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
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let message = state
            .send_room_message(room.id, "sip:alice@example.com", "hello", None, None)
            .unwrap();

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
        assert!(!event_visible_to(
            &state,
            &room_event,
            "sip:mallory@example.com"
        ));

        let reaction_event = crate::SseEvent {
            event_type: "reaction".to_string(),
            payload: serde_json::json!({ "message_id": message.id }),
        };
        assert!(event_visible_to(
            &state,
            &reaction_event,
            "sip:alice@example.com"
        ));
        assert!(!event_visible_to(
            &state,
            &reaction_event,
            "sip:mallory@example.com"
        ));

        let call_event = crate::SseEvent {
            event_type: "room_call_started".to_string(),
            payload: serde_json::json!({ "room_id": room.id }),
        };
        assert!(event_visible_to(&state, &call_event, "sip:bob@example.com"));
        assert!(!event_visible_to(
            &state,
            &call_event,
            "sip:mallory@example.com"
        ));

        let call_ended_event = crate::SseEvent {
            event_type: "room_call_ended".to_string(),
            payload: serde_json::json!({ "room_id": room.id }),
        };
        assert!(event_visible_to(
            &state,
            &call_ended_event,
            "sip:bob@example.com"
        ));
        assert!(!event_visible_to(
            &state,
            &call_ended_event,
            "sip:mallory@example.com"
        ));

        let meeting_event = crate::SseEvent {
            event_type: "meeting_scheduled".to_string(),
            payload: serde_json::json!({
                "organizer_uri": "sip:alice@example.com",
                "participants": ["sip:bob@example.com"],
            }),
        };
        assert!(event_visible_to(
            &state,
            &meeting_event,
            "sip:alice@example.com"
        ));
        assert!(event_visible_to(
            &state,
            &meeting_event,
            "sip:bob@example.com"
        ));
        assert!(!event_visible_to(
            &state,
            &meeting_event,
            "sip:mallory@example.com"
        ));
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
