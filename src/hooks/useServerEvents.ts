import { useEffect, useRef } from "react";
import { usePresenceStore, type UserPresence } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { useChatStore } from "@/store/chatStore";
import { useAccountStore } from "@/store/accountStore";
import { useActivityStore } from "@/store/activityStore";
import { paleServerGetPresence } from "@/lib/tauri";
import { adminRefreshToken } from "@/lib/adminApi";
import { shouldNotify, shouldPlaySound } from "@/lib/notifications";
import { playNotificationBeep } from "@/lib/notificationSound";
import { toast } from "@/components/ui/Toast";

const RECONNECT_DELAY_MS = 3000;
const TOKEN_REFRESH_BUFFER_MS = 30 * 60 * 1000; // Refresh 30 min before expiry

/**
 * Connects to the pale-server SSE endpoint for real-time events.
 * Updates presenceStore and chatStore on incoming events.
 * Auto-refreshes the admin token before expiry.
 */
export function useServerEvents(baseUrl: string | null, token: string | null) {
  const setPresence = usePresenceStore((s) => s.setPresence);
  const setBulkPresence = usePresenceStore((s) => s.setBulkPresence);
  const addMessage = useChatStore((s) => s.addMessage);
  const addActivity = useActivityStore((s) => s.addItem);
  const updateToken = useServerStore((s) => s.updateToken);
  const tokenExpiresAt = useServerStore((s) => s.tokenExpiresAt);
  const disconnect = useServerStore((s) => s.disconnect);
  const sourceRef = useRef<EventSource | null>(null);
  const reconnectRef = useRef<number | null>(null);
  const refreshRef = useRef<number | null>(null);

  // SSE connection
  useEffect(() => {
    if (!baseUrl || !token) return;

    paleServerGetPresence(baseUrl, token)
      .then(setBulkPresence)
      .catch(() => {});

    const connect = () => {
      const url = `${baseUrl.replace(/\/+$/, "")}/v1/events`;
      const es = new EventSource(`${url}?token=${encodeURIComponent(token)}`);
      sourceRef.current = es;

      es.addEventListener("presence", (e) => {
        try {
          const presence: UserPresence = JSON.parse(e.data);
          setPresence(presence.sip_uri, presence);
        } catch { /* ignore */ }
      });

      es.addEventListener("message", (e) => {
        try {
          const msg = JSON.parse(e.data);
          if (msg.room_id && msg.event_id) {
            addMessage(msg);
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("room_message", (e) => {
        try {
          const msg = JSON.parse(e.data);
          const currentSipUri = useAccountStore.getState().account?.sipUri;
          const isOwn = currentSipUri != null && msg.sender_uri === currentSipUri;
          addMessage({
            event_id: msg.id,
            room_id: msg.room_id,
            sender: msg.sender_uri,
            sender_name: null,
            body: msg.body,
            msg_type: "text" as const,
            timestamp: Math.floor(new Date(msg.created_at).getTime() / 1000),
            is_encrypted: false,
            is_own: isOwn,
          });
          if (!isOwn) {
            const senderLabel = msg.sender_uri?.replace(/^sip:/, "") ?? "Someone";
            const preview = msg.body?.length > 50 ? msg.body.slice(0, 50) + "..." : msg.body;

            // Check for @mention of current user
            const displayName = useAccountStore.getState().account?.displayName;
            if (displayName && msg.body && msg.body.includes(`@${displayName}`)) {
              addActivity({
                id: `mention-${msg.id ?? Date.now()}`,
                type: "mention",
                title: `${senderLabel} mentioned you`,
                body: preview,
                timestamp: Math.floor(Date.now() / 1000),
                read: false,
                room_id: msg.room_id,
              });
            }

            shouldNotify(msg.room_id).then((ok) => {
              if (ok) {
                toast({ type: "info", title: senderLabel, description: preview });
              }
            });
            shouldPlaySound().then((ok) => {
              if (ok) playNotificationBeep();
            });
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("voicemail", () => {
        shouldNotify().then((ok) => {
          if (ok) {
            toast({ type: "info", title: "New voicemail" });
          }
        });
        shouldPlaySound().then((ok) => {
          if (ok) playNotificationBeep();
        });
      });

      es.addEventListener("recording", () => {
        // Recording completed notification
      });

      es.addEventListener("read_receipt", () => {
        // Read receipt received — could update message badges
      });

      es.addEventListener("user_created", () => {
        // Trigger admin panel refresh via custom event
        window.dispatchEvent(new CustomEvent("pale:admin-refresh"));
      });

      es.onerror = () => {
        es.close();
        sourceRef.current = null;
        reconnectRef.current = window.setTimeout(connect, RECONNECT_DELAY_MS);
      };
    };

    connect();

    return () => {
      if (sourceRef.current) {
        sourceRef.current.close();
        sourceRef.current = null;
      }
      if (reconnectRef.current) {
        window.clearTimeout(reconnectRef.current);
        reconnectRef.current = null;
      }
    };
  }, [baseUrl, token, setPresence, setBulkPresence, addMessage]);

  // Token auto-refresh (with stale-token guard to prevent race conditions)
  useEffect(() => {
    if (!baseUrl || !token || !tokenExpiresAt) return;

    const expiresMs = new Date(tokenExpiresAt).getTime();
    const refreshAt = expiresMs - TOKEN_REFRESH_BUFFER_MS;
    const delayMs = Math.max(refreshAt - Date.now(), 0);
    const currentToken = token; // Capture token at effect time

    refreshRef.current = window.setTimeout(async () => {
      // Guard: only refresh if the token hasn't changed since this timer was set
      const storeToken = useServerStore.getState().token;
      if (storeToken !== currentToken) return; // Token already refreshed by another path

      try {
        const session = await adminRefreshToken(baseUrl, currentToken);
        sessionStorage.setItem("pale.admin.token", session.token);
        updateToken(session.token, session.expires_at);
      } catch {
        disconnect();
      }
    }, delayMs);

    return () => {
      if (refreshRef.current) {
        window.clearTimeout(refreshRef.current);
        refreshRef.current = null;
      }
    };
  }, [baseUrl, token, tokenExpiresAt, updateToken, disconnect]);
}
