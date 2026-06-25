/**
 * Typed wrappers around Tauri's invoke() and listen() for the Pale SIP engine.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CallDirection, CallState, RegState } from "@/types";

// ─── Command Payloads ───

export interface AccountConfig {
  display_name: string;
  sip_uri: string;
  registrar_uri: string;
  auth_username: string;
  auth_password: string;
  transport: "udp" | "tcp" | "tls";
}

// ─── Invoke Wrappers ───

export function registerAccount(config: AccountConfig): Promise<void> {
  return invoke("register_account", { config });
}

export function makeCall(uri: string): Promise<void> {
  return invoke("make_call", { uri });
}

export function answerCall(callId: number): Promise<void> {
  return invoke("answer_call", { callId });
}

export function hangupCall(callId: number): Promise<void> {
  return invoke("hangup_call", { callId });
}

export function holdCall(callId: number): Promise<void> {
  return invoke("hold_call", { callId });
}

export function unholdCall(callId: number): Promise<void> {
  return invoke("unhold_call", { callId });
}

export function setMute(callId: number, muted: boolean): Promise<void> {
  return invoke("set_mute", { callId, muted });
}

export function sendDtmf(callId: number, digits: string): Promise<void> {
  return invoke("send_dtmf", { callId, digits });
}

export function blindTransfer(callId: number, target: string): Promise<void> {
  return invoke("blind_transfer", { callId, target });
}

export function attendedTransfer(callId: number, targetCallId: number): Promise<void> {
  return invoke("attended_transfer", { callId, targetCallId });
}

// ─── Call History ───

export interface CallRecord {
  id: number;
  direction: string;
  remote_uri: string;
  remote_name: string;
  start_time: string;
  duration_secs: number;
  answered: boolean;
}

export function getCallHistory(): Promise<CallRecord[]> {
  return invoke("get_call_history");
}

export function addCallRecord(record: CallRecord): Promise<number> {
  return invoke("add_call_record", { record });
}

export function deleteCallRecord(id: number): Promise<void> {
  return invoke("delete_call_record", { id });
}

export function clearCallHistory(): Promise<void> {
  return invoke("clear_call_history");
}

// ─── Config Persistence ───

export interface AppConfig {
  account?: {
    display_name: string;
    sip_uri: string;
    registrar_uri: string;
    auth_username: string;
    transport: "udp" | "tcp" | "tls";
    reg_expiry: number;
  };
  audio: {
    input_device: string | null;
    output_device: string | null;
    echo_cancel: boolean;
    noise_suppression: boolean;
    auto_gain: boolean;
    codec_priority: string[];
  };
  network: {
    stun_server: string;
    turn_server: string;
    turn_username: string;
    enable_ice: boolean;
    sip_port: number;
    rtp_port_min: number;
    rtp_port_max: number;
  };
  matrix: {
    homeserver: string;
    username: string;
    user_id: string | null;
  };
  server: {
    url: string;
    username: string;
    auto_connect: boolean;
  };
  notifications: {
    enabled: boolean;
    sound_enabled: boolean;
    dnd_enabled: boolean;
    dnd_start: string;
    dnd_end: string;
    muted_rooms: string[];
  };
  ui: {
    theme: string;
    window_width: number;
    window_height: number;
  };
}

export function getConfig(): Promise<AppConfig> {
  return invoke("get_config");
}

export function saveSettings(config: AppConfig): Promise<void> {
  return invoke("save_settings", { config });
}

// ─── Keychain ───

export function storeSipPassword(accountId: string, password: string): Promise<void> {
  return invoke("store_sip_password", { accountId, password });
}

export function getSipPassword(accountId: string): Promise<string | null> {
  return invoke("get_sip_password", { accountId });
}

export function deleteSipPassword(accountId: string): Promise<void> {
  return invoke("delete_sip_password", { accountId });
}

// ─── Audio Devices ───

export interface AudioDeviceInfo {
  id: number;
  name: string;
  input_count: number;
  output_count: number;
}

export function listAudioDevices(): Promise<AudioDeviceInfo[]> {
  return invoke("list_audio_devices");
}

// ─── Call Recording ───

export function startRecording(callId: number): Promise<string> {
  return invoke("start_recording", { callId });
}

export function stopRecording(callId: number): Promise<void> {
  return invoke("stop_recording", { callId });
}

export interface RecordingStateEvent {
  type: "recording_state";
  call_id: number;
  recording: boolean;
  file_path: string;
}

export function onRecordingState(
  handler: (event: RecordingStateEvent) => void
): Promise<UnlistenFn> {
  return listen<RecordingStateEvent>("sip://recording-state", (e) => handler(e.payload));
}

// ─── Video Commands ───

export function makeVideoCall(uri: string): Promise<void> {
  return invoke("make_video_call", { uri });
}

export function toggleVideo(callId: number, enabled: boolean): Promise<void> {
  return invoke("toggle_video", { callId, enabled });
}

export function startScreenShare(callId: number, enabled: boolean): Promise<void> {
  return invoke("start_screen_share", { callId, enabled });
}

// ─── Matrix Commands ───

export function matrixLogin(homeserver: string, username: string, password: string): Promise<string> {
  return invoke("matrix_login", { homeserver, username, password });
}

export function matrixLogout(): Promise<void> {
  return invoke("matrix_logout");
}

export function matrixGetRooms(): Promise<any[]> {
  return invoke("matrix_get_rooms");
}

export function matrixSendMessage(roomId: string, body: string): Promise<string> {
  return invoke("matrix_send_message", { roomId, body });
}

export function matrixSetTyping(roomId: string, typing: boolean): Promise<void> {
  return invoke("matrix_set_typing", { roomId, typing });
}

export function matrixSendFile(roomId: string, filePath: string): Promise<string> {
  return invoke("matrix_send_file", { roomId, filePath });
}

export function matrixCreateDm(userId: string): Promise<string> {
  return invoke("matrix_create_dm", { userId });
}

export function matrixIsLoggedIn(): Promise<boolean> {
  return invoke("matrix_is_logged_in");
}

// Matrix event listeners
export function onMatrixAuthState(handler: (event: unknown) => void): Promise<UnlistenFn> {
  return listen("matrix://auth-state", (e) => handler(e.payload));
}

export function onMatrixRooms(handler: (event: unknown) => void): Promise<UnlistenFn> {
  return listen("matrix://rooms", (e) => handler(e.payload));
}

export function onMatrixMessage(handler: (event: unknown) => void): Promise<UnlistenFn> {
  return listen("matrix://message", (e) => handler(e.payload));
}

export function onMatrixTyping(handler: (event: unknown) => void): Promise<UnlistenFn> {
  return listen("matrix://typing", (e) => handler(e.payload));
}

// ─── Event Types (from pale-core PaleEvent) ───

export interface RegStateEvent {
  type: "registration_state";
  account_id: number;
  state: RegState;
  reason: string;
}

export interface IncomingCallEvent {
  type: "incoming_call";
  call_id: number;
  account_id: number;
  caller_name: string;
  caller_uri: string;
}

export interface CallStateEvent {
  type: "call_state";
  call_id: number;
  state: CallState;
  direction: CallDirection;
  remote_uri: string;
  remote_name: string;
}

export interface AudioLevelEvent {
  type: "audio_level";
  input: number;
  output: number;
}

export interface PaleErrorEvent {
  type: "error";
  message: string;
}

// ─── Event Listeners ───

export function onRegState(
  handler: (event: RegStateEvent) => void
): Promise<UnlistenFn> {
  return listen<RegStateEvent>("sip://reg-state", (e) => handler(e.payload));
}

export function onIncomingCall(
  handler: (event: IncomingCallEvent) => void
): Promise<UnlistenFn> {
  return listen<IncomingCallEvent>("sip://incoming-call", (e) =>
    handler(e.payload)
  );
}

export function onCallState(
  handler: (event: CallStateEvent) => void
): Promise<UnlistenFn> {
  return listen<CallStateEvent>("sip://call-state", (e) =>
    handler(e.payload)
  );
}

export function onAudioLevel(
  handler: (event: AudioLevelEvent) => void
): Promise<UnlistenFn> {
  return listen<AudioLevelEvent>("audio://level", (e) => handler(e.payload));
}

export function onAudioDevicesChanged(
  handler: () => void
): Promise<UnlistenFn> {
  return listen("audio://devices-changed", () => handler());
}

export function onPaleError(
  handler: (event: PaleErrorEvent) => void
): Promise<UnlistenFn> {
  return listen<PaleErrorEvent>("pale://error", (e) => handler(e.payload));
}

// ─── Pale Server API (HTTP fetch, not Tauri invoke) ───

export type PresenceStatus = "online" | "offline" | "busy" | "away" | "dnd" | "on_call";

export interface ServerPresence {
  sip_uri: string;
  status: PresenceStatus;
  note: string | null;
  updated_at: string;
}

async function serverFetch<T>(
  baseUrl: string,
  token: string,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const method = init?.method ?? "GET";
  const body = init?.body ? JSON.parse(init.body as string) : undefined;

  return invoke("pale_server_request", {
    input: {
      base_url: baseUrl,
      method,
      path,
      token,
      body: body ?? null,
    },
  });
}

export function paleServerGetPresence(
  baseUrl: string,
  token: string,
): Promise<ServerPresence[]> {
  return serverFetch(baseUrl, token, "/v1/presence");
}

export function paleServerSetPresence(
  baseUrl: string,
  token: string,
  status: PresenceStatus,
  note?: string | null,
): Promise<ServerPresence> {
  return serverFetch(baseUrl, token, "/v1/presence", {
    method: "PUT",
    body: JSON.stringify({ status, note: note ?? null }),
  });
}

export interface ServerUser {
  id: string;
  display_name: string;
  sip_uri: string;
  matrix_user_id: string | null;
  created_at: string;
}

export function paleServerGetUsers(
  baseUrl: string,
  token: string,
): Promise<ServerUser[]> {
  return serverFetch(baseUrl, token, "/v1/users");
}

export interface ConferenceParticipant {
  user_id: string;
  sip_uri: string;
  role: "host" | "moderator" | "member";
  bridge_slot: number | null;
  joined_at: string;
}

export interface ConferenceSummary {
  id: string;
  title: string;
  mode: "audio" | "video" | "webinar";
  participants: ConferenceParticipant[];
  active: boolean;
  created_at: string;
}

export interface RingGroupSummary {
  id: string;
  name: string;
  extension: string;
  strategy: "simultaneous" | "sequential" | "random";
  ring_timeout: number;
  members: string[];
  fallback_uri: string | null;
  enabled: boolean;
  created_at: string;
}

export interface CallQueueSummary {
  id: string;
  name: string;
  extension: string;
  strategy: string;
  max_wait_time: number;
  max_queue_size: number;
  wrap_up_time: number;
  announce_position: boolean;
  announce_interval: number;
  hold_music_file_id: string | null;
  overflow_destination: string | null;
  agents: { agent_uri: string; priority: number; skills: string[]; state: string; calls_handled: number; penalty: number }[];
  enabled: boolean;
  created_at: string;
  callback_enabled: boolean;
  callback_threshold_secs: number;
  sla_target_secs: number;
}

export interface PagingGroupSummary {
  id: string;
  name: string;
  extension: string;
  members: string[];
}

export function paleServerGetConferences(baseUrl: string, token: string): Promise<ConferenceSummary[]> {
  return serverFetch(baseUrl, token, "/v1/conferences");
}

export function paleServerGetRingGroups(baseUrl: string, token: string): Promise<RingGroupSummary[]> {
  return serverFetch(baseUrl, token, "/v1/ring-groups");
}

export function paleServerGetQueues(baseUrl: string, token: string): Promise<CallQueueSummary[]> {
  return serverFetch(baseUrl, token, "/v1/queues");
}

export function paleServerGetPagingGroups(baseUrl: string, token: string): Promise<PagingGroupSummary[]> {
  return serverFetch(baseUrl, token, "/v1/paging-groups");
}

// ─── Call History Sync ───

export interface ServerCallHistoryEntry {
  id: string;
  user_sip_uri: string;
  direction: string;
  remote_uri: string;
  remote_name: string;
  start_time: string;
  duration_secs: number;
  answered: boolean;
  synced_at: string;
}

export function paleServerGetCallHistory(
  baseUrl: string,
  token: string,
): Promise<ServerCallHistoryEntry[]> {
  return serverFetch(baseUrl, token, "/v1/call-history");
}

export function paleServerSyncCallHistory(
  baseUrl: string,
  token: string,
  entries: Omit<ServerCallHistoryEntry, "id" | "user_sip_uri" | "synced_at">[],
): Promise<{ merged: number }> {
  return serverFetch(baseUrl, token, "/v1/call-history", {
    method: "POST",
    body: JSON.stringify({ entries }),
  });
}

// ─── Message History ───

export interface ServerSipMessage {
  id: string;
  call_id: string | null;
  from_uri: string;
  to_uri: string;
  content_type: string;
  body: string;
  received_at: string;
}

export function paleServerGetMessages(
  baseUrl: string,
  token: string,
  options?: { limit?: number; before?: string; roomId?: string },
): Promise<ServerSipMessage[]> {
  const params = new URLSearchParams();
  if (options?.limit) params.set("limit", String(options.limit));
  if (options?.before) params.set("before", options.before);
  if (options?.roomId) params.set("room_id", options.roomId);
  const qs = params.toString();
  return serverFetch(baseUrl, token, `/v1/sip/messages${qs ? `?${qs}` : ""}`);
}

// ─── Group Chat Rooms ───

export interface ServerRoom {
  id: string;
  name: string;
  description: string;
  is_direct: boolean;
  created_by: string;
  members: { user_sip_uri: string; role: string; joined_at: string }[];
  created_at: string;
}

export interface ServerRoomMessage {
  id: string;
  room_id: string;
  sender_uri: string;
  body: string;
  content_type: string;
  created_at: string;
  reply_to?: string;
  edited_at?: string;
  pinned?: boolean;
}

export function paleServerGetRooms(baseUrl: string, token: string): Promise<ServerRoom[]> {
  return serverFetch(baseUrl, token, "/v1/rooms");
}

export function paleServerCreateRoom(
  baseUrl: string,
  token: string,
  name: string,
  description: string,
  members: string[],
): Promise<ServerRoom> {
  return serverFetch(baseUrl, token, "/v1/rooms", {
    method: "POST",
    body: JSON.stringify({ name, description, members }),
  });
}

export function paleServerCreateDirectRoom(
  baseUrl: string,
  token: string,
  user: Pick<ServerUser, "display_name" | "sip_uri">,
): Promise<ServerRoom> {
  return serverFetch(baseUrl, token, "/v1/rooms", {
    method: "POST",
    body: JSON.stringify({
      name: user.display_name,
      description: "",
      members: [user.sip_uri],
      is_direct: true,
    }),
  });
}

export function paleServerGetRoomMessages(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerRoomMessage[]> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/messages`);
}

export function paleServerSendRoomMessage(
  baseUrl: string,
  token: string,
  roomId: string,
  body: string,
  replyTo?: string,
): Promise<ServerRoomMessage> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/messages`, {
    method: "POST",
    body: JSON.stringify({ body, reply_to: replyTo ?? null }),
  });
}

export function paleServerSetTyping(
  baseUrl: string,
  token: string,
  roomId: string,
  typing: boolean,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/typing`, {
    method: "POST",
    body: JSON.stringify({ typing }),
  });
}

export function paleServerPinMessage(
  baseUrl: string,
  token: string,
  messageId: string,
  pinned: boolean,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}/pin`, {
    method: "PUT",
    body: JSON.stringify({ pinned }),
  });
}

export function paleServerGetPinnedMessages(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerRoomMessage[]> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/pinned`);
}

export function paleServerAddFavorite(
  baseUrl: string,
  token: string,
  sipUri: string,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/favorites`, {
    method: "POST",
    body: JSON.stringify({ sip_uri: sipUri }),
  });
}

export function paleServerRemoveFavorite(
  baseUrl: string,
  token: string,
  sipUri: string,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/favorites/${encodeURIComponent(sipUri)}`, {
    method: "DELETE",
  });
}

export function paleServerGetFavorites(
  baseUrl: string,
  token: string,
): Promise<string[]> {
  return serverFetch(baseUrl, token, `/v1/favorites`);
}

export function paleServerUpdateProfile(
  baseUrl: string,
  token: string,
  updates: { display_name?: string; title?: string; department?: string; email?: string; status_message?: string },
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/profile`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

// ─── Search ───

export interface SearchResult {
  id: string;
  source: string;
  from_uri: string;
  body: string;
  timestamp: string;
  room_id: string | null;
}

export function paleServerSearchMessages(
  baseUrl: string,
  token: string,
  query: string,
  limit?: number,
): Promise<SearchResult[]> {
  const params = new URLSearchParams({ q: query });
  if (limit) params.set("limit", String(limit));
  return serverFetch(baseUrl, token, `/v1/search/messages?${params}`);
}

// ─── Read Receipts ───

export function paleServerMarkRead(
  baseUrl: string,
  token: string,
  messageId: string,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}/read`, {
    method: "PUT",
  });
}

// ─── Message Edit & Delete ───

export function paleServerEditMessage(
  baseUrl: string,
  token: string,
  messageId: string,
  body: string,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}`, {
    method: "PUT",
    body: JSON.stringify({ body }),
  });
}

export function paleServerDeleteMessage(
  baseUrl: string,
  token: string,
  messageId: string,
): Promise<void> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}`, {
    method: "DELETE",
  });
}

/**
 * Generic server API call routed through Tauri (bypasses webview fetch restrictions).
 * Use this instead of fetch() for all pale-server API calls.
 */
