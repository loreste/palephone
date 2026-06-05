import { create } from "zustand";
import type { CallSession, CallState } from "@/types";

interface CallStoreState {
  sessions: CallSession[];
  activeCallId: number | null;
  incomingCall: CallSession | null;

  addSession: (session: CallSession) => void;
  removeSession: (id: number) => void;
  updateSessionState: (id: number, state: CallState) => void;
  setMuted: (id: number, muted: boolean) => void;
  setHeld: (id: number, held: boolean) => void;
  setActiveCallId: (id: number | null) => void;
  setIncomingCall: (session: CallSession | null) => void;
  setConnectTime: (id: number, time: number) => void;
  clearAll: () => void;
}

export const useCallStore = create<CallStoreState>((set) => ({
  sessions: [],
  activeCallId: null,
  incomingCall: null,

  addSession: (session) =>
    set((state) => ({ sessions: [...state.sessions, session] })),

  removeSession: (id) =>
    set((state) => ({
      sessions: state.sessions.filter((s) => s.id !== id),
      activeCallId: state.activeCallId === id ? null : state.activeCallId,
    })),

  updateSessionState: (id, callState) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, state: callState } : s
      ),
    })),

  setMuted: (id, muted) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, isMuted: muted } : s
      ),
    })),

  setHeld: (id, held) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, isHeld: held } : s
      ),
    })),

  setActiveCallId: (id) => set({ activeCallId: id }),

  setIncomingCall: (session) => set({ incomingCall: session }),

  setConnectTime: (id, time) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, connectTime: time } : s
      ),
    })),

  clearAll: () => set({ sessions: [], activeCallId: null, incomingCall: null }),
}));
