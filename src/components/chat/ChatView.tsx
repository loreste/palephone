import { useState, useRef, useEffect, useCallback } from "react";
import { Send, Paperclip, MessageSquare, FileIcon, ImageIcon, Plus, X, Loader2, Phone, Users } from "lucide-react";
import { cn } from "@/lib/cn";
import { useChatStore, type ChatMessage, type RoomSummary } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { matrixSendMessage, matrixSetTyping, matrixCreateDm, paleServerGetMessages, makeCall as ipcMakeCall, paleServerApi } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { EncryptionBadge } from "@/components/encryption/EncryptionBadge";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

const QUICK_REACTIONS = ["\u{1F44D}", "\u{2764}\u{FE0F}", "\u{1F602}", "\u{1F44F}", "\u{1F914}"];

export function ChatView() {
  const authState = useMatrixStore((s) => s.authState);
  const { rooms, activeRoomId, setActiveRoomId, messages, typingByRoom } = useChatStore();
  const { baseUrl, token, connected } = useServerStore();
  const [serverRooms, setServerRooms] = useState<RoomSummary[]>([]);

  // Load server rooms
  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    import("@/lib/tauri").then(({ paleServerGetRooms }) => {
      paleServerGetRooms(baseUrl, token)
        .then((rooms) => {
          setServerRooms(
            rooms.map((r) => ({
              room_id: r.id,
              name: r.name,
              is_direct: r.is_direct,
              is_encrypted: false,
              last_message: null,
              last_message_sender: null,
              last_message_ts: null,
              unread_count: 0,
            }))
          );
        })
        .catch(() => {});
    });
  }, [connected, baseUrl, token]);

  // Merge Matrix rooms with server rooms
  const allRooms = [...rooms, ...serverRooms.filter((sr) => !rooms.some((r) => r.room_id === sr.room_id))];

  if (authState !== "logged_in" && !connected) {
    return <MatrixLoginView />;
  }

  const activeRoom = allRooms.find((r) => r.room_id === activeRoomId);
  const roomMessages = activeRoomId ? (messages[activeRoomId] ?? []) : [];
  const typingUsers = activeRoomId ? (typingByRoom[activeRoomId] ?? []) : [];

  return (
    <div className="flex flex-col h-full">
      {activeRoom ? (
        <ChatRoom
          room={activeRoom}
          messages={roomMessages}
          typingUsers={typingUsers}
          onBack={() => setActiveRoomId(null)}
        />
      ) : (
        <ConversationList
          rooms={allRooms}
          onSelect={(id) => setActiveRoomId(id)}
        />
      )}
    </div>
  );
}

function ConversationList({
  rooms,
  onSelect,
}: {
  rooms: RoomSummary[];
  onSelect: (id: string) => void;
}) {
  const [showNewChat, setShowNewChat] = useState(false);

  const { baseUrl, token, connected } = useServerStore();

  const handleNewDm = async (userId: string) => {
    try {
      const roomId = await matrixCreateDm(userId);
      setShowNewChat(false);
      onSelect(roomId);
    } catch (err) {
      toast({ type: "error", title: "Could not create chat", description: String(err) });
    }
  };

  const handleCreateRoom = async (name: string, members: string[]) => {
    if (!connected || !baseUrl || !token) {
      toast({ type: "error", title: "Not connected to server" });
      return;
    }
    try {
      const { paleServerCreateRoom } = await import("@/lib/tauri");
      const room = await paleServerCreateRoom(baseUrl, token, name, "", members);
      setShowNewChat(false);
      onSelect(room.id);
    } catch (err) {
      toast({ type: "error", title: "Could not create room", description: String(err) });
    }
  };

  return (
    <>
      <div className="px-4 pt-4 pb-2 flex items-center justify-between">
        <h1 className="text-lg font-semibold text-primary">Chat</h1>
        <button
          onClick={() => setShowNewChat(!showNewChat)}
          className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-elevated transition-colors"
          title="New conversation"
        >
          {showNewChat ? <X size={16} /> : <Plus size={16} />}
        </button>
      </div>

      {showNewChat && <NewChatInput onSubmit={handleNewDm} onCreateRoom={handleCreateRoom} />}

      <ActiveConferences />

      <div className="flex-1 overflow-y-auto px-2">
        {rooms.length === 0 && !showNewChat ? (
          <div className="flex flex-col items-center justify-center h-48 gap-2">
            <MessageSquare size={32} className="text-tertiary" />
            <p className="text-sm text-tertiary">No conversations yet</p>
            <button
              onClick={() => setShowNewChat(true)}
              className="text-xs text-accent hover:underline"
            >
              Start a new chat
            </button>
          </div>
        ) : (
          rooms.map((room) => (
            <button
              key={room.room_id}
              onClick={() => onSelect(room.room_id)}
              className={cn(
                "w-full flex items-center gap-3 px-3 py-3 rounded-lg",
                "hover:bg-elevated transition-colors text-left"
              )}
            >
              <div className="relative">
                <CallerAvatar name={room.name} size="sm" />
                {room.is_direct && <PresenceDot name={room.name} />}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1">
                  {room.is_encrypted && <EncryptionBadge level="encrypted" />}
                  <span className="text-sm font-medium text-primary truncate">{room.name}</span>
                </div>
                {room.last_message && (
                  <p className="text-xs text-tertiary truncate">{room.last_message}</p>
                )}
              </div>
              {room.unread_count > 0 && (
                <span className="shrink-0 w-5 h-5 rounded-full bg-accent text-white text-[10px] font-bold flex items-center justify-center">
                  {room.unread_count > 99 ? "99+" : room.unread_count}
                </span>
              )}
            </button>
          ))
        )}
      </div>
    </>
  );
}

