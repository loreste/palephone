import { useServerStore } from "@/store/serverStore";
import { usePresenceStore } from "@/store/presenceStore";
import { useFileStore } from "@/store/fileStore";
import { useChatStore } from "@/store/chatStore";
import { deleteSipPassword } from "@/lib/tauri";

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

/**
 * Full sign-out: clears saved credentials, session storage, localStorage
 * setup flag, and reloads the app so the setup wizard appears.
 */
export function signOut() {
  // Clear server session
  disconnectServer();
  // Remove saved keychain password so auto-login won't fire
  deleteSipPassword("pale-server-login").catch(() => {});
  // Clear the setup-complete flag so the wizard shows on reload
  localStorage.removeItem("pale.setup_complete");
  // Reload the app to reset all state cleanly
  window.location.reload();
}
