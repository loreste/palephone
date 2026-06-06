import { useEffect } from "react";
import { onMatrixAuthState, onMatrixRooms, onMatrixMessage, onMatrixTyping } from "@/lib/tauri";
import { useMatrixStore, type MatrixAuthState } from "@/store/matrixStore";
import { useChatStore, type ChatMessage, type RoomSummary } from "@/store/chatStore";

interface MatrixAuthEvent {
  state: MatrixAuthState;
  user_id?: string | null;
  display_name?: string | null;
}

interface MatrixRoomsEvent {
  rooms: RoomSummary[];
}

interface MatrixTypingEvent {
  room_id: string;
  user_ids: string[];
}

/**
 * Listens to Matrix events from the Rust backend and updates stores.
 */
export function useMatrixEvents() {
  const setAuthState = useMatrixStore((s) => s.setAuthState);
  const { setRooms, addMessage, setTypingUsers } = useChatStore();

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    unlisteners.push(
      onMatrixAuthState((event: unknown) => {
        const e = event as MatrixAuthEvent;
        setAuthState(e.state, e.user_id ?? null, e.display_name ?? null);
      })
    );

    unlisteners.push(
      onMatrixRooms((event: unknown) => {
        const e = event as MatrixRoomsEvent;
        if (e.rooms) {
          setRooms(e.rooms);
        }
      })
    );

    unlisteners.push(
      onMatrixMessage((msg: unknown) => {
        addMessage(msg as ChatMessage);
      })
    );

    unlisteners.push(
      onMatrixTyping((event: unknown) => {
        const e = event as MatrixTypingEvent;
        setTypingUsers(e.room_id, e.user_ids ?? []);
      })
    );

    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [setAuthState, setRooms, addMessage, setTypingUsers]);
}
