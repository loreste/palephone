/**
 * Typed wrappers around Tauri's invoke() and listen() for the Pale SIP engine.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CallDirection, CallState, RegState } from "@/types";

// ─── Pale HTTP Client ───

/** Version injected at build time from package.json */
const PALE_VERSION = __PALE_VERSION__;

/**
 * Drop-in replacement for `fetch()` that always sends the Pale User-Agent.
 * All direct HTTP calls to the Pale server MUST use this instead of `fetch()`.
 */
export function paleFetch(
  input: string | URL | Request,
  init?: RequestInit,
): Promise<Response> {
  const headers = new Headers(init?.headers);
  // Browsers forbid setting User-Agent in fetch(), so we use a custom header
  // that the server also accepts as proof of a Pale client.
  if (!headers.has("X-Pale-Client")) {
    headers.set("X-Pale-Client", `Pale/${PALE_VERSION}`);
  }
  return fetch(input, { ...init, headers });
}

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

export function openPopoutWindow(
  kind: "chat" | "meeting" | "call" | "files" | "calendar",
  targetId?: string | null,
  title?: string,
): Promise<string> {
  return invoke("open_popout_window", { kind, targetId, title });
}

async function requestMediaForNativeCall(video = false): Promise<void> {
  // In Tauri desktop/mobile, PJSIP opens audio/video devices directly via
  // the OS (CoreAudio, ALSA, OpenSL ES). The WebView's getUserMedia is not
  // available or needed — skip the check entirely in Tauri.
  if ((window as any).__TAURI_INTERNALS__) return;

  const mediaDevices = globalThis.navigator?.mediaDevices;
  if (!mediaDevices?.getUserMedia) return;

  let stream: MediaStream | null = null;
  try {
    stream = await mediaDevices.getUserMedia({ audio: true, video });
  } catch (error) {
    const label = video ? "microphone/camera" : "microphone";
    throw new Error(`Allow ${label} access before starting a call.`);
  } finally {
    stream?.getTracks().forEach((track) => track.stop());
  }
}

export async function makeCall(uri: string): Promise<void> {
  await requestMediaForNativeCall(false);
  return invoke("make_call", { uri });
}

