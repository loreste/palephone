import { useEffect, useRef } from "react";
import { usePresenceStore, type UserPresence } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { useChatStore } from "@/store/chatStore";
import { useAccountStore } from "@/store/accountStore";
import { useActivityStore } from "@/store/activityStore";
import { useMeetingStore } from "@/store/meetingStore";
import { paleServerGetPresence } from "@/lib/tauri";
import { adminRefreshToken } from "@/lib/adminApi";
import { shouldNotify, shouldPlaySound } from "@/lib/notifications";
import { playNotificationBeep } from "@/lib/notificationSound";
import { toast } from "@/components/ui/Toast";

function desktopNotify(title: string, body?: string) {
  if (typeof Notification === "undefined") return;
  if (Notification.permission === "granted") {
    new Notification(title, { body });
  } else if (Notification.permission !== "denied") {
    Notification.requestPermission().then((perm) => {
      if (perm === "granted") new Notification(title, { body });
    });
  }
}

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
  const updateMessage = useChatStore((s) => s.updateMessage);
  const removeMessage = useChatStore((s) => s.removeMessage);
  const upsertRoom = useChatStore((s) => s.upsertRoom);
  const setTypingUsers = useChatStore((s) => s.setTypingUsers);
  const setOffline = useChatStore((s) => s.setOffline);
  const addActivity = useActivityStore((s) => s.addItem);
  const updateToken = useServerStore((s) => s.updateToken);
  const tokenExpiresAt = useServerStore((s) => s.tokenExpiresAt);
  const disconnect = useServerStore((s) => s.disconnect);
  const sourceRef = useRef<EventSource | null>(null);
  const reconnectRef = useRef<number | null>(null);
  const refreshRef = useRef<number | null>(null);
  const typingTimeoutsRef = useRef<Map<string, number>>(new Map());

  // SSE connection
  useEffect(() => {
    if (!baseUrl || !token) return;
    const typingTimeouts = typingTimeoutsRef.current;

    paleServerGetPresence(baseUrl, token)
      .then(setBulkPresence)
      .catch(() => {});

    const connect = () => {
      const url = `${baseUrl.replace(/\/+$/, "")}/v1/events`;
      const es = new EventSource(`${url}?token=${encodeURIComponent(token)}`);
      sourceRef.current = es;

      es.onopen = () => {
        setOffline(false);
        // Flush any queued messages on reconnect
        const queued = useChatStore.getState().flushQueue();
        if (queued.length > 0 && baseUrl && token) {
          for (const msg of queued) {
            fetch(`${baseUrl.replace(/\/+$/, "")}/v1/rooms/${msg.room_id}/messages`, {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                Authorization: `Bearer ${token}`,
              },
              body: JSON.stringify({ body: msg.body }),
            })
              .then((res) => {
                if (res.ok) {
                  useChatStore.getState().removeFromQueue(msg.id);
                }
              })
              .catch(() => {
                // Message stays in queue for next reconnect attempt
              });
          }
        }
      };

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

      es.addEventListener("typing", (e) => {
        try {
          const data = JSON.parse(e.data);
          const currentSipUri = useAccountStore.getState().account?.sipUri;
          if (data.room_id && data.user !== currentSipUri) {
            const roomId = data.room_id;
            const key = `${roomId}:${data.user}`;
            const existing = useChatStore.getState().typingByRoom[roomId] ?? [];
            const existingTimeout = typingTimeouts.get(key);
            if (existingTimeout) {
              window.clearTimeout(existingTimeout);
              typingTimeouts.delete(key);
            }
            if (data.typing) {
              if (!existing.includes(data.user)) {
                setTypingUsers(roomId, [...existing, data.user]);
              }
              const timeout = window.setTimeout(() => {
                const latest = useChatStore.getState().typingByRoom[roomId] ?? [];
                setTypingUsers(roomId, latest.filter((u: string) => u !== data.user));
                typingTimeouts.delete(key);
              }, 3500);
              typingTimeouts.set(key, timeout);
            } else {
              setTypingUsers(roomId, existing.filter((u: string) => u !== data.user));
            }
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("room_created", (e) => {
        try {
          const room = JSON.parse(e.data);
          const currentSipUri = useAccountStore.getState().account?.sipUri;
          if (!currentSipUri || !Array.isArray(room.members)) return;
          const isMember = room.members.some(
            (member: { user_sip_uri?: string }) => member.user_sip_uri === currentSipUri
          );
          if (!isMember) return;
          const otherMember = room.is_direct
            ? room.members.find(
                (member: { user_sip_uri?: string }) => member.user_sip_uri !== currentSipUri
              )
            : null;
          upsertRoom({
            room_id: room.id,
            name: otherMember?.user_sip_uri?.replace(/^sip:/, "") ?? room.name,
            is_direct: Boolean(room.is_direct),
            is_encrypted: false,
            last_message: null,
            last_message_sender: null,
            last_message_ts: null,
            unread_count: 0,
          });
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
            priority: msg.priority ?? "normal",
            saved_by: msg.saved_by ?? [],
            mentions: msg.mentions ?? [],
            mentioned_user_uris: msg.mentioned_user_uris ?? [],
            delivery_status: msg.delivery_status ?? "sent",
            scheduled_at: msg.scheduled_at,
          });
          if (!isOwn) {
            const senderLabel = msg.sender_uri?.replace(/^sip:/, "") ?? "Someone";
            const preview = msg.body?.length > 50 ? msg.body.slice(0, 50) + "..." : msg.body;
            const priorityPrefix = msg.priority === "urgent" ? "Urgent: " : msg.priority === "high" ? "High priority: " : "";

            // Increment unread count if the message is not for the active room
            const activeRoomId = useChatStore.getState().activeRoomId;
            if (msg.room_id !== activeRoomId) {
              const rooms = useChatStore.getState().rooms;
              const updatedRooms = rooms.map((r) =>
                r.room_id === msg.room_id
                  ? { ...r, unread_count: r.unread_count + 1 }
                  : r
              );
              useChatStore.getState().setRooms(updatedRooms);
            }

            // Check structured mention targets instead of substring matching.
            if (currentSipUri && Array.isArray(msg.mentioned_user_uris) && msg.mentioned_user_uris.includes(currentSipUri)) {
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
                toast({ type: msg.priority === "urgent" ? "warning" : "info", title: `${priorityPrefix}${senderLabel}`, description: preview });
                desktopNotify(`${priorityPrefix}${senderLabel}`, preview);
              }
            });
            shouldPlaySound().then((ok) => {
              if (ok) playNotificationBeep();
            });
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("thread_reply", (e) => {
        try {
          const data = JSON.parse(e.data);
          const msg = data.message;
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
            thread_id: data.thread?.id ?? null,
            priority: msg.priority ?? "normal",
            delivery_status: msg.delivery_status ?? "sent",
          });
        } catch { /* ignore */ }
      });

      es.addEventListener("message_edited", (e) => {
        try {
          const msg = JSON.parse(e.data);
          updateMessage(msg.room_id, msg.id, {
            body: msg.body,
            edited_at: msg.edited_at ? Math.floor(new Date(msg.edited_at).getTime() / 1000) : undefined,
            mentions: msg.mentions ?? [],
            mentioned_user_uris: msg.mentioned_user_uris ?? [],
          });
        } catch { /* ignore */ }
      });

      es.addEventListener("message_pinned", (e) => {
        try {
          const msg = JSON.parse(e.data);
          updateMessage(msg.room_id, msg.id, { pinned: msg.pinned });
        } catch { /* ignore */ }
      });

      es.addEventListener("message_saved", (e) => {
        try {
          const msg = JSON.parse(e.data);
          updateMessage(msg.room_id, msg.id, { saved_by: msg.saved_by ?? [] });
        } catch { /* ignore */ }
      });

      es.addEventListener("message_deleted", (e) => {
        try {
          const payload = JSON.parse(e.data);
          const roomId = payload.room_id;
          if (roomId && payload.message_id) {
            removeMessage(roomId, payload.message_id);
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("scheduled_message_delivered", (e) => {
        try {
          const msg = JSON.parse(e.data);
          const currentSipUri = useAccountStore.getState().account?.sipUri;
          const isOwn = currentSipUri != null && msg.sender_uri === currentSipUri;
          // The scheduled message is now delivered — add it as a regular message
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
            delivery_status: "sent",
          });
          if (isOwn) {
            toast({ type: "success", title: "Scheduled message delivered" });
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("room_call_started", (e) => {
        try {
          const target = JSON.parse(e.data);
          if (!target.room_id || !target.call_uri || !target.conference_id) return;
          const existing = useChatStore.getState().rooms.find((room) => room.room_id === target.room_id);
          if (!existing) return;
          upsertRoom({
            ...existing,
            call_uri: target.call_uri,
            conference_id: target.conference_id,
          });
        } catch { /* ignore */ }
      });

      es.addEventListener("room_call_ended", (e) => {
        try {
          const ended = JSON.parse(e.data);
          if (!ended.room_id) return;
          const existing = useChatStore.getState().rooms.find((room) => room.room_id === ended.room_id);
          if (!existing) return;
          upsertRoom({
            ...existing,
            call_uri: null,
            conference_id: null,
          });
        } catch { /* ignore */ }
      });

      es.addEventListener("meeting_scheduled", (e) => {
        try {
          const meeting = JSON.parse(e.data);
          useMeetingStore.getState().upsertMeeting(meeting);
          window.dispatchEvent(new CustomEvent("pale:meeting-scheduled", { detail: meeting }));
        } catch { /* ignore */ }
      });

      es.addEventListener("meeting_updated", (e) => {
        try {
          const meeting = JSON.parse(e.data);
          useMeetingStore.getState().upsertMeeting(meeting);
          window.dispatchEvent(new CustomEvent("pale:meeting-updated", { detail: meeting }));
        } catch { /* ignore */ }
      });

      es.addEventListener("meeting_cancelled", (e) => {
        try {
          const meeting = JSON.parse(e.data);
          useMeetingStore.getState().upsertMeeting(meeting);
          window.dispatchEvent(new CustomEvent("pale:meeting-cancelled", { detail: meeting }));
        } catch { /* ignore */ }
      });

      es.addEventListener("spotlight_changed", (e) => {
        try {
          const payload = JSON.parse(e.data);
          const conferenceId = payload.conference_id;
          if (!conferenceId) return;
          const conf = useMeetingStore.getState().conferences[conferenceId];
          if (conf) {
            useMeetingStore.getState().setConference({
              ...conf,
              spotlight_participant_id: payload.participant_id ?? null,
            });
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("meeting_reaction", (e) => {
        try {
          const payload = JSON.parse(e.data);
          if (payload.reaction) {
            useMeetingStore.getState().addReaction(payload.reaction);
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("green_room_updated", (e) => {
        try {
          const state = JSON.parse(e.data);
          if (state.conference_id) {
            useMeetingStore.getState().setGreenRoom(state);
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("reaction", (e) => {
        try {
          const payload = JSON.parse(e.data);
          if (!payload.room_id || !payload.message_id || !payload.emoji || !payload.user) return;
          const existing = useChatStore
            .getState()
            .messages[payload.room_id]
            ?.find((message) => message.event_id === payload.message_id);
          const reactions = { ...(existing?.reactions ?? {}) };
          const users = reactions[payload.emoji] ?? [];
          if (payload.added) {
            reactions[payload.emoji] = users.includes(payload.user) ? users : [...users, payload.user];
          } else {
            const remaining = users.filter((user) => user !== payload.user);
            if (remaining.length > 0) {
              reactions[payload.emoji] = remaining;
            } else {
              delete reactions[payload.emoji];
            }
          }
          updateMessage(payload.room_id, payload.message_id, { reactions });
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

      es.addEventListener("read_receipt", (e) => {
        try {
          const payload = JSON.parse(e.data);
          if (!payload.room_id || !payload.message_id || !payload.reader_uri) return;
          const existing = useChatStore
            .getState()
            .messages[payload.room_id]
            ?.find((message) => message.event_id === payload.message_id);
          const readBy = existing?.read_by ?? [];
          if (!readBy.includes(payload.reader_uri)) {
            updateMessage(payload.room_id, payload.message_id, { read_by: [...readBy, payload.reader_uri] });
          }
        } catch { /* ignore */ }
      });

      es.addEventListener("user_created", () => {
        // Trigger admin panel refresh via custom event
        window.dispatchEvent(new CustomEvent("pale:admin-refresh"));
      });

      // Meeting events
      es.addEventListener("lobby_updated", (e) => {
        try {
          useMeetingStore.getState().setLobby(JSON.parse(e.data));
        } catch { /* ignore */ }
      });

      es.addEventListener("conference_participant_updated", (e) => {
        try {
          useMeetingStore.getState().setConference(JSON.parse(e.data));
        } catch { /* ignore */ }
      });

      es.addEventListener("hand_raised", (e) => {
        try {
          const data = JSON.parse(e.data);
          useMeetingStore.getState().setRaisedHands(data.hands ?? []);
        } catch { /* ignore */ }
      });

      es.addEventListener("poll_updated", (e) => {
        try {
          useMeetingStore.getState().upsertPoll(JSON.parse(e.data));
        } catch { /* ignore */ }
      });

      es.addEventListener("qa_updated", (e) => {
        try {
          useMeetingStore.getState().upsertQuestion(JSON.parse(e.data));
        } catch { /* ignore */ }
      });

      es.addEventListener("breakout_updated", (e) => {
        try {
          useMeetingStore.getState().upsertBreakout(JSON.parse(e.data));
        } catch { /* ignore */ }
      });

      es.addEventListener("live_caption", (e) => {
        try {
          const segment = JSON.parse(e.data);
          if (useMeetingStore.getState().captionsEnabled) {
            useMeetingStore.getState().addCaption(segment);
          }
        } catch { /* ignore */ }
      });

      es.onerror = () => {
        es.close();
        sourceRef.current = null;
        setOffline(true);
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
      for (const timeout of typingTimeouts.values()) {
        window.clearTimeout(timeout);
      }
      typingTimeouts.clear();
    };
  }, [
    baseUrl,
    token,
    setPresence,
    setBulkPresence,
    addMessage,
    updateMessage,
    removeMessage,
    upsertRoom,
    setTypingUsers,
    setOffline,
    addActivity,
  ]);

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
