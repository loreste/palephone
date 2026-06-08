export type Tab = "dialpad" | "chat" | "people" | "files" | "recent" | "admin" | "settings";

export type Theme = "dark" | "light";

export type RegState = "registered" | "registering" | "unregistered" | "none";

export type CallDirection = "inbound" | "outbound";

export type CallState =
  | "idle"
  | "dialing"
  | "ringing"
  | "early_media"
  | "connected"
  | "on_hold"
  | "transferring"
  | "terminated";

export interface SipAccount {
  id?: number;
  displayName: string;
  sipUri: string;
  registrarUri: string;
  authUsername: string;
  transport: "udp" | "tcp" | "tls";
}

export interface CallSession {
  id: number;
  direction: CallDirection;
  state: CallState;
  remoteUri: string;
  remoteName: string;
  startTime: number | null;
  connectTime: number | null;
  isMuted: boolean;
  isHeld: boolean;
  isRecording: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
  direction: "input" | "output";
  isDefault: boolean;
}

export interface RecentCall {
  id: string;
  direction: CallDirection;
  remoteUri: string;
  remoteName: string;
  startTime: number;
  durationSecs: number;
  answered: boolean;
}
