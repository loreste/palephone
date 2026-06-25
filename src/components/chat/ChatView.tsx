import { useState, useRef, useEffect, useCallback } from "react";
import { Send, Paperclip, MessageSquare, FileIcon, ImageIcon, Plus, X, Loader2, Phone, Users, Reply, Pencil, Pin, Forward, Check, CheckCheck } from "lucide-react";
import { cn } from "@/lib/cn";
import { useChatStore, type ChatMessage, type RoomSummary } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import { matrixSendMessage, matrixSetTyping, matrixCreateDm, paleServerGetMessages, makeCall as ipcMakeCall, paleServerApi, paleServerPinMessage, paleServerMarkRead, paleServerGetUsers, paleServerSetTyping, paleServerUploadFile, paleServerGetRooms, paleServerCreateRoom, paleServerCreateDirectRoom, type ServerRoom, type ServerUser } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { EncryptionBadge } from "@/components/encryption/EncryptionBadge";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

const QUICK_REACTIONS = ["\u{1F44D}", "\u{2764}\u{FE0F}", "\u{1F602}", "\u{1F44F}", "\u{1F914}"];

const EMOJI_CATEGORIES: { label: string; emojis: string[] }[] = [
  { label: "Smileys", emojis: ["\u{1F600}", "\u{1F603}", "\u{1F604}", "\u{1F601}", "\u{1F606}", "\u{1F605}", "\u{1F602}", "\u{1F923}", "\u{1F60A}", "\u{1F607}", "\u{1F970}", "\u{1F60D}", "\u{1F618}", "\u{1F617}", "\u{1F914}", "\u{1F928}"] },
  { label: "Gestures", emojis: ["\u{1F44D}", "\u{1F44E}", "\u{1F44F}", "\u{1F64C}", "\u{1F4AA}", "\u{270C}\u{FE0F}", "\u{1F91E}", "\u{1F44B}", "\u{1F64F}", "\u{1F91D}"] },
  { label: "Objects", emojis: ["\u{2764}\u{FE0F}", "\u{1F525}", "\u{2B50}", "\u{1F389}", "\u{1F388}", "\u{1F381}", "\u{1F4A1}", "\u{1F4AC}", "\u{1F514}", "\u{1F3B5}", "\u{1F680}", "\u{2705}", "\u{274C}"] },
];

function directRoomName(room: ServerRoom, currentSipUri?: string | null): string {
  if (!room.is_direct || !currentSipUri) return room.name;
  const other = room.members.find((member) => member.user_sip_uri !== currentSipUri);
  return other?.user_sip_uri.replace(/^sip:/, "") ?? room.name;
}

function serverRoomToSummary(room: ServerRoom, currentSipUri?: string | null, nameOverride?: string): RoomSummary {
  return {
    room_id: room.id,
    name: nameOverride ?? directRoomName(room, currentSipUri),
    is_direct: room.is_direct,
    is_encrypted: false,
    last_message: null,
    last_message_sender: null,
    last_message_ts: null,
    unread_count: 0,
  };
}

function sanitizeHtml(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, "")
    .replace(/on\w+="[^"]*"/gi, "")
    .replace(/on\w+='[^']*'/gi, "");
}

