import { create } from "zustand";

export type PresenceStatus = "online" | "offline" | "busy" | "away" | "dnd" | "on_call";

export interface UserPresence {
  sip_uri: string;
  status: PresenceStatus;
  note: string | null;
  updated_at: string;
}

interface PresenceStoreState {
  presenceMap: Record<string, UserPresence>;

  setPresence: (sipUri: string, presence: UserPresence) => void;
  setBulkPresence: (list: UserPresence[]) => void;
  clearPresence: () => void;
}

export const usePresenceStore = create<PresenceStoreState>((set) => ({
  presenceMap: {},

  setPresence: (sipUri, presence) =>
    set((state) => ({
      presenceMap: { ...state.presenceMap, [sipUri]: presence },
    })),

  setBulkPresence: (list) =>
    set(() => {
      const map: Record<string, UserPresence> = {};
      for (const p of list) {
        map[p.sip_uri] = p;
      }
      return { presenceMap: map };
    }),

  clearPresence: () => set({ presenceMap: {} }),
}));