export async function answerCall(callId: number): Promise<void> {
  // Request camera as well so video offers can be accepted (Android/desktop).
  await requestMediaForNativeCall(true);
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
    turn_password: string;
    enable_ice: boolean;
    srtp_mode: "disabled" | "optional" | "required";
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
    role?: string | null;
    display_name?: string | null;
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

export async function makeVideoCall(uri: string): Promise<void> {
  await requestMediaForNativeCall(true);
  return invoke("make_video_call", { uri });
}

export function toggleVideo(callId: number, enabled: boolean): Promise<void> {
  return invoke("toggle_video", { callId, enabled });
}

/** Force all SIP accounts to re-register. Call on app resume or network change. */
export function refreshRegistration(): Promise<void> {
  return invoke("refresh_registration");
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

export interface VideoStreamEvent {
  call_id: number;
  active: boolean;
  has_incoming: boolean;
  has_outgoing: boolean;
}

export function onVideoStream(
  handler: (event: VideoStreamEvent) => void
): Promise<UnlistenFn> {
  return listen<VideoStreamEvent>("video://stream-state", (e) =>
    handler(e.payload)
  );
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
  muted?: boolean;
  removed?: boolean;
  removed_at?: string | null;
  removed_by?: string | null;
  removal_reason?: string | null;
  joined_at: string;
}

export interface ConferenceSummary {
  id: string;
  title: string;
  mode: "audio" | "video" | "webinar";
  participants: ConferenceParticipant[];
  locked?: boolean;
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
  team_id?: string | null;
  channel_name?: string | null;
  channel_type?: "standard" | "private" | "shared";
  channel_owners?: string[];
  posting_policy?: "members" | "owners";
  name: string;
  description: string;
  is_direct: boolean;
  created_by: string;
  members: { user_sip_uri: string; role: string; joined_at: string }[];
  conference_id?: string | null;
  call_uri?: string | null;
  created_at: string;
}

export interface ServerTeam {
  id: string;
  name: string;
  description: string;
  owner_uri: string;
  members: { user_sip_uri: string; role: string; joined_at: string }[];
  created_at: string;
}

export interface ServerMeeting {
  id: string;
  title: string;
  description: string;
  organizer_uri: string;
  room_id?: string | null;
  conference_id?: string | null;
  participants: string[];
  starts_at: string;
  ends_at: string;
  recurrence?: {
    frequency: "daily" | "weekly" | "monthly";
    interval: number;
    until?: string | null;
  } | null;
  status?: "scheduled" | "cancelled";
  cancelled_at?: string | null;
  updated_at?: string | null;
  created_at: string;
}

export interface ServerCollaborationSearchResult {
  kind: "direct" | "room" | "channel" | "team" | "meeting" | "conference";
  id: string;
  title: string;
  subtitle: string;
  room_id?: string | null;
  team_id?: string | null;
  conference_id?: string | null;
  call_uri?: string | null;
  updated_at: string;
}

export interface ServerRetentionPolicy {
  id: string;
  name: string;
  scope: string;
  room_id?: string | null;
  retain_days?: number | null;
  legal_hold: boolean;
  export_enabled: boolean;
  created_by: string;
  updated_at: string;
}

export interface ServerRetentionEnforcementResult {
  evaluated_at: string;
  dry_run: boolean;
  matched_messages: number;
  deleted_messages: number;
  skipped_legal_hold_policies: string[];
  policy_results: {
    policy_id: string;
    room_id?: string | null;
    retain_days?: number | null;
    matched_messages: number;
    deleted_messages: number;
    matched_files?: number;
    deleted_files?: number;
    legal_hold: boolean;
  }[];
}

export interface ServerCollaborationPolicy {
  id: string;
  structured_mentions_enabled: boolean;
  broad_mentions_enabled: boolean;
  broad_mentions_allowed_roles: string[];
  broad_mentions_per_minute: number;
  external_access_enabled: boolean;
  allowed_external_domains: string[];
  urgent_messages_enabled: boolean;
  meeting_recording_enabled: boolean;
  updated_by?: string | null;
  updated_at: string;
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
  priority?: "normal" | "high" | "urgent";
  saved_by?: string[];
  mentions?: { kind: string; token: string; user_sip_uri?: string | null }[];
  mentioned_user_uris?: string[];
  scheduled_at?: string;
  delivered?: boolean;
  delivery_status?: "pending" | "sent" | "delivered" | "failed";
  card_payload?: AdaptiveCardPayload | null;
  thread_id?: string | null;
}

export interface ServerMessageThread {
  id: string;
  room_id: string;
  root_message_id: string;
  reply_count: number;
  last_reply_at: string;
  participants: string[];
  created_at: string;
}

export interface ServerRateLimitConfig {
  default_rpm: number;
  auth_rpm: number;
  file_upload_rpm: number;
  message_send_rpm: number;
  sse_connections: number;
}

// ─── Adaptive Cards ───

export interface AdaptiveCardAction {
  action_type: string;
  title: string;
  url?: string | null;
  data?: unknown;
}

export interface AdaptiveCardPayload {
  card_type: string;
  title?: string | null;
  body?: string | null;
  image_url?: string | null;
  actions: AdaptiveCardAction[];
}

// ─── Custom Emojis ───

export interface CustomEmoji {
  id: string;
  team_id: string;
  shortcode: string;
  image_url: string;
  uploaded_by: string;
  created_at: string;
}

// ─── Wiki Pages ───

export interface WikiPage {
  id: string;
  team_id: string;
  title: string;
  body: string;
  created_by: string;
  updated_by: string;
  created_at: string;
  updated_at: string;
  parent_id?: string | null;
}

// ─── Task Boards & Tasks ───

export interface TaskBoard {
  id: string;
  team_id: string;
  name: string;
  created_by: string;
  created_at: string;
}

export interface TaskItem {
  id: string;
  board_id: string;
  title: string;
  description: string;
  assignee?: string | null;
  status: string;
  priority: string;
  due_date?: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

// ─── Tags ───

export interface ServerTag {
  id: string;
  team_id: string;
  name: string;
  members: string[];
  created_at: string;
}

// ─── Notification Preferences ───

export interface ServerNotificationPreference {
  room_id: string;
  user_uri: string;
  notification_level: "all" | "mentions" | "muted";
  updated_at: string;
}

// ─── GIF Search ───

export interface GifResult {
  title: string;
  url: string;
  preview: string;
}

export interface ServerChannelWebhook {
  id: string;
  room_id: string;
  name: string;
  description: string;
  enabled: boolean;
  created_by: string;
  created_at: string;
  last_used_at?: string | null;
}

export interface ServerCreateChannelWebhookResponse {
  webhook: ServerChannelWebhook;
  token: string;
}

export interface ServerMessageRead {
  message_id: string;
  reader_uri: string;
  read_at: string;
}

export interface ServerMessageReaction {
  emoji: string;
  user_uri: string;
  created_at: string;
}

export interface ServerRoomMessageState {
  message_id: string;
  reactions: ServerMessageReaction[];
  reads: ServerMessageRead[];
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
  teamId?: string | null,
  channelName?: string | null,
): Promise<ServerRoom> {
  return serverFetch(baseUrl, token, "/v1/rooms", {
    method: "POST",
    body: JSON.stringify({ name, description, members, team_id: teamId ?? null, channel_name: channelName ?? null }),
  });
}

export function paleServerGetTeams(baseUrl: string, token: string): Promise<ServerTeam[]> {
  return serverFetch(baseUrl, token, "/v1/teams");
}

export function paleServerCreateTeam(
  baseUrl: string,
  token: string,
  name: string,
  description: string,
  members: string[],
): Promise<ServerTeam> {
  return serverFetch(baseUrl, token, "/v1/teams", {
    method: "POST",
    body: JSON.stringify({ name, description, members }),
  });
}

export function paleServerCreateTeamChannel(
  baseUrl: string,
  token: string,
  teamId: string,
  name: string,
  description: string,
  members: string[],
  options: {
    channel_type?: "standard" | "private" | "shared";
    channel_owners?: string[];
    posting_policy?: "members" | "owners";
  } = {},
): Promise<ServerRoom> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/channels`, {
    method: "POST",
    body: JSON.stringify({
      name,
      description,
      members,
      channel_name: name,
      channel_type: options.channel_type ?? "standard",
      channel_owners: options.channel_owners ?? [],
      posting_policy: options.posting_policy ?? "members",
    }),
  });
}

export function paleServerGetMeetings(baseUrl: string, token: string): Promise<ServerMeeting[]> {
  return serverFetch(baseUrl, token, "/v1/meetings");
}

export function paleServerCreateMeeting(
  baseUrl: string,
  token: string,
  input: {
    title: string;
    description?: string;
    room_id?: string | null;
    participants: string[];
    starts_at: string;
    ends_at: string;
    mode?: "audio" | "video";
    recurrence?: ServerMeeting["recurrence"];
  },
): Promise<ServerMeeting> {
  return serverFetch(baseUrl, token, "/v1/meetings", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function paleServerUpdateMeeting(
  baseUrl: string,
  token: string,
  meetingId: string,
  input: Partial<Pick<ServerMeeting, "title" | "description" | "participants" | "starts_at" | "ends_at" | "recurrence">>,
): Promise<ServerMeeting> {
  return serverFetch(baseUrl, token, `/v1/meetings/${meetingId}`, {
    method: "PUT",
    body: JSON.stringify(input),
  });
}

export function paleServerCancelMeeting(
  baseUrl: string,
  token: string,
  meetingId: string,
): Promise<ServerMeeting> {
  return serverFetch(baseUrl, token, `/v1/meetings/${meetingId}`, { method: "DELETE" });
}

export function paleServerStartMeeting(
  baseUrl: string,
  token: string,
  meetingId: string,
): Promise<ServerRoomCallTarget> {
  return serverFetch(baseUrl, token, `/v1/meetings/${meetingId}/start`, { method: "POST" });
}

export function paleServerSearchCollaboration(
  baseUrl: string,
  token: string,
  query: string,
  limit = 25,
): Promise<ServerCollaborationSearchResult[]> {
  const params = new URLSearchParams({ q: query, limit: String(limit) });
  return serverFetch(baseUrl, token, `/v1/search/collaboration?${params.toString()}`);
}

export function paleServerGetRetentionPolicies(
  baseUrl: string,
  token: string,
): Promise<ServerRetentionPolicy[]> {
  return serverFetch(baseUrl, token, "/v1/admin/governance/retention");
}

export function paleServerUpsertRetentionPolicy(
  baseUrl: string,
  token: string,
  policy: Partial<ServerRetentionPolicy> & { name: string; scope: string },
): Promise<ServerRetentionPolicy> {
  return serverFetch(baseUrl, token, "/v1/admin/governance/retention", {
    method: "PUT",
    body: JSON.stringify(policy),
  });
}

export function paleServerPreviewRetentionEnforcement(
  baseUrl: string,
  token: string,
): Promise<ServerRetentionEnforcementResult> {
  return serverFetch(baseUrl, token, "/v1/admin/governance/retention/enforce");
}

export function paleServerApplyRetentionEnforcement(
  baseUrl: string,
  token: string,
): Promise<ServerRetentionEnforcementResult> {
  return serverFetch(baseUrl, token, "/v1/admin/governance/retention/enforce", {
    method: "POST",
  });
}

export function paleServerDiscoveryExport(
  baseUrl: string,
  token: string,
  roomId?: string,
): Promise<{ exported_at: string; room_id?: string | null; messages: ServerRoomMessage[]; files: import("@/store/fileStore").ServerFile[] }> {
  const query = roomId ? `?room_id=${encodeURIComponent(roomId)}` : "";
  return serverFetch(baseUrl, token, `/v1/admin/ediscovery/export${query}`);
}

export function paleServerGetCollaborationPolicy(
  baseUrl: string,
  token: string,
): Promise<ServerCollaborationPolicy> {
  return serverFetch(baseUrl, token, "/v1/admin/collaboration/policy");
}

export function paleServerUpdateCollaborationPolicy(
  baseUrl: string,
  token: string,
  policy: Partial<Pick<
    ServerCollaborationPolicy,
    | "structured_mentions_enabled"
    | "broad_mentions_enabled"
    | "broad_mentions_allowed_roles"
    | "broad_mentions_per_minute"
    | "external_access_enabled"
    | "allowed_external_domains"
    | "urgent_messages_enabled"
    | "meeting_recording_enabled"
  >>,
): Promise<ServerCollaborationPolicy> {
  return serverFetch(baseUrl, token, "/v1/admin/collaboration/policy", {
    method: "PUT",
    body: JSON.stringify(policy),
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
  options?: { limit?: number; before?: string },
): Promise<ServerRoomMessage[]> {
  const params = new URLSearchParams();
  if (options?.limit) params.set("limit", String(options.limit));
  if (options?.before) params.set("before", options.before);
  const qs = params.toString();
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/messages${qs ? `?${qs}` : ""}`);
}

export function paleServerGetRoomMessageState(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerRoomMessageState[]> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/message-state`);
}

export function paleServerSendRoomMessage(
  baseUrl: string,
  token: string,
  roomId: string,
  body: string,
  replyTo?: string,
  priority: "normal" | "high" | "urgent" = "normal",
): Promise<ServerRoomMessage> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/messages`, {
    method: "POST",
    body: JSON.stringify({ body, reply_to: replyTo ?? null, priority }),
  });
}

