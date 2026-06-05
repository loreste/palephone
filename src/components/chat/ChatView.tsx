import { useState, useRef, useEffect } from "react";
import { Send, Paperclip, MessageSquare } from "lucide-react";
import { cn } from "@/lib/cn";
import { useChatStore, type ChatMessage, type RoomSummary } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { matrixSendMessage } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { EncryptionBadge } from "@/components/encryption/EncryptionBadge";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

export function ChatView() {
  const authState = useMatrixStore((s) => s.authState);
  const { rooms, activeRoomId, setActiveRoomId, messages } = useChatStore();

  if (authState !== "logged_in") {
    return <MatrixLoginView />;
  }

  const activeRoom = rooms.find((r) => r.room_id === activeRoomId);
  const roomMessages = activeRoomId ? (messages[activeRoomId] ?? []) : [];

  return (
    <div className="flex flex-col h-full">
      {activeRoom ? (
        <ChatRoom
          room={activeRoom}
          messages={roomMessages}
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
  return (
    <>
      <div className="px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold text-primary">Chat</h1>
      </div>

      <div className="flex-1 overflow-y-auto px-2">
        {rooms.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 gap-2">
            <MessageSquare size={32} className="text-tertiary" />
            <p className="text-sm text-tertiary">No conversations yet</p>
            <p className="text-xs text-tertiary">Start a new chat to begin</p>
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
              <CallerAvatar name={room.name} size="sm" />
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
  onBack,
}: {
  room: RoomSummary;
  messages: ChatMessage[];
  onBack: () => void;
}) {
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  const handleSend = async () => {
    if (!input.trim()) return;
    const body = input.trim();
    setInput("");
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
        <CallerAvatar name={room.name} size="sm" />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1">
            {room.is_encrypted && <EncryptionBadge level="encrypted" />}
            <span className="text-sm font-semibold text-primary truncate">{room.name}</span>
          </div>
          <p className="text-[10px] text-tertiary">
            {room.is_encrypted ? "End-to-end encrypted" : "Not encrypted"}
          </p>
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-2">
        {messages.length === 0 && (
          <div className="flex items-center justify-center h-32">
            <p className="text-sm text-tertiary">No messages yet</p>
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.event_id} message={msg} />
        ))}
        <div ref={messagesEndRef} />
      </div>

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
          onChange={(e) => setInput(e.target.value)}
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

function MessageBubble({ message }: { message: ChatMessage }) {
  const time = new Date(message.timestamp * 1000).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });

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
