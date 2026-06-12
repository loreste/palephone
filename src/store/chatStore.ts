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
  msg_type:
    | "text"
    | "emote"
    | "notice"
    | { image: { url: string; thumbnail_url: string | null; width: number | null; height: number | null } }
    | { file: { url: string; filename: string; size: number | null; mimetype: string | null } }
    | { audio: { url: string; duration_ms: number | null } }
    | { video: { url: string; duration_ms: number | null; width: number | null; height: number | null } };
  timestamp: number;
  is_encrypted: boolean;
  is_own: boolean;
}

interface ChatStoreState {
  rooms: RoomSummary[];
  activeRoomId: string | null;
  messages: Record<string, ChatMessage[]>; // room_id -> messages
  typingByRoom: Record<string, string[]>;

  setRooms: (rooms: RoomSummary[]) => void;
  setActiveRoomId: (id: string | null) => void;
  addMessage: (msg: ChatMessage) => void;
  setMessages: (roomId: string, msgs: ChatMessage[]) => void;
  removeMessage: (roomId: string, eventId: string) => void;
  setTypingUsers: (roomId: string, userIds: string[]) => void;
  clearServerData: () => void;
}

/** Pale-server room ids are UUIDs; Matrix room ids start with "!". */
export function isServerRoomId(roomId: string): boolean {
  return !roomId.startsWith("!");
}

export const useChatStore = create<ChatStoreState>((set) => ({
  rooms: [],
  activeRoomId: null,
  messages: {},
  typingByRoom: {},

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

  removeMessage: (roomId, eventId) =>
    set((state) => ({
      messages: {
        ...state.messages,
        [roomId]: (state.messages[roomId] ?? []).filter((m) => m.event_id !== eventId),
      },
    })),

  setTypingUsers: (roomId, userIds) =>
    set((state) => ({
      typingByRoom: { ...state.typingByRoom, [roomId]: userIds },
    })),

  // Drop all pale-server rooms/messages (e.g. on server disconnect),
  // keeping Matrix data intact.
  clearServerData: () =>
    set((state) => ({
      rooms: state.rooms.filter((r) => !isServerRoomId(r.room_id)),
      messages: Object.fromEntries(
        Object.entries(state.messages).filter(([roomId]) => !isServerRoomId(roomId))
      ),
      typingByRoom: Object.fromEntries(
        Object.entries(state.typingByRoom).filter(([roomId]) => !isServerRoomId(roomId))
      ),
      activeRoomId:
        state.activeRoomId && isServerRoomId(state.activeRoomId) ? null : state.activeRoomId,
    })),
}));