// ─── Message Threads ───

export function paleServerGetRoomThreads(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerMessageThread[]> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/threads`);
}

export function paleServerGetThreadMessages(
  baseUrl: string,
  token: string,
  threadId: string,
): Promise<ServerRoomMessage[]> {
  return serverFetch(baseUrl, token, `/v1/threads/${threadId}/messages`);
}

export function paleServerReplyToThread(
  baseUrl: string,
  token: string,
  rootMessageId: string,
  body: string,
  priority: "normal" | "high" | "urgent" = "normal",
): Promise<{ message: ServerRoomMessage; thread: ServerMessageThread }> {
  return serverFetch(baseUrl, token, `/v1/threads/${rootMessageId}/reply`, {
    method: "POST",
    body: JSON.stringify({ body, priority }),
  });
}

export function paleServerScheduleRoomMessage(
  baseUrl: string,
  token: string,
  roomId: string,
  body: string,
  scheduledAt: string,
  replyTo?: string,
  priority: "normal" | "high" | "urgent" = "normal",
): Promise<ServerRoomMessage> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/messages/schedule`, {
    method: "POST",
    body: JSON.stringify({ body, scheduled_at: scheduledAt, reply_to: replyTo ?? null, priority }),
  });
}

// ─── Tag API ───

export function paleServerGetTags(
  baseUrl: string,
  token: string,
  teamId: string,
): Promise<ServerTag[]> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/tags`);
}

export function paleServerCreateTag(
  baseUrl: string,
  token: string,
  teamId: string,
  name: string,
  members: string[],
): Promise<ServerTag> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/tags`, {
    method: "POST",
    body: JSON.stringify({ name, members }),
  });
}

