import { useEffect } from "react";
import { onMatrixAuthState, onMatrixRooms, onMatrixMessage } from "@/lib/tauri";
import { useMatrixStore } from "@/store/matrixStore";
import { useChatStore } from "@/store/chatStore";

/**
 * Listens to Matrix events from the Rust backend and updates stores.
 */
export function useMatrixEvents() {
  const setAuthState = useMatrixStore((s) => s.setAuthState);
  const { setRooms, addMessage } = useChatStore();

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    unlisteners.push(
      onMatrixAuthState((event: any) => {
        setAuthState(
          event.state as any,
          event.user_id ?? null,
          event.display_name ?? null
        );
      })
    );

    unlisteners.push(
      onMatrixRooms((event: any) => {
        if (event.rooms) {
          setRooms(event.rooms);
        }
      })
    );

    unlisteners.push(
      onMatrixMessage((msg: any) => {
        addMessage(msg);
      })
    );

    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [setAuthState, setRooms, addMessage]);
}
