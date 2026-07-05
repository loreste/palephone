import { create } from "zustand";

export interface LobbyParticipant {
  user_id: string;
  sip_uri: string;
  display_name: string;
  state: "waiting" | "admitted" | "rejected";
  requested_at: string;
}

export interface ConferenceLobby {
  conference_id: string;
  enabled: boolean;
  participants: LobbyParticipant[];
}

export interface ConferenceParticipant {
  user_id: string;
  sip_uri: string;
  role: "host" | "moderator" | "member";
  bridge_slot?: number | null;
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
  spotlight_participant_id?: string | null;
  green_room_enabled?: boolean;
  chat_room_id?: string | null;
  livekit_room?: string | null;
  livekit_egress_id?: string | null;
}

/** Extended join response that may include LiveKit credentials. */
export interface JoinConferenceResponse extends ConferenceSummary {
  livekit_url?: string | null;
  livekit_token?: string | null;
}

export interface MeetingTemplate {
  id: string;
  name: string;
  description: string;
  default_lobby: boolean;
  default_mute_on_join: boolean;
  default_allow_reactions: boolean;
  default_recording: boolean;
  max_participants: number | null;
  allowed_roles: string[];
  created_at: string;
  created_by: string;
}

export interface MeetingReaction {
  user_id: string;
  user_name: string;
  emoji: string;
  timestamp: string;
}

export interface GreenRoomParticipant {
  user_id: string;
  sip_uri: string;
  ready: boolean;
  joined_at: string;
}

export interface GreenRoomState {
  conference_id: string;
  enabled: boolean;
  participants: GreenRoomParticipant[];
}

export interface OutOfOfficeSettings {
  message: string | null;
  until: string | null;
}

export interface ConferenceAttendanceRecord {
  id: string;
  conference_id: string;
  user_id: string;
  sip_uri: string;
  role: "host" | "moderator" | "member";
  joined_at: string;
  left_at?: string | null;
  duration_secs?: number | null;
  leave_reason?: "left" | "removed" | "ended" | null;
  removed_by?: string | null;
}

export interface HandRaise {
  user_id: string;
  sip_uri: string;
  raised_at: string;
}

export interface PollOption {
  id: string;
  text: string;
  votes: string[];
}

export interface MeetingPoll {
  id: string;
  conference_id: string;
  question: string;
  options: PollOption[];
  status: "draft" | "active" | "closed";
  anonymous: boolean;
  multi_select: boolean;
  created_by: string;
  created_at: string;
}

export interface QaQuestion {
  id: string;
  conference_id: string;
  text: string;
  asked_by: string;
  upvotes: string[];
  answered: boolean;
  answer: string | null;
  created_at: string;
}

export interface BreakoutRoom {
  id: string;
  name: string;
  participants: string[];
}

export interface BreakoutSession {
  id: string;
  conference_id: string;
  rooms: BreakoutRoom[];
  status: "pending" | "active" | "closed";
  duration_secs: number | null;
  created_at: string;
}

export interface TranscriptSegment {
  id: string;
  conference_id: string;
  speaker_uri: string;
  speaker_name: string;
  text: string;
  timestamp: string;
  is_final: boolean;
}

export interface ScheduledMeeting {
  id: string;
  title: string;
  description: string;
  organizer_uri: string;
  room_id: string | null;
  conference_id: string | null;
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

interface MeetingStoreState {
  conferences: Record<string, ConferenceSummary>;
  setConference: (conference: ConferenceSummary) => void;

  // Lobby
  lobby: ConferenceLobby | null;
  setLobby: (lobby: ConferenceLobby) => void;

  // Raised hands
  raisedHands: HandRaise[];
  setRaisedHands: (hands: HandRaise[]) => void;

  // Polls
  polls: MeetingPoll[];
  setPolls: (polls: MeetingPoll[]) => void;
  upsertPoll: (poll: MeetingPoll) => void;

  // Q&A
  questions: QaQuestion[];
  setQuestions: (questions: QaQuestion[]) => void;
  upsertQuestion: (question: QaQuestion) => void;

  // Breakout rooms
  breakouts: BreakoutSession[];
  setBreakouts: (sessions: BreakoutSession[]) => void;
  upsertBreakout: (session: BreakoutSession) => void;

