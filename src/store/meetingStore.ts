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
  created_at: string;
}

interface MeetingStoreState {
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

  // Active conference ID for the current meeting
  activeConferenceId: string | null;
  setActiveConferenceId: (id: string | null) => void;
}

export const useMeetingStore = create<MeetingStoreState>((set) => ({
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
      meetings: [...state.meetings, meeting],
    })),

  activeConferenceId: null,
  setActiveConferenceId: (activeConferenceId) => set({ activeConferenceId }),
}));
