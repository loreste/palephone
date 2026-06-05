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

// ─── Video Commands ───

export function makeVideoCall(uri: string): Promise<void> {
  return invoke("make_video_call", { uri });
}

export function toggleVideo(callId: number, enabled: boolean): Promise<void> {
  return invoke("toggle_video", { callId, enabled });
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
export function onMatrixAuthState(handler: (event: any) => void): Promise<UnlistenFn> {
  return listen("matrix://auth-state", (e) => handler(e.payload));
}

export function onMatrixRooms(handler: (event: any) => void): Promise<UnlistenFn> {
  return listen("matrix://rooms", (e) => handler(e.payload));
}

export function onMatrixMessage(handler: (event: any) => void): Promise<UnlistenFn> {
  return listen("matrix://message", (e) => handler(e.payload));
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