export function paleServerUpdateTag(
  baseUrl: string,
  token: string,
  teamId: string,
  tagId: string,
  updates: { name?: string; members?: string[] },
): Promise<ServerTag> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/tags/${tagId}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export function paleServerDeleteTag(
  baseUrl: string,
  token: string,
  teamId: string,
  tagId: string,
): Promise<ServerTag> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/tags/${tagId}`, {
    method: "DELETE",
  });
}

// ─── Notification Preferences API ───

export function paleServerGetNotificationPreference(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerNotificationPreference> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/notifications`);
}

export function paleServerSetNotificationPreference(
  baseUrl: string,
  token: string,
  roomId: string,
  level: "all" | "mentions" | "muted",
): Promise<ServerNotificationPreference> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/notifications`, {
    method: "PUT",
    body: JSON.stringify({ notification_level: level }),
  });
}

// ─── GIF Search API ───

export function paleServerSearchGifs(
  baseUrl: string,
  token: string,
  query: string,
  limit?: number,
): Promise<{ results: GifResult[] }> {
  const params = new URLSearchParams({ q: query });
  if (limit) params.set("limit", String(limit));
  return serverFetch(baseUrl, token, `/v1/gif/search?${params}`);
}

export function paleServerGetChannelWebhooks(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerChannelWebhook[]> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/webhooks`);
}