function ChatRoom({
  room,
  messages,
  typingUsers,
  onBack,
}: {
  room: RoomSummary;
  messages: ChatMessage[];
  typingUsers: string[];
  onBack: () => void;
}) {
  const [input, setInput] = useState("");
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<number | null>(null);
  const typingSentRef = useRef(false);
  const { baseUrl, token, connected } = useServerStore();
  const addMessage = useChatStore((s) => s.addMessage);
  const isServerRoom = !room.room_id.startsWith("!");

  // Load server room messages on mount (stable deps only — no addMessage to avoid re-render loop)
  useEffect(() => {
    if (isServerRoom && connected && baseUrl && token) {
      import("@/lib/tauri").then(({ paleServerGetRoomMessages }) => {
        paleServerGetRoomMessages(baseUrl, token, room.room_id)
          .then((msgs) => {
            const add = useChatStore.getState().addMessage;
            for (const msg of msgs) {
              add({
                event_id: msg.id,
                room_id: room.room_id,
                sender: msg.sender_uri,
                sender_name: null,
                body: msg.body,
                msg_type: "text",
                timestamp: Math.floor(new Date(msg.created_at).getTime() / 1000),
                is_encrypted: false,
                is_own: false,
              });
            }
          })
          .catch(() => {});
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [room.room_id, isServerRoom, connected, baseUrl, token]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  // Load older messages when scrolling to top (avoid messages/addMessage in deps to prevent re-creation)
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

  const handleScroll = useCallback(async () => {
    const container = messagesContainerRef.current;
    if (!container || !connected || !baseUrl || !token || loadingHistory || !hasMore) return;
    if (container.scrollTop > 50) return;

    setLoadingHistory(true);
    const oldest = messagesRef.current[0];
    const before = oldest ? new Date(oldest.timestamp * 1000).toISOString() : undefined;

    try {
      const older = await paleServerGetMessages(baseUrl, token, {
        limit: 50,
        before,
        roomId: room.room_id,
      });
      if (older.length === 0) {
        setHasMore(false);
      } else {
        const add = useChatStore.getState().addMessage;
        for (const msg of older) {
          add({
            event_id: msg.id,
            room_id: room.room_id,
            sender: msg.from_uri,
            sender_name: null,
            body: msg.body,
            msg_type: "text",
            timestamp: Math.floor(new Date(msg.received_at).getTime() / 1000),
            is_encrypted: false,
            is_own: false,
          });
        }
      }
    } catch { /* ignore */ }
    setLoadingHistory(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected, baseUrl, token, loadingHistory, hasMore, room.room_id]);

  useEffect(() => {
    return () => {
      if (typingTimeoutRef.current) {
        window.clearTimeout(typingTimeoutRef.current);
      }
      if (typingSentRef.current) {
        matrixSetTyping(room.room_id, false).catch(() => {});
      }
    };
  }, [room.room_id]);

  const stopTyping = () => {
    if (!typingSentRef.current) return;
    typingSentRef.current = false;
    matrixSetTyping(room.room_id, false).catch(() => {});
  };

  const notifyTyping = (value: string) => {
    if (typingTimeoutRef.current) {
      window.clearTimeout(typingTimeoutRef.current);
    }

    if (value.trim() && !typingSentRef.current) {
      typingSentRef.current = true;
      matrixSetTyping(room.room_id, true).catch(() => {});
    }

    typingTimeoutRef.current = window.setTimeout(stopTyping, 2500);
  };

  const handleSend = async () => {
    if (!input.trim()) return;
    const body = input.trim();
    setInput("");
    stopTyping();
    try {
      if (isServerRoom && connected && baseUrl && token) {
        const { paleServerSendRoomMessage } = await import("@/lib/tauri");
        const msg = await paleServerSendRoomMessage(baseUrl, token, room.room_id, body);
        addMessage({
          event_id: msg.id,
          room_id: room.room_id,
          sender: msg.sender_uri,
          sender_name: null,
          body: msg.body,
          msg_type: "text",
          timestamp: Math.floor(new Date(msg.created_at).getTime() / 1000),
          is_encrypted: false,
          is_own: true,
        });
      } else {
        await matrixSendMessage(room.room_id, body);
      }
    } catch (err) {
      toast({ type: "error", title: "Send failed", description: String(err) });
    }
  };

  return (
    <>
      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-3 border-b border-border-subtle shrink-0">
        <button
          onClick={onBack}
          className="text-tertiary hover:text-primary text-sm"
        >
          &larr;
        </button>
        <div className="relative">
          <CallerAvatar name={room.name} size="sm" />
          {room.is_direct && <PresenceDot name={room.name} />}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1">
            {room.is_encrypted && <EncryptionBadge level="encrypted" />}
            <span className="text-sm font-semibold text-primary truncate">{room.name}</span>
          </div>
          <PresenceLabel name={room.name} isDirect={room.is_direct} isEncrypted={room.is_encrypted} />
        </div>
      </div>

      {/* Messages */}
      <div
        ref={messagesContainerRef}
        className="flex-1 overflow-y-auto px-4 py-3 space-y-2"
        onScroll={handleScroll}
      >
        {loadingHistory && (
          <div className="flex items-center justify-center py-2">
            <Loader2 size={16} className="animate-spin text-tertiary" />
            <span className="text-xs text-tertiary ml-2">Loading history...</span>
          </div>
        )}
        {messages.length === 0 && !loadingHistory && (
          <div className="flex items-center justify-center h-32">
            <p className="text-sm text-tertiary">No messages yet</p>
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.event_id} message={msg} />
        ))}
        <div ref={messagesEndRef} />
      </div>

      {typingUsers.length > 0 && <TypingIndicator users={typingUsers} />}

      {/* Compose bar */}
      <div className="flex items-center gap-2 px-3 py-2 border-t border-border-subtle shrink-0">
        <button
          className="p-2 text-tertiary hover:text-secondary rounded-md hover:bg-elevated"
          aria-label="Attach file"
        >
          <Paperclip size={18} />
        </button>
        <input
          type="text"
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
            notifyTyping(e.target.value);
          }}
          onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && handleSend()}
          placeholder="Type a message..."
          className={cn(
            "flex-1 bg-surface border border-border-subtle rounded-lg",
            "px-3 py-2 text-sm text-primary",
            "placeholder:text-tertiary",
            "focus:outline-none focus:border-border-focus"
          )}
        />
        <button
          onClick={handleSend}
          disabled={!input.trim()}
          className={cn(
            "p-2 rounded-lg transition-colors",
            input.trim()
              ? "bg-accent text-white hover:bg-accent-hover"
              : "text-tertiary cursor-not-allowed"
          )}
          aria-label="Send"
        >
          <Send size={18} />
        </button>
      </div>
    </>
  );
}

interface ConferenceSummary {
  id: string;
  title: string;
  mode: "audio" | "video" | "webinar";
  active: boolean;
  participants: Array<{ user_id: string; sip_uri: string; role: string; joined_at: string }>;
  created_at: string;
}

function ActiveConferences() {
  const { baseUrl, token, connected } = useServerStore();
  const [conferences, setConferences] = useState<ConferenceSummary[]>([]);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    paleServerApi<ConferenceSummary[]>(baseUrl, token, "/v1/conferences")
      .then((all) => setConferences(all.filter((c) => c.active)))
      .catch(() => {});
  }, [connected, baseUrl, token]);

  if (!connected || conferences.length === 0) return null;

  return (
    <div className="px-3 pb-2">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1.5 text-xs font-semibold text-tertiary hover:text-secondary w-full py-1"
      >
        <Users size={13} />
        <span>Active Conferences ({conferences.length})</span>
      </button>
      {expanded && (
        <div className="space-y-1 mt-1">
          {conferences.map((conf) => (
            <div
              key={conf.id}
              className={cn(
                "flex items-center justify-between px-3 py-2 rounded-lg",
                "bg-surface border border-border-subtle"
              )}
            >
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium text-primary truncate">{conf.title}</p>
                <p className="text-[10px] text-tertiary">
                  {conf.mode} &middot; {conf.participants.length} participant{conf.participants.length !== 1 ? "s" : ""}
                </p>
              </div>
              <button
                onClick={() => {
                  const uri = `sip:conf-${conf.id}@pale.local`;
                  toast({ type: "info", title: `Joining ${conf.title}...` });
                  ipcMakeCall(uri).catch(() =>
                    toast({ type: "error", title: "Failed to join conference" })
                  );
                }}
                className={cn(
                  "shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium",
                  "bg-success/10 text-success hover:bg-success/20 transition-colors"
                )}
              >
                <Phone size={12} />
                Join
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function NewChatInput({
  onSubmit,
  onCreateRoom,
}: {
  onSubmit: (userId: string) => void;
  onCreateRoom: (name: string, members: string[]) => void;
}) {
  const [mode, setMode] = useState<"dm" | "room">("dm");
  const [userId, setUserId] = useState("");
  const [roomName, setRoomName] = useState("");
  const [roomMembers, setRoomMembers] = useState("");
  const [serverUsers, setServerUsers] = useState<{ id: string; display_name: string; sip_uri: string }[]>([]);
  const [filteredUsers, setFilteredUsers] = useState<typeof serverUsers>([]);
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);

  useEffect(() => {
    if (baseUrl && token) {
      paleServerApi<typeof serverUsers>(baseUrl, token, "/v1/users")
        .then(setServerUsers)
        .catch(() => {});
    }
  }, [baseUrl, token]);

  useEffect(() => {
    if (!userId.trim()) { setFilteredUsers([]); return; }
    const q = userId.toLowerCase();
    setFilteredUsers(serverUsers.filter((u) =>
      u.display_name.toLowerCase().includes(q) || u.sip_uri.toLowerCase().includes(q)
    ));
  }, [userId, serverUsers]);

  return (
    <div className="px-4 pb-3 space-y-2">
      <div className="flex gap-1">
        <button
          onClick={() => setMode("dm")}
          className={cn(
            "px-2 py-1 text-xs rounded-md",
            mode === "dm" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary"
          )}
        >
          Direct Message
        </button>
        <button
          onClick={() => setMode("room")}
          className={cn(
            "px-2 py-1 text-xs rounded-md",
            mode === "room" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary"
          )}
        >
          Group Room
        </button>
      </div>

      {mode === "dm" ? (
        <>
          <div className="relative">
            <div className="flex gap-2">
              <input
                type="text"
                value={userId}
                onChange={(e) => setUserId(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && userId.trim()) onSubmit(userId.trim());
                }}
                placeholder="Search by name or SIP URI..."
                className={cn(
                  "flex-1 bg-surface border border-border-subtle rounded-lg",
                  "px-3 py-2 text-sm text-primary",
                  "placeholder:text-tertiary",
                  "focus:outline-none focus:border-border-focus"
                )}
                autoFocus
              />
              <button
                onClick={() => userId.trim() && onSubmit(userId.trim())}
                disabled={!userId.trim()}
                className={cn(
                  "px-3 py-2 rounded-lg text-sm font-medium transition-colors",
                  userId.trim()
                    ? "bg-accent text-white hover:bg-accent-hover"
                    : "bg-elevated text-tertiary cursor-not-allowed"
                )}
              >
                Start
              </button>
            </div>
            {filteredUsers.length > 0 && (
              <div className="absolute z-10 left-0 right-12 mt-1 bg-surface border border-border-subtle rounded-lg shadow-lg max-h-40 overflow-y-auto">
                {filteredUsers.map((u) => (
                  <button
                    key={u.id}
                    onClick={() => { setUserId(u.sip_uri); onSubmit(u.sip_uri); }}
                    className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-elevated transition-colors"
                  >
                    <div className="w-7 h-7 rounded-full bg-accent-muted text-accent flex items-center justify-center text-xs font-bold">
                      {u.display_name.charAt(0)}
                    </div>
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-primary truncate">{u.display_name}</p>
                      <p className="text-[10px] text-tertiary truncate">{u.sip_uri}</p>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
          <p className="text-[10px] text-tertiary">Search for a user to start a conversation</p>
        </>
      ) : (
        <>
          <input
            type="text"
            value={roomName}
            onChange={(e) => setRoomName(e.target.value)}
            placeholder="Room name"
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-lg",
              "px-3 py-2 text-sm text-primary placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus"
            )}
            autoFocus
          />
          <input
            type="text"
            value={roomMembers}
            onChange={(e) => setRoomMembers(e.target.value)}
            placeholder="Members (comma-separated SIP URIs)"
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-lg",
              "px-3 py-2 text-sm text-primary placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus"
            )}
          />
          <button
            onClick={() => {
              if (roomName.trim()) {
                const members = roomMembers.split(",").map((m) => m.trim()).filter(Boolean);
                onCreateRoom(roomName.trim(), members);
              }
            }}
            disabled={!roomName.trim()}
            className={cn(
              "w-full px-3 py-2 rounded-lg text-sm font-medium transition-colors",
              roomName.trim()
                ? "bg-accent text-white hover:bg-accent-hover"
                : "bg-elevated text-tertiary cursor-not-allowed"
            )}
          >
            Create Room
          </button>
          <p className="text-[10px] text-tertiary">Create a group chat on the Pale server</p>
        </>
      )}
    </div>
  );
}

function TypingIndicator({ users }: { users: string[] }) {
  const names = users.map((id) => id.split(":")[0]?.replace("@", "") || id);
  const label =
    names.length === 1
      ? `${names[0]} is typing`
      : names.length === 2
        ? `${names[0]} and ${names[1]} are typing`
        : `${names[0]} and ${names.length - 1} others are typing`;

  return (
    <div className="px-4 pb-1 text-[11px] text-tertiary">
      {label}
      <span className="inline-block w-5 text-left">...</span>
    </div>
  );
}

function msgTypeLabel(mt: ChatMessage["msg_type"]): string {
  if (typeof mt === "string") return mt;
  if ("image" in mt) return "image";
  if ("file" in mt) return "file";
  if ("audio" in mt) return "audio";
  if ("video" in mt) return "video";
  return "text";
}

function MessageBubble({ message }: { message: ChatMessage }) {
  const time = new Date(message.timestamp * 1000).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
  const kind = msgTypeLabel(message.msg_type);
  const { baseUrl, token, connected } = useServerStore();

  const handleDelete = async () => {
    if (!connected || !baseUrl || !token) return;
    const { paleServerDeleteMessage } = await import("@/lib/tauri");
    paleServerDeleteMessage(baseUrl, token, message.event_id).catch(() => {});
  };

  return (
    <div
      className={cn(
        "flex group/msg",
        message.is_own ? "justify-end" : "justify-start"
      )}
    >
      {/* Message actions — visible on hover */}
      {message.is_own && connected && (
        <div className="flex items-center gap-0.5 mr-1 opacity-0 group-hover/msg:opacity-100 transition-opacity">
          <button
            onClick={handleDelete}
            className="p-0.5 rounded text-tertiary hover:text-destructive"
            title="Delete message"
          >
            <X size={12} />
          </button>
        </div>
      )}
      {!message.is_own && connected && (
        <div className="flex items-center gap-0.5 order-last ml-1 opacity-0 group-hover/msg:opacity-100 transition-opacity">
          {QUICK_REACTIONS.map((emoji) => (
            <button
              key={emoji}
              onClick={async () => {
                if (!baseUrl || !token) return;
                await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/messages/${message.event_id}/react`, {
                  method: "POST",
                  headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
                  body: JSON.stringify({ emoji }),
                });
              }}
              className="p-0.5 rounded hover:bg-elevated text-xs"
              title={emoji}
            >
              {emoji}
            </button>
          ))}
        </div>
      )}
      <div
        className={cn(
          "max-w-[80%] rounded-2xl px-3 py-2",
          message.is_own
            ? "bg-accent text-white rounded-br-md"
            : "bg-surface border border-border-subtle text-primary rounded-bl-md"
        )}
      >
        {!message.is_own && (
          <p className="text-[10px] font-semibold text-accent mb-0.5">
            {message.sender_name ?? message.sender.split(":")[0]?.replace("@", "")}
          </p>
        )}
        {kind === "image" && typeof message.msg_type === "object" && "image" in message.msg_type && (message.msg_type as { image: { url: string } }).image.url ? (
          <img
            src={(message.msg_type as { image: { url: string } }).image.url}
            alt="Shared image"
            className="rounded-lg max-w-full max-h-[300px] object-contain mb-1 cursor-pointer"
            onClick={() => window.open((message.msg_type as { image: { url: string } }).image.url, "_blank")}
          />
        ) : kind === "image" ? (
          <div className="flex items-center gap-1 mb-1">
            <ImageIcon size={14} className="opacity-60" />
            <span className="text-xs opacity-60">Image</span>
          </div>
        ) : null}
        {kind === "file" && typeof message.msg_type === "object" && "file" in message.msg_type && (
          <div className="flex items-center gap-1 mb-1 px-2 py-1.5 bg-black/5 rounded-lg">
            <FileIcon size={14} className="opacity-60" />
            <span className="text-xs opacity-60 flex-1 truncate">{message.msg_type.file.filename}</span>
            {message.msg_type.file.url && (
              <a
                href={message.msg_type.file.url}
                target="_blank"
                rel="noopener noreferrer"
                className="text-[10px] text-accent hover:underline shrink-0"
              >
                Download
              </a>
            )}
          </div>
        )}
        {kind === "audio" && typeof message.msg_type === "object" && "audio" in message.msg_type && message.msg_type.audio.url && (
          <audio controls className="max-w-full mb-1" src={message.msg_type.audio.url} />
        )}
        {kind === "video" && typeof message.msg_type === "object" && "video" in message.msg_type && message.msg_type.video.url && (
          <video controls className="rounded-lg max-w-full max-h-[300px] mb-1" src={message.msg_type.video.url} />
        )}
        <p className="text-sm whitespace-pre-wrap break-words">{message.body}</p>
        <p
          className={cn(
            "text-[9px] mt-1",
            message.is_own ? "text-white/60" : "text-tertiary"
          )}
        >
          {time}
        </p>
      </div>
    </div>
  );
}

const presenceColors: Record<PresenceStatus, string> = {
  online: "bg-green-500",
  busy: "bg-red-500",
  on_call: "bg-red-500",
  away: "bg-yellow-500",
  dnd: "bg-red-600",
  offline: "bg-gray-400",
};

function PresenceDot({ name }: { name: string }) {
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const match = Object.values(presenceMap).find(
    (p) => p.sip_uri.includes(name.toLowerCase()) || name.toLowerCase().includes(p.sip_uri.split("@")[0]?.replace("sip:", "") ?? "")
  );
  if (!match) return null;

  return (
    <span
      className={cn(
        "absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-surface",
        presenceColors[match.status]
      )}
      title={match.status}
    />
  );
}

function PresenceLabel({ name, isDirect, isEncrypted }: { name: string; isDirect: boolean; isEncrypted: boolean }) {
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const match = isDirect
    ? Object.values(presenceMap).find(
        (p) => p.sip_uri.includes(name.toLowerCase()) || name.toLowerCase().includes(p.sip_uri.split("@")[0]?.replace("sip:", "") ?? "")
      )
    : undefined;

  if (match) {
    const label = match.note ?? match.status.charAt(0).toUpperCase() + match.status.slice(1);
    return <p className="text-[10px] text-tertiary">{label}</p>;
  }
  return (
    <p className="text-[10px] text-tertiary">
      {isEncrypted ? "End-to-end encrypted" : "Not encrypted"}
    </p>
  );
}
