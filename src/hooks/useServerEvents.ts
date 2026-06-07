import { useEffect, useRef } from "react";
import { usePresenceStore, type UserPresence } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { useChatStore } from "@/store/chatStore";
import { paleServerGetPresence } from "@/lib/tauri";
import { adminRefreshToken } from "@/lib/adminApi";

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
          addMessage({
            event_id: msg.id,
            room_id: msg.room_id,
            sender: msg.sender_uri,
            sender_name: null,
            body: msg.body,
            msg_type: "text" as const,
            timestamp: Math.floor(new Date(msg.created_at).getTime() / 1000),
            is_encrypted: false,
            is_own: false,
          });
        } catch { /* ignore */ }
      });

      es.addEventListener("voicemail", () => {
        // Voicemail received — could update a badge or store
        import("@/lib/notifications").then(({ shouldNotify }) => {
          shouldNotify().then((ok) => {
            if (ok) {
              import("@/components/ui/Toast").then(({ toast }) => {
                toast({ type: "info", title: "New voicemail" });
              });
            }
          });
        });
      });

      es.addEventListener("recording", () => {
        // Recording completed notification
      });

      es.addEventListener("read_receipt", () => {
        // Read receipt received — could update message badges
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

  // Token auto-refresh
  useEffect(() => {
    if (!baseUrl || !token || !tokenExpiresAt) return;

    const expiresMs = new Date(tokenExpiresAt).getTime();
    const refreshAt = expiresMs - TOKEN_REFRESH_BUFFER_MS;
    const delayMs = Math.max(refreshAt - Date.now(), 0);

    refreshRef.current = window.setTimeout(async () => {
      try {
        const session = await adminRefreshToken(baseUrl, token);
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