export function paleServerCreateChannelWebhook(
  baseUrl: string,
  token: string,
  roomId: string,
  input: { name: string; description?: string },
): Promise<ServerCreateChannelWebhookResponse> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/webhooks`, {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function paleServerUpdateChannelWebhook(
  baseUrl: string,
  token: string,
  roomId: string,
  webhookId: string,
  enabled: boolean,
): Promise<ServerChannelWebhook> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/webhooks/${webhookId}`, {
    method: "PUT",
    body: JSON.stringify({ enabled }),
  });
}

export function paleServerDeleteChannelWebhook(
  baseUrl: string,
  token: string,
  roomId: string,
  webhookId: string,
): Promise<ServerChannelWebhook> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/webhooks/${webhookId}`, {
    method: "DELETE",
  });
}

export interface ServerRoomCallTarget {
  room_id: string;
  conference_id: string;
  call_uri: string;
  mode: "audio" | "video";
}

export interface ServerRoomCallEnded {
  room_id: string;
  conference_id: string;
  call_uri: string;
}

export function paleServerStartRoomCall(
  baseUrl: string,
  token: string,
  roomId: string,
  mode: "audio" | "video",
): Promise<ServerRoomCallTarget> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/call`, {
    method: "POST",
    body: JSON.stringify({ mode }),
  });
}