function renderMarkdown(text: string): string {
  let html = text
    // Escape HTML entities first
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  // Code blocks (triple backtick)
  html = html.replace(/```([\s\S]*?)```/g, '<pre class="bg-black/10 rounded p-2 my-1 text-xs overflow-x-auto"><code>$1</code></pre>');

  // Inline code
  html = html.replace(/`([^`]+)`/g, '<code class="bg-black/10 rounded px-1 py-0.5 text-xs">$1</code>');

  // Bold
  html = html.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");

  // Italic
  html = html.replace(/\*(.+?)\*/g, "<em>$1</em>");

  // Links [text](url)
  html = html.replace(
    /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer" class="underline">$1</a>'
  );

  // Auto-link bare URLs
  html = html.replace(
    /(?<![href="])(https?:\/\/[^\s<]+)/g,
    '<a href="$1" target="_blank" rel="noopener noreferrer" class="underline">$1</a>'
  );

  // @Mentions
  html = html.replace(
    /@(\w[\w\s]{0,30}?\w)(?=\s|$|[.,!?;:])/g,
    '<span class="text-accent font-semibold">@$1</span>'
  );

  // Newlines to <br>
  html = html.replace(/\n/g, "<br>");

  return sanitizeHtml(html);
}

export function ChatView() {
  const authState = useMatrixStore((s) => s.authState);
  const { rooms, activeRoomId, setActiveRoomId, setRooms, messages, typingByRoom } = useChatStore();
  const { baseUrl, token, connected } = useServerStore();
  const currentSipUri = useAccountStore((s) => s.account?.sipUri);

  // Load server rooms
  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    paleServerGetRooms(baseUrl, token)
      .then((serverRooms) => setRooms(serverRooms.map((room) => serverRoomToSummary(room, currentSipUri))))
      .catch(() => {});
  }, [connected, baseUrl, token, currentSipUri, setRooms]);

  if (authState !== "logged_in" && !connected) {
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

  const { baseUrl, token, connected } = useServerStore();
  const upsertRoom = useChatStore((s) => s.upsertRoom);

  const handleNewDm = async (user: { display_name: string; sip_uri: string; matrix_user_id?: string | null }) => {
    try {
      if (connected && baseUrl && token) {
        const room = await paleServerCreateDirectRoom(baseUrl, token, user);
        upsertRoom(serverRoomToSummary(room, undefined, user.display_name));
        setShowNewChat(false);
        onSelect(room.id);
        return;
      }

      const roomId = await matrixCreateDm(user.matrix_user_id ?? user.sip_uri);
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
      const room = await paleServerCreateRoom(baseUrl, token, name, "", members);
      upsertRoom(serverRoomToSummary(room));
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
  const [replyingTo, setReplyingTo] = useState<ChatMessage | null>(null);
  const [editingMessage, setEditingMessage] = useState<ChatMessage | null>(null);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [mentionUsers, setMentionUsers] = useState<ServerUser[]>([]);
  const [allUsers, setAllUsers] = useState<ServerUser[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<number | null>(null);
  const typingSentRef = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const { baseUrl, token, connected } = useServerStore();
  const addMessage = useChatStore((s) => s.addMessage);
  const updateMessage = useChatStore((s) => s.updateMessage);
  const isServerRoom = !room.room_id.startsWith("!");

  // Load server users for mentions
  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    paleServerGetUsers(baseUrl, token).then(setAllUsers).catch(() => {});
  }, [connected, baseUrl, token]);

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
  }, [room.room_id, isServerRoom, connected, baseUrl, token]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  // Send read receipt for the latest message when room opens
  useEffect(() => {
    if (!connected || !baseUrl || !token || messages.length === 0) return;
    const lastMsg = messages[messages.length - 1];
    if (lastMsg && !lastMsg.is_own) {
      paleServerMarkRead(baseUrl, token, lastMsg.event_id).catch(() => {});
    }
  }, [connected, baseUrl, token, messages.length]); // eslint-disable-line react-hooks/exhaustive-deps

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
  }, [connected, baseUrl, token, loadingHistory, hasMore, room.room_id]);

  useEffect(() => {
    return () => {
      if (typingTimeoutRef.current) {
        window.clearTimeout(typingTimeoutRef.current);
      }
      if (typingSentRef.current) {
        if (isServerRoom && connected && baseUrl && token) {
          paleServerSetTyping(baseUrl, token, room.room_id, false).catch(() => {});
        } else {
          matrixSetTyping(room.room_id, false).catch(() => {});
        }
      }
    };
  }, [room.room_id, isServerRoom, connected, baseUrl, token]);

  const sendTypingState = (typing: boolean) => {
    if (isServerRoom && connected && baseUrl && token) {
      paleServerSetTyping(baseUrl, token, room.room_id, typing).catch(() => {});
    } else {
      matrixSetTyping(room.room_id, typing).catch(() => {});
    }
  };

  const stopTyping = () => {
    if (!typingSentRef.current) return;
    typingSentRef.current = false;
    sendTypingState(false);
  };

  const notifyTyping = (value: string) => {
    if (typingTimeoutRef.current) {
      window.clearTimeout(typingTimeoutRef.current);
    }

    if (value.trim() && !typingSentRef.current) {
      typingSentRef.current = true;
      sendTypingState(true);
    }

    typingTimeoutRef.current = window.setTimeout(stopTyping, 2500);
  };

  // Handle @ mention detection
  const handleInputChange = (value: string) => {
    setInput(value);
    notifyTyping(value);

    // Detect @ mention
    const cursorPos = inputRef.current?.selectionStart ?? value.length;
    const textBeforeCursor = value.slice(0, cursorPos);
    const atMatch = textBeforeCursor.match(/@(\w*)$/);
    if (atMatch) {
      const query = atMatch[1].toLowerCase();
      setMentionQuery(query);
      setMentionUsers(
        allUsers.filter((u) =>
          u.display_name.toLowerCase().includes(query) ||
          u.sip_uri.toLowerCase().includes(query)
        ).slice(0, 5)
      );
    } else {
      setMentionQuery(null);
      setMentionUsers([]);
    }
  };

  const insertMention = (user: ServerUser) => {
    const cursorPos = inputRef.current?.selectionStart ?? input.length;
    const textBeforeCursor = input.slice(0, cursorPos);
    const atIdx = textBeforeCursor.lastIndexOf("@");
    const newInput = input.slice(0, atIdx) + `@${user.display_name} ` + input.slice(cursorPos);
    setInput(newInput);
    setMentionQuery(null);
    setMentionUsers([]);
    inputRef.current?.focus();
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (!connected || !baseUrl || !token) return;
    const files = Array.from(e.dataTransfer.files);
    for (const file of files) {
      try {
        const uploaded = await paleServerUploadFile(baseUrl, token, file);
        const fileUrl = `${baseUrl.replace(/\/+$/, "")}/v1/files/${uploaded.id}`;
        const body = `[File: ${uploaded.filename}](${fileUrl})`;
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
      } catch (err) {
        toast({ type: "error", title: "Upload failed", description: String(err) });
      }
    }
  };

  const handleSend = async () => {
    if (!input.trim()) return;
    const body = input.trim();
    setInput("");
    stopTyping();

    // Handle edit mode
    if (editingMessage) {
      const msgId = editingMessage.event_id;
      setEditingMessage(null);
      try {
        if (connected && baseUrl && token) {
          const { paleServerEditMessage } = await import("@/lib/tauri");
          await paleServerEditMessage(baseUrl, token, msgId, body);
          updateMessage(room.room_id, msgId, { body, edited_at: Math.floor(Date.now() / 1000) });
        }
      } catch (err) {
        toast({ type: "error", title: "Edit failed", description: String(err) });
      }
      return;
    }

    // Handle reply or normal send
    try {
      if (isServerRoom && connected && baseUrl && token) {
        const { paleServerSendRoomMessage } = await import("@/lib/tauri");
        const msg = await paleServerSendRoomMessage(baseUrl, token, room.room_id, body, replyingTo?.event_id);
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
          reply_to: replyingTo?.event_id,
          reply_preview: replyingTo ? { sender: replyingTo.sender_name ?? replyingTo.sender, body: replyingTo.body } : undefined,
        });
      } else {
        await matrixSendMessage(room.room_id, body);
      }
    } catch (err) {
      toast({ type: "error", title: "Send failed", description: String(err) });
    }
    setReplyingTo(null);
  };

  const handleReply = (msg: ChatMessage) => {
    setReplyingTo(msg);
    setEditingMessage(null);
    inputRef.current?.focus();
  };

  const handleEdit = (msg: ChatMessage) => {
    setEditingMessage(msg);
    setReplyingTo(null);
    setInput(msg.body);
    inputRef.current?.focus();
  };

  const handleForward = async (msg: ChatMessage) => {
    const target = window.prompt("Enter room ID or user SIP URI to forward to:");
    if (!target || !connected || !baseUrl || !token) return;
    const senderLabel = msg.sender_name ?? msg.sender;
    const body = `Forwarded from ${senderLabel}:\n${msg.body}`;
    try {
      const { paleServerSendRoomMessage } = await import("@/lib/tauri");
      await paleServerSendRoomMessage(baseUrl, token, target, body);
      toast({ type: "success", title: "Message forwarded" });
    } catch (err) {
      toast({ type: "error", title: "Forward failed", description: String(err) });
    }
  };

  const handlePin = async (msg: ChatMessage) => {
    if (!connected || !baseUrl || !token) return;
    const newPinned = !msg.pinned;
    try {
      await paleServerPinMessage(baseUrl, token, msg.event_id, newPinned);
      updateMessage(room.room_id, msg.event_id, { pinned: newPinned });
    } catch (err) {
      toast({ type: "error", title: "Pin failed", description: String(err) });
    }
  };

  return (
    <div
      className="flex flex-col h-full relative"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {isDragging && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-accent/10 border-2 border-dashed border-accent rounded-lg pointer-events-none">
          <p className="text-accent font-semibold text-sm">Drop files to upload</p>
        </div>
      )}
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
          <MessageBubble
            key={msg.event_id}
            message={msg}
            onReply={handleReply}
            onEdit={handleEdit}
            onForward={handleForward}
            onPin={handlePin}
          />
        ))}
        <div ref={messagesEndRef} />
      </div>

      {typingUsers.length > 0 && <TypingIndicator users={typingUsers} />}

      {/* Reply/Edit preview bar */}
      {replyingTo && (
        <div className="flex items-center gap-2 px-3 py-1.5 border-t border-border-subtle bg-elevated text-xs">
          <Reply size={12} className="text-accent shrink-0" />
          <div className="flex-1 min-w-0">
            <span className="font-semibold text-accent">{replyingTo.sender_name ?? replyingTo.sender}</span>
            <span className="text-tertiary ml-1 truncate">{replyingTo.body.slice(0, 60)}</span>
          </div>
          <button onClick={() => setReplyingTo(null)} className="p-0.5 text-tertiary hover:text-primary">
            <X size={12} />
          </button>
        </div>
      )}
      {editingMessage && (
        <div className="flex items-center gap-2 px-3 py-1.5 border-t border-border-subtle bg-elevated text-xs">
          <Pencil size={12} className="text-warning shrink-0" />
          <span className="text-warning font-semibold">Editing message</span>
          <div className="flex-1" />
          <button onClick={() => { setEditingMessage(null); setInput(""); }} className="p-0.5 text-tertiary hover:text-primary">
            <X size={12} />
          </button>
        </div>
      )}

      {/* Mention dropdown */}
      {mentionQuery !== null && mentionUsers.length > 0 && (
        <div className="px-3 pb-1">
          <div className="bg-surface border border-border-subtle rounded-lg shadow-lg max-h-32 overflow-y-auto">
            {mentionUsers.map((u) => (
              <button
                key={u.id}
                onClick={() => insertMention(u)}
                className="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-elevated transition-colors text-sm"
              >
                <span className="w-5 h-5 rounded-full bg-accent-muted text-accent flex items-center justify-center text-[10px] font-bold">
                  {u.display_name.charAt(0)}
                </span>
                <span className="text-primary">{u.display_name}</span>
                <span className="text-tertiary text-xs ml-auto">{u.sip_uri}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Compose bar */}
      <div className="flex items-center gap-2 px-3 py-2 border-t border-border-subtle shrink-0">
        <button
          className="p-2 text-tertiary hover:text-secondary rounded-md hover:bg-elevated"
          aria-label="Attach file"
        >
          <Paperclip size={18} />
        </button>
        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => handleInputChange(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && handleSend()}
          placeholder={editingMessage ? "Edit your message..." : "Type a message..."}
          className={cn(
            "flex-1 bg-surface border border-border-subtle rounded-lg",
            "px-3 py-2 text-sm text-primary",
            "placeholder:text-tertiary",
            "focus:outline-none focus:border-border-focus",
            editingMessage && "border-warning/50"
          )}
        />
        <button
          onClick={handleSend}
          disabled={!input.trim()}
          className={cn(
            "p-2 rounded-lg transition-colors",
            input.trim()
              ? editingMessage
                ? "bg-warning text-white hover:bg-warning/80"
                : "bg-accent text-white hover:bg-accent-hover"
              : "text-tertiary cursor-not-allowed"
          )}
          aria-label={editingMessage ? "Save edit" : "Send"}
        >
          {editingMessage ? <Check size={18} /> : <Send size={18} />}
        </button>
      </div>
    </div>
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
  onSubmit: (user: { display_name: string; sip_uri: string; matrix_user_id?: string | null }) => void;
  onCreateRoom: (name: string, members: string[]) => void;
}) {
  const [mode, setMode] = useState<"dm" | "room">("dm");
  const [userId, setUserId] = useState("");
  const [roomName, setRoomName] = useState("");
  const [roomMembers, setRoomMembers] = useState("");
  const [serverUsers, setServerUsers] = useState<ServerUser[]>([]);
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
                  if (e.key === "Enter" && userId.trim()) {
                    onSubmit({ display_name: userId.trim(), sip_uri: userId.trim() });
                  }
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
                onClick={() =>
                  userId.trim() && onSubmit({ display_name: userId.trim(), sip_uri: userId.trim() })
                }
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
                    onClick={() => { setUserId(u.sip_uri); onSubmit(u); }}
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

function EmojiPickerButton({ onSelect }: { onSelect: (emoji: string) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="p-0.5 rounded hover:bg-elevated text-xs text-tertiary hover:text-accent"
        title="More reactions"
      >
        <Plus size={12} />
      </button>
      {open && (
        <div className="absolute bottom-6 right-0 z-50 bg-surface border border-border-subtle rounded-lg shadow-lg p-2 w-56">
          {EMOJI_CATEGORIES.map((cat) => (
            <div key={cat.label} className="mb-1.5">
              <p className="text-[9px] font-semibold text-tertiary uppercase tracking-wider mb-0.5 px-0.5">{cat.label}</p>
              <div className="flex flex-wrap gap-0.5">
                {cat.emojis.map((emoji) => (
                  <button
                    key={emoji}
                    onClick={() => { onSelect(emoji); setOpen(false); }}
                    className="w-6 h-6 flex items-center justify-center rounded hover:bg-elevated text-sm"
                  >
                    {emoji}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
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

function MessageBubble({
  message,
  onReply,
  onEdit,
  onForward,
  onPin,
}: {
  message: ChatMessage;
  onReply?: (msg: ChatMessage) => void;
  onEdit?: (msg: ChatMessage) => void;
  onForward?: (msg: ChatMessage) => void;
  onPin?: (msg: ChatMessage) => void;
}) {
  const time = new Date(message.timestamp * 1000).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
  const kind = msgTypeLabel(message.msg_type);
  const { baseUrl, token, connected } = useServerStore();
  const updateMessage = useChatStore((s) => s.updateMessage);
  const currentSipUri = useAccountStore((s) => s.account?.sipUri);

  const handleDelete = async () => {
    if (!connected || !baseUrl || !token) return;
    const { paleServerDeleteMessage } = await import("@/lib/tauri");
    paleServerDeleteMessage(baseUrl, token, message.event_id).catch(() => {});
  };

  const handleReaction = async (emoji: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/messages/${message.event_id}/react`, {
        method: "POST",
        body: { emoji },
      });
      // Optimistically update reactions
      const uri = currentSipUri ? `sip:${currentSipUri}` : "me";
      const reactions = { ...(message.reactions ?? {}) };
      const existing = reactions[emoji] ?? [];
      if (existing.includes(uri)) {
        reactions[emoji] = existing.filter((u) => u !== uri);
        if (reactions[emoji].length === 0) delete reactions[emoji];
      } else {
        reactions[emoji] = [...existing, uri];
      }
      updateMessage(message.room_id, message.event_id, { reactions });
    } catch { /* ignore */ }
  };

  const renderedBody = renderMarkdown(message.body);

  return (
    <div
      className={cn(
        "flex group/msg",
        message.is_own ? "justify-end" : "justify-start"
      )}
    >
      {/* Hover actions — own messages (left side) */}
      {connected && (
        <div className={cn(
          "flex items-center gap-0.5 opacity-0 group-hover/msg:opacity-100 transition-opacity",
          message.is_own ? "mr-1" : "order-last ml-1"
        )}>
          {/* Reply */}
          <button
            onClick={() => onReply?.(message)}
            className="p-0.5 rounded text-tertiary hover:text-accent"
            title="Reply"
          >
            <Reply size={12} />
          </button>
          {/* Edit (own only) */}
          {message.is_own && (
            <button
              onClick={() => onEdit?.(message)}
              className="p-0.5 rounded text-tertiary hover:text-accent"
              title="Edit"
            >
              <Pencil size={12} />
            </button>
          )}
          {/* Pin/Unpin */}
          <button
            onClick={() => onPin?.(message)}
            className={cn("p-0.5 rounded", message.pinned ? "text-accent" : "text-tertiary hover:text-accent")}
            title={message.pinned ? "Unpin" : "Pin"}
          >
            <Pin size={12} />
          </button>
          {/* Forward */}
          <button
            onClick={() => onForward?.(message)}
            className="p-0.5 rounded text-tertiary hover:text-accent"
            title="Forward"
          >
            <Forward size={12} />
          </button>
          {/* Delete (own only) */}
          {message.is_own && (
            <button
              onClick={handleDelete}
              className="p-0.5 rounded text-tertiary hover:text-destructive"
              title="Delete"
            >
              <X size={12} />
            </button>
          )}
          {/* Quick reactions (others' messages) */}
          {!message.is_own && QUICK_REACTIONS.map((emoji) => (
            <button
              key={emoji}
              onClick={() => handleReaction(emoji)}
              className="p-0.5 rounded hover:bg-elevated text-xs"
              title={emoji}
            >
              {emoji}
            </button>
          ))}
          {!message.is_own && (
            <EmojiPickerButton onSelect={handleReaction} />
          )}
        </div>
      )}
      <div className="max-w-[80%]">
        <div
          className={cn(
            "rounded-2xl px-3 py-2 relative",
            message.is_own
              ? "bg-accent text-white rounded-br-md"
              : "bg-surface border border-border-subtle text-primary rounded-bl-md"
          )}
        >
          {/* Pin indicator */}
          {message.pinned && (
            <div className="flex items-center gap-1 text-[9px] text-accent mb-1">
              <Pin size={9} /> Pinned
            </div>
          )}

          {/* Reply preview */}
          {message.reply_preview && (
            <div className={cn(
              "rounded px-2 py-1 mb-1.5 text-[10px] border-l-2",
              message.is_own
                ? "bg-white/10 border-white/40"
                : "bg-elevated border-accent"
            )}>
              <p className="font-semibold truncate">{message.reply_preview.sender}</p>
              <p className="truncate opacity-70">{message.reply_preview.body}</p>
            </div>
          )}

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
          <div
            className="text-sm whitespace-pre-wrap break-words"
            dangerouslySetInnerHTML={{ __html: renderedBody }}
          />
          <p
            className={cn(
              "text-[9px] mt-1 flex items-center gap-1",
              message.is_own ? "text-white/60" : "text-tertiary"
            )}
          >
            {time}
            {message.edited_at && <span>(edited)</span>}
            {/* Read receipt indicators (own messages only) */}
            {message.is_own && (
              <span className="inline-flex items-center ml-0.5">
                {message.reactions && Object.keys(message.reactions).length > 0 ? (
                  <CheckCheck size={10} className="text-blue-300" />
                ) : (
                  <Check size={10} />
                )}
              </span>
            )}
          </p>
        </div>

        {/* Reactions display */}
        {message.reactions && Object.keys(message.reactions).length > 0 && (
          <div className={cn("flex flex-wrap gap-1 mt-1", message.is_own ? "justify-end" : "justify-start")}>
            {Object.entries(message.reactions).map(([emoji, users]) => {
              const ownUri = currentSipUri ? `sip:${currentSipUri}` : "";
              const isOwn = users.includes(ownUri);
              return (
                <button
                  key={emoji}
                  onClick={() => handleReaction(emoji)}
                  className={cn(
                    "inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full text-xs border transition-colors",
                    isOwn
                      ? "border-accent bg-accent/10 text-accent"
                      : "border-border-subtle bg-surface text-secondary hover:border-accent"
                  )}
                >
                  <span>{emoji}</span>
                  <span className="text-[10px]">{users.length}</span>
                </button>
              );
            })}
          </div>
        )}
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
