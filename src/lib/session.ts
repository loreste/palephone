import { useServerStore } from "@/store/serverStore";
import { usePresenceStore } from "@/store/presenceStore";
import { useFileStore } from "@/store/fileStore";
import { useChatStore } from "@/store/chatStore";

/**
 * Single orchestrator for tearing down a pale-server session.
 *
 * serverStore.disconnect() only nulls its own fields, so every disconnect
 * path (Settings, Admin sign-out, token-refresh failure) must also clear the
 * stale server-derived state: presence dots, server rooms/messages, and
 * server files — otherwise the UI keeps showing data the user can no longer
 * interact with.
 */
export function disconnectServer() {
  sessionStorage.removeItem("pale.admin.token");
  useServerStore.getState().disconnect();
  usePresenceStore.getState().clearPresence();
  useFileStore.getState().setServerFiles([]);
  useChatStore.getState().clearServerData();
}