export function paleServerEndRoomCall(
  baseUrl: string,
  token: string,
  roomId: string,
): Promise<ServerRoomCallEnded> {
  return serverFetch(baseUrl, token, `/v1/rooms/${roomId}/call`, {
    method: "DELETE",
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

export function paleServerSaveMessage(
  baseUrl: string,
  token: string,
  messageId: string,
  saved: boolean,
): Promise<ServerRoomMessage> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}/saved`, {
    method: "PUT",
    body: JSON.stringify({ saved }),
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

export interface UnifiedSearchResult {
  id: string;
  kind: "message" | "direct" | "room" | "channel" | "team" | "user" | "meeting" | "recording" | "file" | "app" | string;
  title: string;
  snippet: string;
  source: string;
  url?: string | null;
  room_id?: string | null;
  team_id?: string | null;
  conference_id?: string | null;
  user_uri?: string | null;
  file_id?: string | null;
  app_id?: string | null;
  score: number;
  updated_at: string;
}

export interface CopilotCitation {
  index: number;
  result: UnifiedSearchResult;
}

export interface CopilotAnswer {
  question: string;
  generated_at: string;
  provider_configured: boolean;
  grounded: boolean;
  answer: string;
  citations: CopilotCitation[];
  suggested_prompts: string[];
  governance: string[];
}

export function paleServerUnifiedSearch(
  baseUrl: string,
  token: string,
  query: string,
  limit?: number,
): Promise<UnifiedSearchResult[]> {
  const params = new URLSearchParams({ q: query });
  if (limit) params.set("limit", String(limit));
  return serverFetch(baseUrl, token, `/v1/search?${params}`);
}

export function paleServerCopilotQuery(
  baseUrl: string,
  token: string,
  question: string,
  contextQuery?: string,
  limit = 8,
): Promise<CopilotAnswer> {
  return serverFetch(baseUrl, token, "/v1/copilot/query", {
    method: "POST",
    body: JSON.stringify({
      question,
      context_query: contextQuery || question,
      limit,
    }),
  });
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
): Promise<ServerMessageRead> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}/read`, {
    method: "PUT",
  });
}

export function paleServerGetMessageReads(
  baseUrl: string,
  token: string,
  messageId: string,
): Promise<ServerMessageRead[]> {
  return serverFetch(baseUrl, token, `/v1/messages/${messageId}/reads`);
}

// ─── Message Edit & Delete ───

export function paleServerEditMessage(
  baseUrl: string,
  token: string,
  messageId: string,
  body: string,
): Promise<ServerRoomMessage> {
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
    registrar_uri: string | null;
    registration_available: boolean;
    username: string;
    password: string;
    transport: string;
    domain: string;
  } | null;
  expires_at: string;
  /** When true, `token` is a short-lived mfa_pending token — complete MFA before using APIs. */
  mfa_required?: boolean;
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
  options: { roomId?: string; folderId?: string | null } = {},
): Promise<PaleServerFile> {
  const buffer = await file.arrayBuffer();
  const headers: Record<string, string> = {
    "Content-Type": file.type || "application/octet-stream",
    Authorization: `Bearer ${token}`,
    "X-Pale-Filename": file.name,
  };
  if (options.roomId) {
    headers["X-Pale-Room-Id"] = options.roomId;
  }
  if (options.folderId) {
    headers["X-Pale-Folder-Id"] = options.folderId;
  }
  const response = await paleFetch(`${baseUrl.replace(/\/+$/, "")}/v1/files`, {
    method: "POST",
    headers,
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

// ─── Custom Emojis ───

export function paleServerGetCustomEmojis(
  baseUrl: string,
  token: string,
  teamId: string,
): Promise<CustomEmoji[]> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/emojis`);
}

export function paleServerCreateCustomEmoji(
  baseUrl: string,
  token: string,
  teamId: string,
  shortcode: string,
  imageUrl: string,
): Promise<CustomEmoji> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/emojis`, {
    method: "POST",
    body: JSON.stringify({ shortcode, image_url: imageUrl }),
  });
}

export function paleServerDeleteCustomEmoji(
  baseUrl: string,
  token: string,
  teamId: string,
  emojiId: string,
): Promise<CustomEmoji> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/emojis/${emojiId}`, {
    method: "DELETE",
  });
}

// ─── Wiki Pages ───

export function paleServerGetWikiPages(
  baseUrl: string,
  token: string,
  teamId: string,
): Promise<WikiPage[]> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/wiki`);
}

export function paleServerCreateWikiPage(
  baseUrl: string,
  token: string,
  teamId: string,
  title: string,
  body?: string,
  parentId?: string | null,
): Promise<WikiPage> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/wiki`, {
    method: "POST",
    body: JSON.stringify({ title, body: body ?? "", parent_id: parentId ?? null }),
  });
}

export function paleServerGetWikiPage(
  baseUrl: string,
  token: string,
  pageId: string,
): Promise<WikiPage> {
  return serverFetch(baseUrl, token, `/v1/wiki/${pageId}`);
}

export function paleServerUpdateWikiPage(
  baseUrl: string,
  token: string,
  pageId: string,
  updates: { title?: string; body?: string; parent_id?: string | null },
): Promise<WikiPage> {
  return serverFetch(baseUrl, token, `/v1/wiki/${pageId}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export function paleServerDeleteWikiPage(
  baseUrl: string,
  token: string,
  pageId: string,
): Promise<WikiPage> {
  return serverFetch(baseUrl, token, `/v1/wiki/${pageId}`, {
    method: "DELETE",
  });
}