  // Live captions
  captions: TranscriptSegment[];
  captionsEnabled: boolean;
  addCaption: (segment: TranscriptSegment) => void;
  setCaptionsEnabled: (enabled: boolean) => void;
  clearCaptions: () => void;

  // Scheduled meetings (for calendar)
  meetings: ScheduledMeeting[];
  setMeetings: (meetings: ScheduledMeeting[]) => void;
  addMeeting: (meeting: ScheduledMeeting) => void;
  upsertMeeting: (meeting: ScheduledMeeting) => void;

  // Active conference ID for the current meeting
  activeConferenceId: string | null;
  setActiveConferenceId: (id: string | null) => void;

  // LiveKit media session (set when joining a conference with LiveKit)
  livekitUrl: string | null;
  livekitToken: string | null;
  setLiveKitCredentials: (url: string | null, token: string | null) => void;

  // Meeting reactions (ephemeral floating reactions)
  reactions: MeetingReaction[];
  addReaction: (reaction: MeetingReaction) => void;

  // Green room
  greenRoom: GreenRoomState | null;
  setGreenRoom: (state: GreenRoomState) => void;

  // Meeting templates
  templates: MeetingTemplate[];
  setTemplates: (templates: MeetingTemplate[]) => void;
}

export const useMeetingStore = create<MeetingStoreState>((set) => ({
  conferences: {},
  setConference: (conference) => set((state) => ({
    conferences: { ...state.conferences, [conference.id]: conference },
  })),

  lobby: null,
  setLobby: (lobby) => set({ lobby }),

  raisedHands: [],
  setRaisedHands: (raisedHands) => set({ raisedHands }),

  polls: [],
  setPolls: (polls) => set({ polls }),
  upsertPoll: (poll) =>
    set((state) => ({
      polls: state.polls.some((p) => p.id === poll.id)
        ? state.polls.map((p) => (p.id === poll.id ? poll : p))
        : [...state.polls, poll],
    })),

  questions: [],
  setQuestions: (questions) => set({ questions }),
  upsertQuestion: (question) =>
    set((state) => ({
      questions: state.questions.some((q) => q.id === question.id)
        ? state.questions.map((q) => (q.id === question.id ? question : q))
        : [...state.questions, question],
    })),

  breakouts: [],
  setBreakouts: (breakouts) => set({ breakouts }),
  upsertBreakout: (session) =>
    set((state) => ({
      breakouts: state.breakouts.some((b) => b.id === session.id)
        ? state.breakouts.map((b) => (b.id === session.id ? session : b))
        : [...state.breakouts, session],
    })),

  captions: [],
  captionsEnabled: false,
  addCaption: (segment) =>
    set((state) => ({
      captions: [...state.captions.slice(-200), segment],
    })),
  setCaptionsEnabled: (captionsEnabled) => set({ captionsEnabled }),
  clearCaptions: () => set({ captions: [] }),

  meetings: [],
  setMeetings: (meetings) => set({ meetings }),
  addMeeting: (meeting) =>
    set((state) => ({
      meetings: state.meetings.some((m) => m.id === meeting.id)
        ? state.meetings.map((m) => (m.id === meeting.id ? meeting : m))
        : [...state.meetings, meeting],
    })),
  upsertMeeting: (meeting) =>
    set((state) => ({
      meetings: state.meetings.some((m) => m.id === meeting.id)
        ? state.meetings.map((m) => (m.id === meeting.id ? meeting : m))
        : [...state.meetings, meeting],
    })),

  activeConferenceId: null,
  setActiveConferenceId: (activeConferenceId) => set({ activeConferenceId }),

  livekitUrl: null,
  livekitToken: null,
  setLiveKitCredentials: (livekitUrl, livekitToken) => set({ livekitUrl, livekitToken }),

  reactions: [],
  addReaction: (reaction) =>
    set((state) => ({
      reactions: [...state.reactions.slice(-50), reaction],
    })),

  greenRoom: null,
  setGreenRoom: (greenRoom) => set({ greenRoom }),

  templates: [],
  setTemplates: (templates) => set({ templates }),
}));
