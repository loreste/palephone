import { useState, useRef, useEffect, useCallback } from "react";
import { Send, Paperclip, MessageSquare, FileIcon, ImageIcon, Plus, X, Loader2 } from "lucide-react";
import { cn } from "@/lib/cn";
import { useChatStore, type ChatMessage, type RoomSummary } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { matrixSendMessage, matrixSetTyping, matrixCreateDm, paleServerGetMessages } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { EncryptionBadge } from "@/components/encryption/EncryptionBadge";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

export function ChatView() {
  const authState = useMatrixStore((s) => s.authState);
  const { rooms, activeRoomId, setActiveRoomId, messages, typingByRoom } = useChatStore();

  if (authState !== "logged_in") {
    return <MatrixLoginView />;
  }

  const activeRoom = rooms.find((r) => r.room_id === activeRoomId);
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
          rooms={rooms}
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

  const handleNewDm = async (userId: string) => {
    try {
      const roomId = await matrixCreateDm(userId);
      setShowNewChat(false);
      onSelect(roomId);
    } catch (err) {
      toast({ type: "error", title: "Could not create chat", description: String(err) });
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

      {showNewChat && <NewChatInput onSubmit={handleNewDm} />}

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

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  // Load older messages when scrolling to top
  const handleScroll = useCallback(async () => {
    const container = messagesContainerRef.current;
    if (!container || !connected || !baseUrl || !token || loadingHistory || !hasMore) return;
    if (container.scrollTop > 50) return; // Only trigger near top

    setLoadingHistory(true);
    const oldest = messages[0];
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
        for (const msg of older) {
          addMessage({
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
  }, [connected, baseUrl, token, loadingHistory, hasMore, messages, room.room_id, addMessage]);

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
      await matrixSendMessage(room.room_id, body);
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

function NewChatInput({ onSubmit }: { onSubmit: (userId: string) => void }) {
  const [userId, setUserId] = useState("");

  return (
    <div className="px-4 pb-3">
      <div className="flex gap-2">
        <input
          type="text"
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && userId.trim()) onSubmit(userId.trim());
          }}
          placeholder="@user:homeserver.com"
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
      <p className="text-[10px] text-tertiary mt-1">Enter a Matrix user ID to start a conversation</p>
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

  return (
    <div
      className={cn(
        "flex",
        message.is_own ? "justify-end" : "justify-start"
      )}
    >
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
        {kind === "image" && (
          <div className="flex items-center gap-1 mb-1">
            <ImageIcon size={14} className="opacity-60" />
            <span className="text-xs opacity-60">Image</span>
          </div>
        )}
        {kind === "file" && typeof message.msg_type === "object" && "file" in message.msg_type && (
          <div className="flex items-center gap-1 mb-1">
            <FileIcon size={14} className="opacity-60" />
            <span className="text-xs opacity-60">{message.msg_type.file.filename}</span>
          </div>
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