// ─── Task Boards & Tasks ───

export function paleServerGetTaskBoards(
  baseUrl: string,
  token: string,
  teamId: string,
): Promise<TaskBoard[]> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/boards`);
}

export function paleServerCreateTaskBoard(
  baseUrl: string,
  token: string,
  teamId: string,
  name: string,
): Promise<TaskBoard> {
  return serverFetch(baseUrl, token, `/v1/teams/${teamId}/boards`, {
    method: "POST",
    body: JSON.stringify({ name }),
  });
}

export function paleServerDeleteTaskBoard(
  baseUrl: string,
  token: string,
  boardId: string,
): Promise<TaskBoard> {
  return serverFetch(baseUrl, token, `/v1/boards/${boardId}`, {
    method: "DELETE",
  });
}

export function paleServerGetTasks(
  baseUrl: string,
  token: string,
  boardId: string,
): Promise<TaskItem[]> {
  return serverFetch(baseUrl, token, `/v1/boards/${boardId}/tasks`);
}

export function paleServerCreateTask(
  baseUrl: string,
  token: string,
  boardId: string,
  task: { title: string; description?: string; assignee?: string; status?: string; priority?: string; due_date?: string },
): Promise<TaskItem> {
  return serverFetch(baseUrl, token, `/v1/boards/${boardId}/tasks`, {
    method: "POST",
    body: JSON.stringify(task),
  });
}

export function paleServerUpdateTask(
  baseUrl: string,
  token: string,
  taskId: string,
  updates: { title?: string; description?: string; assignee?: string; status?: string; priority?: string; due_date?: string },
): Promise<TaskItem> {
  return serverFetch(baseUrl, token, `/v1/tasks/${taskId}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export function paleServerDeleteTask(
  baseUrl: string,
  token: string,
  taskId: string,
): Promise<TaskItem> {
  return serverFetch(baseUrl, token, `/v1/tasks/${taskId}`, {
    method: "DELETE",
  });
}

// ─── Inline Translation ───

export interface TranslateResult {
  translated_text: string;
  source_language?: string | null;
  target_language: string;
}

export function paleServerTranslate(
  baseUrl: string,
  token: string,
  text: string,
  targetLanguage: string,
): Promise<TranslateResult> {
  return serverFetch(baseUrl, token, "/v1/translate", {
    method: "POST",
    body: JSON.stringify({ text, target_language: targetLanguage }),
  });
}

// ─── USB HID Device Integration ───

export interface HidAudioDevice {
  name: string;
  device_type: string; // "headset" | "speaker" | "microphone"
  connected: boolean;
}

export function detectHidDevices(): Promise<HidAudioDevice[]> {
  return invoke("detect_hid_devices");
}

export function onHidHookSwitch(callback: (payload: { action: string }) => void): Promise<UnlistenFn> {
  return listen<{ action: string }>("hid_hook_switch", (event) => callback(event.payload));
}

export function onHidMuteToggle(callback: (payload: { muted: boolean }) => void): Promise<UnlistenFn> {
  return listen<{ muted: boolean }>("hid_mute_toggle", (event) => callback(event.payload));
}

// ─── Rate Limit Admin ───

export function paleServerGetRateLimits(
  baseUrl: string,
  token: string,
): Promise<ServerRateLimitConfig> {
  return serverFetch(baseUrl, token, "/v1/admin/rate-limits");
}

export function paleServerUpdateRateLimits(
  baseUrl: string,
  token: string,
  config: ServerRateLimitConfig,
): Promise<ServerRateLimitConfig> {
  return serverFetch(baseUrl, token, "/v1/admin/rate-limits", {
    method: "PUT",
    body: JSON.stringify(config),
  });
}
