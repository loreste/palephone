import { paleFetch } from "./tauri";

const DEFAULT_BASE_URL = "http://127.0.0.1:8080";

export interface AdminSession {
  token: string;
  principal: string;
  expires_at: string;
}

export interface AdminUser {
  id: string;
  display_name: string;
  sip_uri: string;
  matrix_user_id?: string | null;
  created_at: string;
  active: boolean;
  deactivated_at?: string | null;
  deactivated_by?: string | null;
  role?: string;
}

export interface AdminSipAccount {
  username: string;
  domain: string;
  display_name?: string | null;
  enabled: boolean;
  created_at: string;
}

export interface AdminRegistration {
  aor: string;
  contact: string;
  source: string;
  user_agent?: string | null;
  expires_at: string;
  updated_at: string;
}

export interface AdminDialog {
  call_id: string;
  from_uri: string;
  to_uri: string;
  target_contact?: string | null;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface AdminConference {
  id: string;
  title: string;
  mode: "audio" | "video" | "webinar";
  participants: Array<{
    user_id: string;
    sip_uri: string;
    role: "host" | "moderator" | "member";
    joined_at: string;
  }>;
  created_at: string;
}

export interface AdminCall {
  id: string;
  conference_id?: string | null;
  caller: string;
  callees: string[];
  media: string[];
  status: string;
  created_at: string;
  updated_at: string;
}

export interface AdminMediaConfig {
  ice_enabled: boolean;
  stun_servers: string[];
  stun_ignore_failure: boolean;
  turn?: {
    server: string;
    transport: "udp" | "tcp" | "tls";
    username?: string | null;
    realm?: string | null;
  } | null;
}

export interface AdminFile {
  id: string;
  owner: string;
  filename: string;
  content_type: string;
  size: number;
  sha256: string;
  created_at: string;
}

export interface RoutingRule {
  id: string;
  name: string;
  source_pattern: string;
  destination_pattern: string;
  target: string;
  destination_type: string;
  method_pattern: string;
  header_conditions: Array<{ name: string; pattern: string; negate?: boolean }>;
  header_actions: Array<{ kind: "add" | "set" | "remove"; name: string; value?: string }>;
  stop_processing: boolean;
  priority: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface AdminAuditEvent {
  id: string;
  principal: string;
  action: string;
  target?: string | null;
  created_at: string;
}

export interface AdminSubscription {
  subscription_id: string;
  subscriber: string;
  target: string;
  event: string;
  expires_at: string;
  created_at: string;
  updated_at: string;
}

export interface AdminNotification {
  id: string;
  subscription_id?: string | null;
  notifier: string;
  target: string;
  event?: string | null;
  subscription_state?: string | null;
  content_type: string;
  body: string;
  received_at: string;
}

export interface AdminSnapshot {
  users: AdminUser[];
  sipAccounts: AdminSipAccount[];
  registrations: AdminRegistration[];
  dialogs: AdminDialog[];
  conferences: AdminConference[];
  calls: AdminCall[];
  mediaConfig: AdminMediaConfig;
  files: AdminFile[];
  routingRules: RoutingRule[];
  auditEvents: AdminAuditEvent[];
  subscriptions: AdminSubscription[];
  notifications: AdminNotification[];
  presence: AdminPresence[];
}

export interface SecurityPostureControl {
  id: string;
  category: string;
  title: string;
  status: "pass" | "warning" | "fail";
  score: number;
  max_score: number;
  summary: string;
  remediation: string;
}

export interface SecurityPostureRecommendation {
  control_id: string;
  priority: "high" | "medium" | "low";
  title: string;
  action: string;
}

export interface SecurityPostureReport {
  score: number;
  max_score: number;
  posture: "strong" | "moderate" | "needs_attention";
  generated_at: string;
  controls: SecurityPostureControl[];
  recommendations: SecurityPostureRecommendation[];
  counts: {
    active_users: number;
    mfa_enabled_users: number;
    enabled_sso_providers: number;
    enabled_conditional_access_policies: number;
    enabled_dlp_policies: number;
    retention_policies: number;
    legal_hold_policies: number;
    enabled_information_barriers: number;
    sensitivity_labels: number;
    encryption_keys: number;
    enabled_data_residency_regions: number;
    audit_events: number;
    pending_compliance_reviews: number;
  };
}

export interface CreateUserInput {
  display_name: string;
  sip_uri: string;
  matrix_user_id?: string | null;
  password?: string;
  role?: string;
}

export interface CreateSipAccountInput {
  username: string;
  domain: string;
  password: string;
  display_name?: string | null;
}

export interface CreateRoutingRuleInput {
  name: string;
  source_pattern: string;
  destination_pattern: string;
  target: string;
  destination_type?: string;
  method_pattern?: string;
  header_conditions?: Array<{ name: string; pattern: string; negate?: boolean }>;
  header_actions?: Array<{ kind: "add" | "set" | "remove"; name: string; value?: string }>;
  stop_processing?: boolean;
  priority: number;
  enabled: boolean;
}

export interface CreateConferenceInput {
  title: string;
  mode: "audio" | "video" | "webinar";
}

export interface JoinConferenceInput {
  user_id: string;
  sip_uri: string;
  role?: "host" | "moderator" | "member";
}

export interface AdminPresence {
  sip_uri: string;
  status: "online" | "offline" | "busy" | "away" | "dnd";
  note?: string | null;
  updated_at: string;
}

export function adminBaseUrl() {
  return import.meta.env.VITE_PALE_SERVER_URL || DEFAULT_BASE_URL;
}

export async function adminLogout(baseUrl: string, token: string): Promise<void> {
  await request(baseUrl, "/v1/admin/logout", {
    method: "POST",
    headers: authHeaders(token),
  });
}

export async function adminRefreshToken(baseUrl: string, token: string): Promise<AdminSession> {
  return request<AdminSession>(baseUrl, "/v1/admin/refresh", {
    method: "POST",
    headers: authHeaders(token),
  });
}

export async function adminLogin(
  baseUrl: string,
  username: string,
  password: string
): Promise<AdminSession> {
  return request<AdminSession>(baseUrl, "/v1/admin/login", {
    method: "POST",
    body: JSON.stringify({ username, password }),
  });
}

export async function loadAdminSnapshot(baseUrl: string, token: string): Promise<AdminSnapshot> {
  const [
    users,
    sipAccounts,
    registrations,
    dialogs,
    conferences,
    calls,
    mediaConfig,
    files,
    routingRules,
    auditEvents,
    subscriptions,
    notifications,
    presence,
  ] = await Promise.all([
    adminGet<AdminUser[]>(baseUrl, token, "/v1/users"),
    adminGet<AdminSipAccount[]>(baseUrl, token, "/v1/sip/accounts"),
    adminGet<AdminRegistration[]>(baseUrl, token, "/v1/sip/registrations"),
    adminGet<AdminDialog[]>(baseUrl, token, "/v1/sip/dialogs"),
    adminGet<AdminConference[]>(baseUrl, token, "/v1/conferences"),
    adminGet<AdminCall[]>(baseUrl, token, "/v1/calls"),
    adminGet<AdminMediaConfig>(baseUrl, token, "/v1/media/config"),
    adminGet<AdminFile[]>(baseUrl, token, "/v1/files"),
    adminGet<RoutingRule[]>(baseUrl, token, "/v1/routing/rules"),
    adminGet<AdminAuditEvent[]>(baseUrl, token, "/v1/admin/audit"),
    adminGet<AdminSubscription[]>(baseUrl, token, "/v1/sip/subscriptions"),
    adminGet<AdminNotification[]>(baseUrl, token, "/v1/sip/notifications"),
    adminGet<AdminPresence[]>(baseUrl, token, "/v1/presence"),
  ]);

  return {
    users,
    sipAccounts,
    registrations,
    dialogs,
    conferences,
    calls,
    mediaConfig,
    files,
    routingRules,
    auditEvents,
    subscriptions,
    notifications,
    presence,
  };
}

export function createAdminUser(baseUrl: string, token: string, input: CreateUserInput) {
  return adminPost<AdminUser>(baseUrl, token, "/v1/users", input);
}

export function deleteAdminUser(baseUrl: string, token: string, id: string) {
  return request<AdminUser>(baseUrl, `/v1/users/${id}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
}

export function setAdminUserActive(baseUrl: string, token: string, id: string, active: boolean) {
  return request<AdminUser>(baseUrl, `/v1/users/${id}/active`, {
    method: "PUT",
    headers: authHeaders(token),
    body: JSON.stringify({ active }),
  });
}

export function createAdminSipAccount(
  baseUrl: string,
  token: string,
  input: CreateSipAccountInput
) {
  return adminPost<AdminSipAccount>(baseUrl, token, "/v1/sip/accounts", input);
}

export function setAdminSipAccountEnabled(
  baseUrl: string,
  token: string,
  username: string,
  domain: string,
  enabled: boolean
) {
  return request<AdminSipAccount>(
    baseUrl,
    `/v1/sip/accounts/${encodeURIComponent(username)}/${encodeURIComponent(domain)}`,
    {
      method: "PUT",
      headers: authHeaders(token),
      body: JSON.stringify({ enabled }),
    }
  );
}

export function deleteAdminSipAccount(
  baseUrl: string,
  token: string,
  username: string,
  domain: string
) {
  return request<AdminSipAccount>(
    baseUrl,
    `/v1/sip/accounts/${encodeURIComponent(username)}/${encodeURIComponent(domain)}`,
    {
      method: "DELETE",
      headers: authHeaders(token),
    }
  );
}

export function createRoutingRule(baseUrl: string, token: string, input: CreateRoutingRuleInput) {
  return adminPost<RoutingRule>(baseUrl, token, "/v1/routing/rules", input);
}

export function updateRoutingRule(
  baseUrl: string,
  token: string,
  id: string,
  input: CreateRoutingRuleInput
) {
  return request<RoutingRule>(baseUrl, `/v1/routing/rules/${id}`, {
    method: "PUT",
    headers: authHeaders(token),
    body: JSON.stringify(input),
  });
}

export function deleteRoutingRule(baseUrl: string, token: string, id: string) {
  return request<RoutingRule>(baseUrl, `/v1/routing/rules/${id}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
}

// ─── Conference Management ───

export function createConference(baseUrl: string, token: string, input: CreateConferenceInput) {
  return adminPost<AdminConference>(baseUrl, token, "/v1/conferences", input);
}

export function joinConference(baseUrl: string, token: string, id: string, input: JoinConferenceInput) {
  return adminPost<AdminConference>(baseUrl, token, `/v1/conferences/${id}/participants`, input);
}

export function leaveConference(baseUrl: string, token: string, id: string, userId: string) {
  return request<AdminConference>(baseUrl, `/v1/conferences/${id}/participants/${userId}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
}

// ─── File Management ───

export function deleteFile(baseUrl: string, token: string, id: string) {
  return request<AdminFile>(baseUrl, `/v1/files/${id}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
}

// ─── Presence ───

export function loadPresence(baseUrl: string, token: string) {
  return adminGet<AdminPresence[]>(baseUrl, token, "/v1/presence");
}

export function loadSecurityPosture(baseUrl: string, token: string) {
  return adminGet<SecurityPostureReport>(baseUrl, token, "/v1/admin/security-score");
}

// ─── MFA / TOTP ───

export interface MfaSetupResponse {
  provisioning_uri: string;
  secret_base32: string;
  backup_codes: string[];
}

export interface MfaStatusResponse {
  enabled: boolean;
}

export function getMfaStatus(baseUrl: string, token: string) {
  return adminGet<MfaStatusResponse>(baseUrl, token, "/v1/mfa/status");
}

export function setupMfa(baseUrl: string, token: string) {
  return adminPost<MfaSetupResponse>(baseUrl, token, "/v1/mfa/setup", {});
}

export function verifyMfa(baseUrl: string, token: string, code: string) {
  return adminPost<{ ok: boolean; mfa_enabled: boolean }>(baseUrl, token, "/v1/mfa/verify", { code });
}

/** Complete login after password/SSO when the server returned mfa_required. */
export function validateMfa(baseUrl: string, mfaPendingToken: string, code: string) {
  return adminPost<{
    token: string;
    user: { id: string; display_name: string; sip_uri: string; role: string };
    sip_credentials: {
      sip_uri: string;
      registrar_uri: string | null;
      registration_available: boolean;
      username: string;
      password: string;
      transport: string;
      domain: string;
    } | null;
    expires_at: string;
    mfa_required: boolean;
  }>(baseUrl, mfaPendingToken, "/v1/mfa/validate", { code });
}

export function disableMfa(baseUrl: string, token: string) {
  return adminPost<{ ok: boolean; mfa_enabled: boolean }>(baseUrl, token, "/v1/mfa/disable", {});
}

// ─── Session Management ───

export interface SessionInfo {
  id: string;
  device_name: string;
  device_type: string;
  ip_address: string;
  created_at: string;
  last_active: string;
  current: boolean;
}

export function listSessions(baseUrl: string, token: string) {
  return adminGet<SessionInfo[]>(baseUrl, token, "/v1/sessions");
}

export function revokeSession(baseUrl: string, token: string, id: string) {
  return request<{ ok: boolean }>(baseUrl, `/v1/sessions/${id}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
}

export function revokeAllSessions(baseUrl: string, token: string) {
  return adminPost<{ ok: boolean; revoked: number }>(baseUrl, token, "/v1/sessions/revoke-all", {});
}

function adminGet<T>(baseUrl: string, token: string, path: string) {
  return request<T>(baseUrl, path, { headers: authHeaders(token) });
}

function adminPost<T>(baseUrl: string, token: string, path: string, body: unknown) {
  return request<T>(baseUrl, path, {
    method: "POST",
    headers: authHeaders(token),
    body: JSON.stringify(body),
  });
}

async function request<T>(baseUrl: string, path: string, init: RequestInit): Promise<T> {
  const response = await paleFetch(`${baseUrl.replace(/\/+$/, "")}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init.headers || {}),
    },
  });
  if (!response.ok) {
    const fallback = `${response.status} ${response.statusText}`;
    const payload = await response.json().catch(() => ({ error: fallback }));
    throw new Error(payload.error || fallback);
  }
  return response.json() as Promise<T>;
}

function authHeaders(token: string) {
  return { Authorization: `Bearer ${token}` };
}
