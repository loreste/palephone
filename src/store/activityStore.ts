import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface ActivityItem {
  id: string;
  type: "message" | "mention" | "missed_call" | "system";
  title: string;
  body: string;
  timestamp: number;
  read: boolean;
  room_id?: string;
}

interface ActivityStoreState {
  items: ActivityItem[];
  addItem: (item: ActivityItem) => void;
  markRead: (id: string) => void;
  markAllRead: () => void;
  clearAll: () => void;
}

export const useActivityStore = create<ActivityStoreState>()(
  persist(
    (set) => ({
      items: [],

      addItem: (item) =>
        set((state) => ({
          items: [item, ...state.items].slice(0, 200),
        })),

      markRead: (id) =>
        set((state) => ({
          items: state.items.map((i) => (i.id === id ? { ...i, read: true } : i)),
        })),

      markAllRead: () =>
        set((state) => ({
          items: state.items.map((i) => ({ ...i, read: true })),
        })),

      clearAll: () => set({ items: [] }),
    }),
    {
      name: "pale-activity",
    }
  )
);
