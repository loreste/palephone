import { create } from "zustand";

export interface RoomSummary {
  room_id: string;
  name: string;
  is_direct: boolean;
  is_encrypted: boolean;
  last_message: string | null;
  last_message_sender: string | null;
  last_message_ts: number | null;
  unread_count: number;
}

export interface ChatMessage {
  event_id: string;
  room_id: string;
  sender: string;
  sender_name: string | null;
  body: string;
  msg_type: "text" | "image" | "file" | "audio" | "video" | "emote" | "notice";
  timestamp: number;
  is_encrypted: boolean;
  is_own: boolean;
}

interface ChatStoreState {
  rooms: RoomSummary[];
  activeRoomId: string | null;
  messages: Record<string, ChatMessage[]>; // room_id -> messages

  setRooms: (rooms: RoomSummary[]) => void;
  setActiveRoomId: (id: string | null) => void;
  addMessage: (msg: ChatMessage) => void;
  setMessages: (roomId: string, msgs: ChatMessage[]) => void;
}

export const useChatStore = create<ChatStoreState>((set) => ({
  rooms: [],
  activeRoomId: null,
  messages: {},

  setRooms: (rooms) => set({ rooms }),
  setActiveRoomId: (id) => set({ activeRoomId: id }),

  addMessage: (msg) =>
    set((state) => {
      const existing = state.messages[msg.room_id] ?? [];
      // Avoid duplicates
      if (existing.some((m) => m.event_id === msg.event_id)) return state;
      return {
        messages: {
          ...state.messages,
          [msg.room_id]: [...existing, msg],
        },
        // Update room's last message
        rooms: state.rooms.map((r) =>
          r.room_id === msg.room_id
            ? { ...r, last_message: msg.body, last_message_ts: msg.timestamp }
            : r
        ),
      };
    }),

  setMessages: (roomId, msgs) =>
    set((state) => ({
      messages: { ...state.messages, [roomId]: msgs },
    })),
}));