export async function paleServerApi<T = unknown>(
  baseUrl: string,
  token: string,
  path: string,
  options?: { method?: string; body?: unknown },
): Promise<T> {
  return invoke("pale_server_request", {
    input: {
      base_url: baseUrl,
      method: options?.method ?? "GET",
      path,
      token,
      body: options?.body ?? null,
    },
  });
}

// ─── Unified Login ───

export interface UserLoginResponse {
  token: string;
  user: {
    id: string;
    display_name: string;
    sip_uri: string;
    role: string;
  };
  sip_credentials: {
    sip_uri: string;
    registrar_uri: string;
    username: string;
    password: string;
    transport: string;
    domain: string;
  } | null;
  expires_at: string;
}

export async function paleLogin(
  baseUrl: string,
  sipUri: string,
  password: string,
): Promise<UserLoginResponse> {
  return invoke("pale_server_login", {
    input: { base_url: baseUrl, sip_uri: sipUri, password },
  });
}

// ─── Server Files ───

export interface PaleServerFile {
  id: string;
  owner: string;
  filename: string;
  content_type: string;
  size: number;
  sha256: string;
  created_at: string;
}

export function paleServerGetFiles(
  baseUrl: string,
  token: string,
): Promise<PaleServerFile[]> {
  return serverFetch(baseUrl, token, "/v1/files");
}

export async function paleServerUploadFile(
  baseUrl: string,
  token: string,
  file: File,
): Promise<PaleServerFile> {
  const buffer = await file.arrayBuffer();
  const response = await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/files`, {
    method: "POST",
    headers: {
      "Content-Type": file.type || "application/octet-stream",
      Authorization: `Bearer ${token}`,
      "X-Pale-Filename": file.name,
    },
    body: buffer,
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(payload.error || response.statusText);
  }
  return response.json();
}

export function paleServerDeleteFile(
  baseUrl: string,
  token: string,
  id: string,
): Promise<PaleServerFile> {
  return serverFetch(baseUrl, token, `/v1/files/${id}`, { method: "DELETE" });
}

export function paleServerFileDownloadUrl(baseUrl: string, _token: string, id: string): string {
  return `${baseUrl.replace(/\/+$/, "")}/v1/files/${id}`;
}
