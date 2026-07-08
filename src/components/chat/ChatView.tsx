import { useState, useRef, useEffect, useCallback, type ReactNode } from "react";
import { Send, Paperclip, MessageSquare, FileIcon, ImageIcon, Plus, X, Loader2, Phone, Video, Users, UserPlus, Reply, Pencil, Pin, Forward, Check, CheckCheck, Search, Radio, Hash, CalendarClock, Star, AlertTriangle, Plug, Copy, Trash2, Clock, Image as ImageLucide, Languages, Bold, Italic, Code, Link, BookOpen, ListTodo, ExternalLink, MessagesSquare, Smile } from "lucide-react";
import { cn } from "@/lib/cn";
import { useChatStore, type ChatMessage, type RoomSummary } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import { preflightSipCall } from "@/lib/callTargets";
import { matrixSendMessage, matrixSetTyping, matrixCreateDm, makeCall as ipcMakeCall, makeVideoCall as ipcMakeVideoCall, getConfig, getSipPassword, paleLogin, paleServerApi, paleServerPinMessage, paleServerSaveMessage, paleServerMarkRead, paleServerGetUsers, paleServerSetTyping, paleServerUploadFile, paleServerGetRooms, paleServerCreateRoom, paleServerCreateDirectRoom, paleServerStartRoomCall, paleServerEndRoomCall, paleServerGetConferences, paleServerGetMeetings, paleServerCreateMeeting, paleServerGetRingGroups, paleServerGetQueues, paleServerGetPagingGroups, paleServerSearchCollaboration, paleServerGetRoomMessages, paleServerGetRoomMessageState, paleServerSendRoomMessage, paleServerScheduleRoomMessage, paleServerGetChannelWebhooks, paleServerCreateChannelWebhook, paleServerUpdateChannelWebhook, paleServerDeleteChannelWebhook, paleServerEditMessage, paleServerDeleteMessage, paleServerGetNotificationPreference, paleServerSetNotificationPreference, paleServerSearchGifs, paleServerGetTags, paleServerTranslate, paleServerGetCustomEmojis, paleServerGetWikiPages, paleServerCreateWikiPage, paleServerUpdateWikiPage, paleServerDeleteWikiPage, paleServerGetTaskBoards, paleServerCreateTaskBoard, paleServerGetTasks, paleServerCreateTask, paleServerUpdateTask, paleServerGetRoomThreads, paleServerGetThreadMessages, paleServerReplyToThread, type ServerRoom, type ServerUser, type ServerRoomMessage, type ServerRoomMessageState, type ServerMeeting, type ConferenceSummary, type RingGroupSummary, type CallQueueSummary, type PagingGroupSummary, type ServerCollaborationSearchResult, type ServerChannelWebhook, type GifResult, type ServerTag, type CustomEmoji, type WikiPage, type TaskBoard, type TaskItem, type ServerMessageThread } from "@/lib/tauri";
import { joinScheduledMeeting } from "@/lib/meetingJoin";
import { toast } from "@/components/ui/Toast";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { EncryptionBadge } from "@/components/encryption/EncryptionBadge";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

const QUICK_REACTIONS = ["\u{1F44D}", "\u{2764}\u{FE0F}", "\u{1F602}", "\u{1F44F}", "\u{1F914}"];

interface LoopComponentItem {
  id: string;
  room_id: string;
  component_type: "checklist" | "table" | "paragraph";
  data: any;
  created_by: string;
  created_at: string;
  updated_at: string;
}

type ImmersiveFontSize = "small" | "medium" | "large" | "xlarge";
type ImmersiveColumnWidth = "narrow" | "medium" | "wide";

function getImmersivePrefs() {
  try {
    const stored = localStorage.getItem("pale.immersiveReader");
    if (stored) return JSON.parse(stored);
  } catch { /* ignore */ }
  return { fontSize: "medium", lineSpacing: 1.6, dyslexiaFont: false, columnWidth: "medium", highContrast: false };
}

function setImmersivePrefs(prefs: any) {
  localStorage.setItem("pale.immersiveReader", JSON.stringify(prefs));
}
const NIL_UUID = "00000000-0000-0000-0000-000000000000";
const ROOM_HISTORY_PAGE_SIZE = 50;

function getChatDensity(): "compact" | "comfortable" | "spacious" {
  return (localStorage.getItem("pale.chatDensity") as any) || "comfortable";
}

function chatDensitySpacing(): string {
  const density = getChatDensity();
  switch (density) {
    case "compact":
      return "space-y-0.5";
    case "spacious":
      return "space-y-4";
    default:
      return "space-y-2";
  }
}

function chatDensityBubble(): string {
  const density = getChatDensity();
  switch (density) {
    case "compact":
      return "px-2 py-1 text-xs";
    case "spacious":
      return "px-4 py-3 text-base";
    default:
      return "px-3 py-2 text-sm";
  }
}

function mapServerRoomMessages(
  msgs: ServerRoomMessage[],
  roomId: string,
  currentSipUri: string | undefined,
  messageStates: ServerRoomMessageState[] = [],
): ChatMessage[] {
  const stateByMessage = new Map(messageStates.map((state) => [state.message_id, state]));
  return msgs.map((msg) => {
    const messageState = stateByMessage.get(msg.id);
    const reactions = (messageState?.reactions ?? []).reduce<Record<string, string[]>>(
      (acc, reaction) => {
        acc[reaction.emoji] = [...(acc[reaction.emoji] ?? []), reaction.user_uri];
        return acc;
      },
      {}
    );
    return {
      event_id: msg.id,
      room_id: roomId,
      sender: msg.sender_uri,
      sender_name: null,
      body: msg.body,
      msg_type: "text",
      timestamp: Math.floor(new Date(msg.created_at).getTime() / 1000),
      is_encrypted: false,
      is_own: currentSipUri != null && msg.sender_uri === currentSipUri,
      reply_to: msg.reply_to,
      edited_at: msg.edited_at ? Math.floor(new Date(msg.edited_at).getTime() / 1000) : undefined,
      pinned: msg.pinned,
      priority: msg.priority ?? "normal",
      saved_by: msg.saved_by ?? [],
      mentions: msg.mentions ?? [],
      mentioned_user_uris: msg.mentioned_user_uris ?? [],
      reactions,
      read_by: (messageState?.reads ?? []).map((read) => read.reader_uri),
      delivery_status: msg.delivery_status ?? "sent",
      scheduled_at: msg.scheduled_at,
      card_payload: msg.card_payload ?? null,
      thread_id: msg.thread_id ?? null,
    } satisfies ChatMessage;
  });
}

const EMOJI_CATEGORIES: { label: string; emojis: string[] }[] = [
  { label: "Smileys", emojis: ["\u{1F600}", "\u{1F603}", "\u{1F604}", "\u{1F601}", "\u{1F606}", "\u{1F605}", "\u{1F602}", "\u{1F923}", "\u{1F60A}", "\u{1F607}", "\u{1F970}", "\u{1F60D}", "\u{1F618}", "\u{1F617}", "\u{1F914}", "\u{1F928}"] },
  { label: "Gestures", emojis: ["\u{1F44D}", "\u{1F44E}", "\u{1F44F}", "\u{1F64C}", "\u{1F4AA}", "\u{270C}\u{FE0F}", "\u{1F91E}", "\u{1F44B}", "\u{1F64F}", "\u{1F91D}"] },
  { label: "Objects", emojis: ["\u{2764}\u{FE0F}", "\u{1F525}", "\u{2B50}", "\u{1F389}", "\u{1F388}", "\u{1F381}", "\u{1F4A1}", "\u{1F4AC}", "\u{1F514}", "\u{1F3B5}", "\u{1F680}", "\u{2705}", "\u{274C}"] },
];

function directRoomName(room: ServerRoom, currentSipUri?: string | null): string {
  if (!room.is_direct || !currentSipUri) return room.name;
  const current = normalizeSipUri(currentSipUri);
  const other = room.members.find((member) => normalizeSipUri(member.user_sip_uri) !== current);
  return other?.user_sip_uri.replace(/^sip:/, "") ?? room.name;
}

function normalizeSipUri(uri?: string | null): string {
  const trimmed = (uri ?? "").trim().toLowerCase();
  if (!trimmed) return "";
  return trimmed.startsWith("sip:") || trimmed.startsWith("sips:")
    ? trimmed.replace(/^sips:/, "sip:")
    : `sip:${trimmed}`;
}

function serverRoomToSummary(room: ServerRoom, currentSipUri?: string | null, nameOverride?: string): RoomSummary {
  return {
    room_id: room.id,
    team_id: room.team_id ?? null,
    channel_name: room.channel_name ?? null,
    channel_type: room.channel_type ?? "standard",
    channel_owners: room.channel_owners ?? [],
    posting_policy: room.posting_policy ?? "members",
    name: nameOverride ?? directRoomName(room, currentSipUri),
    is_direct: room.is_direct,
    is_encrypted: false,
    created_by: room.created_by,
    last_message: null,
    last_message_sender: null,
    last_message_ts: null,
    unread_count: 0,
    members: room.members.map((member) => member.user_sip_uri),
    call_uri: room.call_uri ?? null,
    conference_id: room.conference_id ?? null,
  };
}

async function resolvePaleServerSession(current: {
  baseUrl: string | null;
  token: string | null;
}): Promise<{ baseUrl: string; token: string } | null> {
  if (current.baseUrl && current.token) {
    return { baseUrl: current.baseUrl, token: current.token };
  }

  const config = await getConfig().catch(() => null);
  if (!config?.server?.url || !config.server.username || !config.server.auto_connect) {
    return null;
  }

  const password = await getSipPassword("pale-server-login").catch(() => null);
  if (!password) return null;

  const response = await paleLogin(config.server.url, config.server.username, password);
  sessionStorage.setItem("pale.admin.token", response.token);
  useServerStore
    .getState()
    .setConnection(
      config.server.url,
      response.token,
      response.expires_at,
      response.user.role,
      response.user.display_name,
    );
  return { baseUrl: config.server.url, token: response.token };
}

function sipUriForExtension(extension: string, currentSipUri?: string | null): string {
  if (extension.startsWith("sip:")) return extension;
  const domain = currentSipUri?.split("@")[1] ?? "pale.local";
  return `sip:${extension}@${domain}`;
}

function collaborationIcon(kind: ServerCollaborationSearchResult["kind"]): ReactNode {
  switch (kind) {
    case "channel":
      return <Hash size={15} />;
    case "team":
      return <Users size={15} />;
    case "meeting":
      return <CalendarClock size={15} />;
    case "conference":
      return <Video size={15} />;
    case "direct":
      return <MessageSquare size={15} />;
    default:
      return <MessageSquare size={15} />;
  }
}

function groupMatches(query: string, ...values: Array<string | string[] | undefined | null>): boolean {
  if (!query) return true;
  return values.some((value) => {
    if (!value) return false;
    const text = Array.isArray(value) ? value.join(" ") : value;
    return text.toLowerCase().includes(query);
  });
}

function meetingTimeLabel(meeting: ServerMeeting): string {
  const starts = new Date(meeting.starts_at);
  const ends = new Date(meeting.ends_at);
  return `${starts.toLocaleDateString([], { month: "short", day: "numeric" })} ${starts.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}-${ends.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
}

function datetimeLocalValue(date: Date): string {
  const offsetMs = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offsetMs).toISOString().slice(0, 16);
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
  const account = useAccountStore((s) => s.account);
  const currentSipUri = account?.sipUri;

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
  const [query, setQuery] = useState("");
  const [meetings, setMeetings] = useState<ServerMeeting[]>([]);
  const [conferences, setConferences] = useState<ConferenceSummary[]>([]);
  const [ringGroups, setRingGroups] = useState<RingGroupSummary[]>([]);
  const [queues, setQueues] = useState<CallQueueSummary[]>([]);
  const [pagingGroups, setPagingGroups] = useState<PagingGroupSummary[]>([]);
  const [collaborationResults, setCollaborationResults] = useState<ServerCollaborationSearchResult[]>([]);
  const [collaborationSearchLoading, setCollaborationSearchLoading] = useState(false);
  const [focusedTeam, setFocusedTeam] = useState<{ id: string; name: string } | null>(null);

  const { baseUrl, token, connected } = useServerStore();
  const currentSipUri = useAccountStore((s) => s.account?.sipUri);
  const upsertRoom = useChatStore((s) => s.upsertRoom);

  useEffect(() => {
    if (!connected || !baseUrl || !token) {
      setMeetings([]);
      setConferences([]);
      setRingGroups([]);
      setQueues([]);
      setPagingGroups([]);
      return;
    }

    let cancelled = false;
    Promise.allSettled([
      paleServerGetMeetings(baseUrl, token),
      paleServerGetConferences(baseUrl, token),
      paleServerGetRingGroups(baseUrl, token),
      paleServerGetQueues(baseUrl, token),
      paleServerGetPagingGroups(baseUrl, token),
    ]).then(([meetingResult, conferenceResult, ringResult, queueResult, pagingResult]) => {
      if (cancelled) return;
      setMeetings(meetingResult.status === "fulfilled" ? meetingResult.value : []);
      setConferences(conferenceResult.status === "fulfilled" ? conferenceResult.value : []);
      setRingGroups(ringResult.status === "fulfilled" ? ringResult.value : []);
      setQueues(queueResult.status === "fulfilled" ? queueResult.value : []);
      setPagingGroups(pagingResult.status === "fulfilled" ? pagingResult.value : []);
    });

    return () => { cancelled = true; };
  }, [connected, baseUrl, token]);

  useEffect(() => {
    const searchTerm = query.trim();
    if (!connected || !baseUrl || !token || searchTerm.length < 2) {
      setCollaborationResults([]);
      setCollaborationSearchLoading(false);
      return;
    }

    let cancelled = false;
    setCollaborationSearchLoading(true);
    const timer = window.setTimeout(() => {
      paleServerSearchCollaboration(baseUrl, token, searchTerm, 20)
        .then((results) => {
          if (!cancelled) setCollaborationResults(results);
        })
        .catch(() => {
          if (!cancelled) setCollaborationResults([]);
        })
        .finally(() => {
          if (!cancelled) setCollaborationSearchLoading(false);
        });
    }, 250);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [connected, baseUrl, token, query]);

  useEffect(() => {
    const handleMeetingScheduled = (event: Event) => {
      const meeting = (event as CustomEvent<ServerMeeting>).detail;
      if (!meeting?.id) return;
      setMeetings((existing) => [...existing.filter((item) => item.id !== meeting.id), meeting]);
    };
    window.addEventListener("pale:meeting-scheduled", handleMeetingScheduled);
    return () => window.removeEventListener("pale:meeting-scheduled", handleMeetingScheduled);
  }, []);

  const handleNewDm = async (user: { display_name: string; sip_uri: string; matrix_user_id?: string | null }) => {
    try {
      const session = await resolvePaleServerSession({ baseUrl, token });
      if (session) {
        const room = await paleServerCreateDirectRoom(session.baseUrl, session.token, user);
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

  const handleCreateMeeting = async (input: {
    title: string;
    starts_at: string;
    ends_at: string;
    participants: string[];
    mode: "audio" | "video";
  }) => {
    if (!connected || !baseUrl || !token) {
      toast({ type: "error", title: "Not connected to server" });
      return;
    }
    try {
      const meeting = await paleServerCreateMeeting(baseUrl, token, input);
      setMeetings((existing) => [...existing.filter((item) => item.id !== meeting.id), meeting]);
      setShowNewChat(false);
      toast({ type: "success", title: "Meeting scheduled" });
    } catch (err) {
      toast({ type: "error", title: "Could not schedule meeting", description: String(err) });
    }
  };

  const normalizedQuery = query.trim().toLowerCase();
  const scopedRooms = focusedTeam ? rooms.filter((room) => room.team_id === focusedTeam.id) : rooms;
  const filteredRooms = scopedRooms.filter((room) =>
    groupMatches(normalizedQuery, room.name, room.channel_name, room.last_message ?? undefined)
  );
  const filteredMeetings = meetings
    .filter((meeting) =>
      groupMatches(normalizedQuery, meeting.title, meeting.description, meeting.participants)
    )
    .sort((left, right) => new Date(left.starts_at).getTime() - new Date(right.starts_at).getTime());
  const filteredConferences = conferences.filter((conf) =>
    groupMatches(normalizedQuery, conf.title, conf.mode, conf.participants.map((p) => p.sip_uri))
  );
  const filteredRingGroups = ringGroups.filter((group) =>
    groupMatches(normalizedQuery, group.name, group.extension, group.members)
  );
  const filteredQueues = queues.filter((queue) =>
    groupMatches(normalizedQuery, queue.name, queue.extension, queue.agents.map((agent) => agent.agent_uri))
  );
  const filteredPagingGroups = pagingGroups.filter((group) =>
    groupMatches(normalizedQuery, group.name, group.extension, group.members)
  );
  const hasAnyGroupResults = filteredMeetings.length > 0 || filteredConferences.length > 0 || filteredRingGroups.length > 0 || filteredQueues.length > 0 || filteredPagingGroups.length > 0;
  const visibleCollaborationResults = normalizedQuery
    ? collaborationResults.filter((result) => !result.room_id || !filteredRooms.some((room) => room.room_id === result.room_id))
    : [];

  const joinConference = async (conf: ConferenceSummary) => {
    if (!connected || !baseUrl || !token) return;
    try {
      const resp = await paleServerApi<ConferenceSummary & { livekit_url?: string; livekit_token?: string }>(baseUrl, token, `/v1/conferences/${conf.id}/participants`, {
        method: "POST",
        body: {
          user_id: NIL_UUID,
          sip_uri: currentSipUri ?? "sip:unknown@local",
          role: "member",
        },
      });
      // When LiveKit is configured, store media credentials and skip SIP call
      if (resp.livekit_url && resp.livekit_token) {
        const { useMeetingStore } = await import("@/store/meetingStore");
        useMeetingStore.getState().setActiveConferenceId(conf.id);
        useMeetingStore.getState().setLiveKitCredentials(resp.livekit_url, resp.livekit_token);
      } else {
        await ipcMakeCall(`sip:conf-${conf.id}@pale.local`);
      }
    } catch (err) {
      toast({ type: "error", title: "Failed to join conference", description: String(err) });
    }
  };

  const joinMeeting = async (meeting: ServerMeeting) => {
    if (!connected || !baseUrl || !token) return;
    try {
      await joinScheduledMeeting(baseUrl, token, meeting);
    } catch (err) {
      toast({ type: "error", title: "Failed to join meeting", description: String(err) });
    }
  };

  const callExtension = (extension: string, label: string) => {
    ipcMakeCall(sipUriForExtension(extension, currentSipUri))
      .catch(() => toast({ type: "error", title: `Failed to call ${label}` }));
  };

  const openRoomResult = async (roomId: string) => {
    if (rooms.some((room) => room.room_id === roomId)) {
      onSelect(roomId);
      return;
    }
    if (!connected || !baseUrl || !token) return;
    try {
      const serverRooms = await paleServerGetRooms(baseUrl, token);
      for (const room of serverRooms) {
        upsertRoom(serverRoomToSummary(room, currentSipUri));
      }
      if (serverRooms.some((room) => room.id === roomId)) {
        onSelect(roomId);
      }
    } catch (err) {
      toast({ type: "error", title: "Could not open chat", description: String(err) });
    }
  };

  const openTeamResult = async (teamId: string, teamName: string) => {
    if (!connected || !baseUrl || !token) return;
    try {
      const serverRooms = await paleServerGetRooms(baseUrl, token);
      for (const room of serverRooms) {
        upsertRoom(serverRoomToSummary(room, currentSipUri));
      }
      setFocusedTeam({ id: teamId, name: teamName });
      setQuery("");
    } catch (err) {
      toast({ type: "error", title: "Could not open team", description: String(err) });
    }
  };

  const openCollaborationResult = async (result: ServerCollaborationSearchResult) => {
    if (result.room_id) {
      await openRoomResult(result.room_id);
      return;
    }
    if (result.kind === "team" && result.team_id) {
      await openTeamResult(result.team_id, result.title);
      return;
    }
    if (result.kind === "meeting") {
      if (!connected || !baseUrl || !token) return;
      try {
        await joinScheduledMeeting(baseUrl, token, { id: result.id });
      } catch (err) {
        toast({ type: "error", title: "Failed to start meeting", description: String(err) });
      }
      return;
    }
    if (result.call_uri) {
      ipcMakeCall(result.call_uri)
        .catch(() => toast({ type: "error", title: `Failed to call ${result.title}` }));
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

      {showNewChat && (
        <NewChatInput
          onSubmit={handleNewDm}
          onCreateRoom={handleCreateRoom}
          onCreateMeeting={handleCreateMeeting}
        />
      )}

      <div className="px-4 pb-2">
        {focusedTeam && (
          <div className="mb-2 flex items-center justify-between rounded-lg border border-border-subtle bg-elevated px-3 py-2">
            <div className="min-w-0">
              <p className="text-xs font-semibold uppercase text-tertiary">Team</p>
              <p className="truncate text-sm font-medium text-primary">{focusedTeam.name}</p>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              <button
                onClick={async () => {
                  const email = prompt("Guest email:");
                  if (!email) return;
                  const displayName = prompt("Guest display name:") || email;
                  if (!connected || !baseUrl || !token) return;
                  try {
                    await paleServerApi(baseUrl, token, `/v1/teams/${focusedTeam.id}/guests/invite`, {
                      method: "POST",
                      body: { email, display_name: displayName, permissions: ["chat.read", "chat.write"] },
                    });
                    toast({ type: "success", title: "Guest invited" });
                  } catch { toast({ type: "error", title: "Failed to invite guest" }); }
                }}
                className="rounded-md p-1.5 text-tertiary transition-colors hover:bg-surface hover:text-primary"
                title="Invite guest"
              >
                <UserPlus size={14} />
              </button>
              <button
                onClick={() => setFocusedTeam(null)}
                className="rounded-md p-1.5 text-tertiary transition-colors hover:bg-surface hover:text-primary"
                title="Show all conversations"
              >
                <X size={14} />
              </button>
            </div>
          </div>
        )}
        <div className="relative">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-tertiary" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search chats, call groups, conferences..."
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-lg",
              "pl-9 pr-3 py-2 text-sm text-primary placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus"
            )}
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2">
        {rooms.length === 0 && !hasAnyGroupResults && visibleCollaborationResults.length === 0 && !showNewChat ? (
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
          <>
            {filteredRooms.length > 0 && (
              <div className="pb-2">
                <p className="px-3 py-1 text-[10px] font-semibold uppercase text-tertiary">
                  {focusedTeam ? "Team Channels" : "Conversations"}
                </p>
                {filteredRooms.map((room) => (
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
                        {room.channel_type === "private" && (
                          <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-500">Private</span>
                        )}
                        {room.channel_type === "shared" && (
                          <span className="rounded bg-blue-500/10 px-1.5 py-0.5 text-[10px] text-blue-500">Shared</span>
                        )}
                        {room.posting_policy === "owners" && (
                          <span className="rounded bg-accent/10 px-1.5 py-0.5 text-[10px] text-accent">Moderated</span>
                        )}
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
                ))}
              </div>
            )}

            {focusedTeam && filteredRooms.length === 0 && !normalizedQuery && (
              <div className="flex flex-col items-center justify-center h-32 gap-2">
                <Hash size={24} className="text-tertiary" />
                <p className="text-sm text-tertiary">No channels in this team</p>
              </div>
            )}

            {(visibleCollaborationResults.length > 0 || collaborationSearchLoading) && (
              <div className="pb-2">
                <p className="px-3 py-1 text-[10px] font-semibold uppercase text-tertiary">Directory Results</p>
                {collaborationSearchLoading && (
                  <div className="flex items-center gap-2 px-3 py-2 text-xs text-tertiary">
                    <Loader2 size={13} className="animate-spin" />
                    Searching
                  </div>
                )}
                {visibleCollaborationResults.map((result) => {
                  const canOpen = Boolean(result.room_id || result.team_id || result.call_uri || result.kind === "meeting");
                  return (
                    <GroupResultButton
                      key={`${result.kind}-${result.id}`}
                      title={result.title}
                      subtitle={`${result.kind} · ${result.subtitle || "Business collaboration"}`}
                      icon={collaborationIcon(result.kind)}
                      actionLabel={result.kind === "meeting" || result.kind === "conference" ? "Join" : "Open"}
                      disabled={!canOpen}
                      onAction={() => openCollaborationResult(result)}
                    />
                  );
                })}
              </div>
            )}

            {filteredMeetings.length > 0 && (
              <div className="pb-2">
                <p className="px-3 py-1 text-[10px] font-semibold uppercase text-tertiary">Upcoming Meetings</p>
                {filteredMeetings.map((meeting) => (
                  <GroupResultButton
                    key={meeting.id}
                    title={meeting.title}
                    subtitle={`${meetingTimeLabel(meeting)} · ${meeting.participants.length} participant${meeting.participants.length === 1 ? "" : "s"}`}
                    icon={<CalendarClock size={15} />}
                    actionLabel="Join"
                    onAction={() => joinMeeting(meeting)}
                  />
                ))}
              </div>
            )}

            {filteredConferences.length > 0 && (
              <div className="pb-2">
                <p className="px-3 py-1 text-[10px] font-semibold uppercase text-tertiary">Conferences</p>
                {filteredConferences.map((conf) => (
                  <GroupResultButton
                    key={conf.id}
                    title={conf.title}
                    subtitle={`${conf.active ? "Active" : "Available"} ${conf.mode} conference · ${conf.participants.length} participant${conf.participants.length === 1 ? "" : "s"}`}
                    icon={<Users size={15} />}
                    actionLabel="Join"
                    onAction={() => joinConference(conf)}
                  />
                ))}
              </div>
            )}

            {(filteredRingGroups.length > 0 || filteredQueues.length > 0 || filteredPagingGroups.length > 0) && (
              <div className="pb-2">
                <p className="px-3 py-1 text-[10px] font-semibold uppercase text-tertiary">Call Groups</p>
                {filteredRingGroups.map((group) => (
                  <GroupResultButton
                    key={group.id}
                    title={group.name}
                    subtitle={`Ring group ${group.extension} · ${group.members.length} member${group.members.length === 1 ? "" : "s"}`}
                    icon={<Phone size={15} />}
                    actionLabel="Call"
                    disabled={!group.enabled}
                    onAction={() => callExtension(group.extension, group.name)}
                  />
                ))}
                {filteredQueues.map((queue) => (
                  <GroupResultButton
                    key={queue.id}
                    title={queue.name}
                    subtitle={`Queue ${queue.extension} · ${queue.agents.length} agent${queue.agents.length === 1 ? "" : "s"}`}
                    icon={<Users size={15} />}
                    actionLabel="Call"
                    disabled={!queue.enabled}
                    onAction={() => callExtension(queue.extension, queue.name)}
                  />
                ))}
                {filteredPagingGroups.map((group) => (
                  <GroupResultButton
                    key={group.id}
                    title={group.name}
                    subtitle={`Paging group ${group.extension} · ${group.members.length} member${group.members.length === 1 ? "" : "s"}`}
                    icon={<Radio size={15} />}
                    actionLabel="Page"
                    onAction={() => callExtension(group.extension, group.name)}
                  />
                ))}
              </div>
            )}

            {normalizedQuery && filteredRooms.length === 0 && !hasAnyGroupResults && visibleCollaborationResults.length === 0 && !collaborationSearchLoading && (
              <div className="flex flex-col items-center justify-center h-32 gap-2">
                <Search size={24} className="text-tertiary" />
                <p className="text-sm text-tertiary">No groups or chats found</p>
              </div>
            )}
          </>
        )}
      </div>
    </>
  );
}

function GroupResultButton({
  title,
  subtitle,
  icon,
  actionLabel,
  disabled,
  onAction,
}: {
  title: string;
  subtitle: string;
  icon: ReactNode;
  actionLabel: string;
  disabled?: boolean;
  onAction: () => void;
}) {
  return (
    <div className="w-full flex items-center gap-3 px-3 py-3 rounded-lg hover:bg-elevated transition-colors">
      <div className="w-8 h-8 rounded-full bg-accent-muted text-accent flex items-center justify-center shrink-0">
        {icon}
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-primary truncate">{title}</p>
        <p className="text-xs text-tertiary truncate">{subtitle}</p>
      </div>
      <button
        onClick={onAction}
        disabled={disabled}
        className={cn(
          "shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors",
          disabled
            ? "bg-elevated text-tertiary cursor-not-allowed"
            : "bg-success/10 text-success hover:bg-success/20"
        )}
      >
        <Phone size={12} />
        {actionLabel}
      </button>
    </div>
  );
}

function ConnectorDialog({
  baseUrl,
  token,
  roomId,
  roomName,
  onClose,
}: {
  baseUrl: string;
  token: string;
  roomId: string;
  roomName: string;
  onClose: () => void;
}) {
  const [webhooks, setWebhooks] = useState<ServerChannelWebhook[]>([]);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [createdUrl, setCreatedUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    try {
      setWebhooks(await paleServerGetChannelWebhooks(baseUrl, token, roomId));
    } catch (error) {
      toast({ type: "error", title: "Unable to load connectors", description: String(error) });
    }
  }, [baseUrl, token, roomId]);

  useEffect(() => { load(); }, [load]);

  const createWebhook = async () => {
    if (!name.trim()) return;
    setLoading(true);
    try {
      const response = await paleServerCreateChannelWebhook(baseUrl, token, roomId, {
        name: name.trim(),
        description: description.trim() || undefined,
      });
      setCreatedUrl(`${baseUrl.replace(/\/$/, "")}/v1/webhooks/${response.token}`);
      setName("");
      setDescription("");
      await load();
    } catch (error) {
      toast({ type: "error", title: "Unable to create connector", description: String(error) });
    } finally {
      setLoading(false);
    }
  };

  const copyUrl = async (value: string) => {
    await navigator.clipboard?.writeText(value);
    toast({ type: "success", title: "Copied" });
  };

  const setEnabled = async (webhook: ServerChannelWebhook, enabled: boolean) => {
    try {
      await paleServerUpdateChannelWebhook(baseUrl, token, roomId, webhook.id, enabled);
      await load();
    } catch (error) {
      toast({ type: "error", title: "Unable to update connector", description: String(error) });
    }
  };

  const remove = async (webhook: ServerChannelWebhook) => {
    try {
      await paleServerDeleteChannelWebhook(baseUrl, token, roomId, webhook.id);
      await load();
    } catch (error) {
      toast({ type: "error", title: "Unable to delete connector", description: String(error) });
    }
  };

  return (
    <div className="absolute inset-0 z-40 flex items-center justify-center bg-base/70 backdrop-blur-sm px-4">
      <div className="w-full max-w-lg rounded-lg border border-border-default bg-surface shadow-xl">
        <div className="flex items-center justify-between gap-3 border-b border-border-subtle px-4 py-3">
          <div className="min-w-0">
            <div className="text-sm font-semibold truncate">Connectors</div>
            <div className="text-xs text-tertiary truncate">{roomName}</div>
          </div>
          <button onClick={onClose} className="p-1.5 rounded-md text-tertiary hover:text-primary hover:bg-elevated">
            <X size={16} />
          </button>
        </div>
        <div className="p-4 space-y-4">
          <div className="grid gap-2">
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Connector name"
              className="h-9 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
            />
            <input
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              placeholder="Description"
              className="h-9 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
            />
            <button
              onClick={createWebhook}
              disabled={!name.trim() || loading}
              className="h-9 rounded-md bg-accent text-white text-sm font-medium disabled:opacity-50"
            >
              {loading ? "Creating..." : "Create"}
            </button>
          </div>

          {createdUrl && (
            <div className="rounded-md border border-border-subtle bg-base p-3 space-y-2">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-medium">Webhook URL</span>
                <button onClick={() => copyUrl(createdUrl)} className="text-xs text-accent inline-flex items-center gap-1">
                  <Copy size={12} />
                  Copy
                </button>
              </div>
              <div className="break-all text-xs text-secondary">{createdUrl}</div>
            </div>
          )}

          <div className="space-y-2 max-h-64 overflow-y-auto">
            {webhooks.length === 0 ? (
              <div className="py-6 text-center text-sm text-tertiary">No connectors</div>
            ) : (
              webhooks.map((webhook) => (
                <div key={webhook.id} className="flex items-center gap-3 rounded-md bg-base p-3">
                  <div className="h-8 w-8 rounded-md bg-accent-muted text-accent flex items-center justify-center shrink-0">
                    <Plug size={15} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium truncate">{webhook.name}</div>
                    <div className="text-xs text-tertiary truncate">
                      {webhook.enabled ? "Enabled" : "Disabled"}
                      {webhook.last_used_at ? ` · Used ${new Date(webhook.last_used_at).toLocaleString()}` : ""}
                    </div>
                  </div>
                  <button
                    onClick={() => setEnabled(webhook, !webhook.enabled)}
                    className="h-8 px-2 rounded-md text-xs bg-hover text-secondary hover:text-primary"
                  >
                    {webhook.enabled ? "Disable" : "Enable"}
                  </button>
                  <button
                    onClick={() => remove(webhook)}
                    className="h-8 w-8 rounded-md text-destructive hover:bg-destructive/10 inline-flex items-center justify-center"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
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
  const [messagePriority, setMessagePriority] = useState<"normal" | "high" | "urgent">("normal");
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [mentionUsers, setMentionUsers] = useState<ServerUser[]>([]);
  const [allUsers, setAllUsers] = useState<ServerUser[]>([]);
  const [showConnectors, setShowConnectors] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [showScheduleSend, setShowScheduleSend] = useState(false);
  const [scheduleDate, setScheduleDate] = useState("");
  const [notificationLevel, setNotificationLevel] = useState<"all" | "mentions" | "muted">("all");
  const [showGifPicker, setShowGifPicker] = useState(false);
  const [gifQuery, setGifQuery] = useState("");
  const [gifResults, setGifResults] = useState<GifResult[]>([]);
  const [gifLoading, setGifLoading] = useState(false);
  const [teamTags, setTeamTags] = useState<ServerTag[]>([]);
  const [customEmojis, setCustomEmojis] = useState<CustomEmoji[]>([]);
  const [subTab, setSubTab] = useState<"chat" | "wiki" | "tasks" | "tab">("chat");
  const [immersiveReaderMessage, setImmersiveReaderMessage] = useState<ChatMessage | null>(null);
  const [loopComponents, setLoopComponents] = useState<LoopComponentItem[]>([]);
  const [showLoopInsert, setShowLoopInsert] = useState(false);
  const [channelTabs, setChannelTabs] = useState<{ id: string; name: string; url: string; icon?: string; position: number }[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [slashMenuOpen, setSlashMenuOpen] = useState(false);
  const [slashExtensions, setSlashExtensions] = useState<{ id: string; command: string; name: string; description: string; icon?: string }[]>([]);
  const [slashQuery, setSlashQuery] = useState("");
  // Threading state
  const [threadPanelOpen, setThreadPanelOpen] = useState(false);
  const [activeThreadRootId, setActiveThreadRootId] = useState<string | null>(null);
  const [threadMessages, setThreadMessages] = useState<ChatMessage[]>([]);
  const [threadInput, setThreadInput] = useState("");
  const [threadLoading, setThreadLoading] = useState(false);
  const [roomThreads, setRoomThreads] = useState<ServerMessageThread[]>([]);
  const [showThreadList, setShowThreadList] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<number | null>(null);
  const typingSentRef = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const { baseUrl, token, connected } = useServerStore();
  const account = useAccountStore((s) => s.account);
  const currentSipUri = account?.sipUri;
  const regState = useAccountStore((s) => s.regState);
  const addMessage = useChatStore((s) => s.addMessage);
  const updateMessage = useChatStore((s) => s.updateMessage);
  const upsertRoom = useChatStore((s) => s.upsertRoom);
  const isServerRoom = !room.room_id.startsWith("!");
  const localMemberUri = normalizeSipUri(currentSipUri ?? room.created_by);
  const otherDirectMember = room.members?.find((member) => normalizeSipUri(member) !== localMemberUri);
  const canStartRoomCall =
    isServerRoom &&
    regState === "registered" &&
    (!room.is_direct || Boolean(otherDirectMember));
  const canManageConnectors = isServerRoom && !room.is_direct && (
    room.created_by === currentSipUri
    || room.channel_owners?.includes(currentSipUri ?? "")
  );
  const hasActiveRoomCall = isServerRoom && !room.is_direct && Boolean(room.call_uri);

  // Load server users for mentions
  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    paleServerGetUsers(baseUrl, token).then(setAllUsers).catch(() => {});
  }, [connected, baseUrl, token]);

  // Load notification preference for current room
  useEffect(() => {
    if (!isServerRoom || !connected || !baseUrl || !token) return;
    paleServerGetNotificationPreference(baseUrl, token, room.room_id)
      .then((pref) => setNotificationLevel(pref.notification_level))
      .catch(() => {});
  }, [room.room_id, isServerRoom, connected, baseUrl, token]);

  // Load team tags for mention autocomplete
  useEffect(() => {
    if (!isServerRoom || !connected || !baseUrl || !token || !room.team_id) return;
    paleServerGetTags(baseUrl, token, room.team_id)
      .then(setTeamTags)
      .catch(() => {});
    paleServerGetCustomEmojis(baseUrl, token, room.team_id)
      .then(setCustomEmojis)
      .catch(() => {});
  }, [room.team_id, isServerRoom, connected, baseUrl, token]);

  // Load loop components for room
  useEffect(() => {
    if (!isServerRoom || !connected || !baseUrl || !token) return;
    paleServerApi<LoopComponentItem[]>(baseUrl, token, `/v1/rooms/${room.room_id}/loops`)
      .then(setLoopComponents)
      .catch(() => {});
  }, [room.room_id, isServerRoom, connected, baseUrl, token]);

  // Load channel tabs
  useEffect(() => {
    if (!isServerRoom || !connected || !baseUrl || !token || room.is_direct) {
      setChannelTabs([]);
      return;
    }
    paleServerApi<{ id: string; name: string; url: string; icon?: string; position: number }[]>(
      baseUrl, token, `/v1/rooms/${room.room_id}/tabs`
    ).then(setChannelTabs).catch(() => setChannelTabs([]));
  }, [room.room_id, isServerRoom, room.is_direct, connected, baseUrl, token]);

  // Load message extensions for slash commands
  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    paleServerApi<{ id: string; command: string; name: string; description: string; icon?: string }[]>(
      baseUrl, token, "/v1/message-extensions"
    ).then(setSlashExtensions).catch(() => setSlashExtensions([]));
  }, [connected, baseUrl, token]);

  // Load server room messages on mount (stable deps only — no addMessage to avoid re-render loop)
  useEffect(() => {
    if (isServerRoom && connected && baseUrl && token) {
      Promise.all([
        paleServerGetRoomMessages(baseUrl, token, room.room_id, { limit: ROOM_HISTORY_PAGE_SIZE }),
        paleServerGetRoomMessageState(baseUrl, token, room.room_id).catch(() => []),
      ])
        .then(([msgs, messageStates]) => {
          const mappedMessages = mapServerRoomMessages(msgs, room.room_id, currentSipUri, messageStates);
          useChatStore.getState().setMessages(room.room_id, mappedMessages);
          setHasMore(msgs.length === ROOM_HISTORY_PAGE_SIZE);
        })
        .catch(() => {});
    }
  }, [room.room_id, isServerRoom, connected, baseUrl, token, currentSipUri]);

  useEffect(() => {
    if (loadingHistory) return;
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length, loadingHistory]);

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
    if (!isServerRoom || !container || !connected || !baseUrl || !token || loadingHistory || !hasMore) return;
    if (container.scrollTop > 50) return;

    setLoadingHistory(true);
    const previousScrollHeight = container.scrollHeight;
    const oldest = messagesRef.current[0];
    const before = oldest ? new Date(oldest.timestamp * 1000).toISOString() : undefined;

    try {
      const older = await paleServerGetRoomMessages(baseUrl, token, room.room_id, {
        limit: ROOM_HISTORY_PAGE_SIZE,
        before,
      });
      if (older.length === 0) {
        setHasMore(false);
      } else {
        const messageStates = await paleServerGetRoomMessageState(baseUrl, token, room.room_id).catch(() => []);
        const olderMessages = mapServerRoomMessages(older, room.room_id, currentSipUri, messageStates);
        const currentMessages = useChatStore.getState().messages[room.room_id] ?? [];
        const existingIds = new Set(currentMessages.map((msg) => msg.event_id));
        useChatStore
          .getState()
          .setMessages(room.room_id, [
            ...olderMessages.filter((msg) => !existingIds.has(msg.event_id)),
            ...currentMessages,
          ]);
        window.requestAnimationFrame(() => {
          const updatedContainer = messagesContainerRef.current;
          if (updatedContainer) {
            updatedContainer.scrollTop = updatedContainer.scrollHeight - previousScrollHeight;
          }
        });
        setHasMore(older.length === ROOM_HISTORY_PAGE_SIZE);
      }
    } catch { /* ignore */ }
    setLoadingHistory(false);
  }, [isServerRoom, connected, baseUrl, token, loadingHistory, hasMore, room.room_id, currentSipUri]);

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

    // Detect / slash command
    if (value.startsWith("/") && !value.includes(" ")) {
      const q = value.slice(1).toLowerCase();
      setSlashQuery(q);
      setSlashMenuOpen(true);
    } else {
      setSlashMenuOpen(false);
      setSlashQuery("");
    }
  };

  const invokeSlashCommand = async (command: string) => {
    setSlashMenuOpen(false);
    setSlashQuery("");
    const inputText = input.replace(/^\/\w*/, "").trim();
    setInput("");
    if (!connected || !baseUrl || !token) return;
    try {
      const result = await paleServerApi<{ result?: string; text?: string }>(
        baseUrl, token, `/v1/message-extensions/${command}/invoke`, { method: "POST", body: { input: inputText } }
      );
      const text = result.result || result.text || JSON.stringify(result);
      setInput(text);
    } catch (err) {
      toast({ type: "error", title: "Extension error", description: String(err) });
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
          priority: msg.priority ?? "normal",
          saved_by: msg.saved_by ?? [],
          mentions: msg.mentions ?? [],
          mentioned_user_uris: msg.mentioned_user_uris ?? [],
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
          const updated = await paleServerEditMessage(baseUrl, token, msgId, body);
          updateMessage(room.room_id, msgId, {
            body: updated.body,
            edited_at: updated.edited_at ? Math.floor(new Date(updated.edited_at).getTime() / 1000) : Math.floor(Date.now() / 1000),
            mentions: updated.mentions ?? [],
            mentioned_user_uris: updated.mentioned_user_uris ?? [],
          });
        }
      } catch (err) {
        toast({ type: "error", title: "Edit failed", description: String(err) });
      }
      return;
    }

    // Handle reply or normal send
    try {
      if (isServerRoom && connected && baseUrl && token) {
        const msg = await paleServerSendRoomMessage(baseUrl, token, room.room_id, body, replyingTo?.event_id, messagePriority);
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
          priority: msg.priority ?? messagePriority,
          saved_by: msg.saved_by ?? [],
          mentions: msg.mentions ?? [],
          mentioned_user_uris: msg.mentioned_user_uris ?? [],
        });
      } else {
        await matrixSendMessage(room.room_id, body);
      }
      setMessagePriority("normal");
    } catch (err) {
      toast({ type: "error", title: "Send failed", description: String(err) });
    }
    setReplyingTo(null);
  };

  const handleScheduleSend = async () => {
    if (!input.trim() || !scheduleDate || !isServerRoom || !connected || !baseUrl || !token) return;
    const body = input.trim();
    setInput("");
    setShowScheduleSend(false);
    stopTyping();
    try {
      const scheduledAt = new Date(scheduleDate).toISOString();
      await paleServerScheduleRoomMessage(baseUrl, token, room.room_id, body, scheduledAt, replyingTo?.event_id, messagePriority);
      toast({ type: "success", title: "Message scheduled", description: `Will be sent at ${new Date(scheduleDate).toLocaleString()}` });
      setReplyingTo(null);
      setMessagePriority("normal");
      setScheduleDate("");
    } catch (err) {
      toast({ type: "error", title: "Schedule failed", description: String(err) });
    }
  };

  const handleGifSearch = useCallback(async (query: string) => {
    if (!query.trim() || !connected || !baseUrl || !token) return;
    setGifLoading(true);
    try {
      const result = await paleServerSearchGifs(baseUrl, token, query, 20);
      setGifResults(result.results);
    } catch {
      setGifResults([]);
    } finally {
      setGifLoading(false);
    }
  }, [connected, baseUrl, token]);

  const handleGifSelect = async (gif: GifResult) => {
    setShowGifPicker(false);
    setGifQuery("");
    setGifResults([]);
    if (!isServerRoom || !connected || !baseUrl || !token) return;
    try {
      const body = `![${gif.title}](${gif.url})`;
      const msg = await paleServerSendRoomMessage(baseUrl, token, room.room_id, body, replyingTo?.event_id, "normal");
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
        delivery_status: msg.delivery_status ?? "sent",
      });
      setReplyingTo(null);
    } catch (err) {
      toast({ type: "error", title: "Send failed", description: String(err) });
    }
  };

  const handleNotificationLevelChange = async (level: "all" | "mentions" | "muted") => {
    if (!isServerRoom || !connected || !baseUrl || !token) return;
    setNotificationLevel(level);
    try {
      await paleServerSetNotificationPreference(baseUrl, token, room.room_id, level);
    } catch (err) {
      toast({ type: "error", title: "Failed to update notifications", description: String(err) });
    }
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

  const openThread = async (msg: ChatMessage) => {
    if (!connected || !baseUrl || !token) return;
    setActiveThreadRootId(msg.event_id);
    setThreadPanelOpen(true);
    setThreadLoading(true);
    setShowThreadList(false);
    try {
      // Find the thread for this message, or load thread messages via root message
      const threads = await paleServerGetRoomThreads(baseUrl, token, room.room_id);
      const thread = threads.find((t) => t.root_message_id === msg.event_id);
      if (thread) {
        const msgs = await paleServerGetThreadMessages(baseUrl, token, thread.id);
        setThreadMessages(
          msgs.map((m) => ({
            event_id: m.id,
            room_id: m.room_id,
            sender: m.sender_uri,
            sender_name: null,
            body: m.body,
            msg_type: "text" as const,
            timestamp: Math.floor(new Date(m.created_at).getTime() / 1000),
            is_encrypted: false,
            is_own: currentSipUri != null && m.sender_uri === currentSipUri,
            priority: m.priority ?? "normal",
            delivery_status: m.delivery_status ?? "sent",
          }))
        );
      } else {
        // No thread yet -- just show the root message
        setThreadMessages([msg]);
      }
    } catch {
      setThreadMessages([msg]);
    } finally {
      setThreadLoading(false);
    }
  };

  const handleThreadReply = async () => {
    if (!threadInput.trim() || !activeThreadRootId || !connected || !baseUrl || !token) return;
    const body = threadInput.trim();
    setThreadInput("");
    try {
      const result = await paleServerReplyToThread(baseUrl, token, activeThreadRootId, body, "normal");
      const replyMsg: ChatMessage = {
        event_id: result.message.id,
        room_id: result.message.room_id,
        sender: result.message.sender_uri,
        sender_name: null,
        body: result.message.body,
        msg_type: "text",
        timestamp: Math.floor(new Date(result.message.created_at).getTime() / 1000),
        is_encrypted: false,
        is_own: true,
        priority: result.message.priority ?? "normal",
        delivery_status: result.message.delivery_status ?? "sent",
        thread_id: result.thread.id,
      };
      setThreadMessages((prev) => [...prev, replyMsg]);
      // Update the thread reply count in the room threads list
      setRoomThreads((prev) =>
        prev.map((t) =>
          t.id === result.thread.id ? result.thread : t
        ).concat(prev.find((t) => t.id === result.thread.id) ? [] : [result.thread])
      );
    } catch (err) {
      toast({ type: "error", title: "Reply failed", description: String(err) });
    }
  };

  const loadRoomThreads = async () => {
    if (!connected || !baseUrl || !token) return;
    try {
      const threads = await paleServerGetRoomThreads(baseUrl, token, room.room_id);
      setRoomThreads(threads);
    } catch { /* ignore */ }
  };

  // Load threads for reply count display
  useEffect(() => {
    if (!isServerRoom || !connected || !baseUrl || !token) return;
    loadRoomThreads();
  }, [room.room_id, isServerRoom, connected, baseUrl, token]);

  const handleForward = async (msg: ChatMessage) => {
    const target = window.prompt("Enter room ID or user SIP URI to forward to:");
    if (!target || !connected || !baseUrl || !token) return;
    const senderLabel = msg.sender_name ?? msg.sender;
    const body = `Forwarded from ${senderLabel}:\n${msg.body}`;
    try {
      await paleServerSendRoomMessage(baseUrl, token, target, body, undefined, "normal");
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

  const handleSave = async (msg: ChatMessage) => {
    if (!connected || !baseUrl || !token || !currentSipUri) return;
    const savedBy = msg.saved_by ?? [];
    const shouldSave = !savedBy.includes(currentSipUri);
    try {
      const updated = await paleServerSaveMessage(baseUrl, token, msg.event_id, shouldSave);
      updateMessage(room.room_id, msg.event_id, { saved_by: updated.saved_by ?? [] });
    } catch (err) {
      toast({ type: "error", title: "Save failed", description: String(err) });
    }
  };

  const handleTranslate = async (msg: ChatMessage, targetLanguage: string) => {
    if (!connected || !baseUrl || !token) return;
    try {
      const result = await paleServerTranslate(baseUrl, token, msg.body, targetLanguage);
      updateMessage(room.room_id, msg.event_id, { translated_text: result.translated_text });
    } catch (err) {
      toast({ type: "error", title: "Translation failed", description: String(err) });
    }
  };

  const insertMarkdown = (syntax: "bold" | "italic" | "code" | "link") => {
    const el = inputRef.current;
    if (!el) return;
    const start = el.selectionStart ?? input.length;
    const end = el.selectionEnd ?? input.length;
    const selected = input.slice(start, end);
    let replacement: string;
    let cursorOffset: number;
    switch (syntax) {
      case "bold":
        replacement = `**${selected || "text"}**`;
        cursorOffset = selected ? replacement.length : 2;
        break;
      case "italic":
        replacement = `*${selected || "text"}*`;
        cursorOffset = selected ? replacement.length : 1;
        break;
      case "code":
        replacement = `\`${selected || "code"}\``;
        cursorOffset = selected ? replacement.length : 1;
        break;
      case "link":
        replacement = selected ? `[${selected}](url)` : "[text](url)";
        cursorOffset = selected ? replacement.length - 4 : 1;
        break;
    }
    const newInput = input.slice(0, start) + replacement + input.slice(end);
    setInput(newInput);
    setTimeout(() => {
      el.focus();
      const newPos = start + cursorOffset;
      el.setSelectionRange(newPos, newPos);
    }, 0);
  };

  const insertEmoji = (emoji: string) => {
    const el = inputRef.current;
    if (!el) { setInput(input + emoji); return; }
    const start = el.selectionStart ?? input.length;
    const end = el.selectionEnd ?? input.length;
    const newInput = input.slice(0, start) + emoji + input.slice(end);
    setInput(newInput);
    setTimeout(() => {
      el.focus();
      const newPos = start + emoji.length;
      el.setSelectionRange(newPos, newPos);
    }, 0);
  };

  const handleComposeKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      handleSend();
    } else if (e.ctrlKey || e.metaKey) {
      if (e.key === "b") { e.preventDefault(); insertMarkdown("bold"); }
      else if (e.key === "i") { e.preventDefault(); insertMarkdown("italic"); }
    }
  };

  const startRoomCall = async (mode: "audio" | "video") => {
    try {
      if (room.is_direct) {
        const target = preflightSipCall(otherDirectMember ?? room.name, account, regState);
        if (!target.ok) {
          toast({ type: "error", title: `${mode === "video" ? "Video" : "Call"} unavailable`, description: target.reason });
          return;
        }
        if (mode === "video") {
          await ipcMakeVideoCall(target.uri);
        } else {
          await ipcMakeCall(target.uri);
        }
        return;
      }

      if (!connected || !baseUrl || !token || !isServerRoom) {
        toast({ type: "error", title: "Server connection required" });
        return;
      }
      const target = await paleServerStartRoomCall(baseUrl, token, room.room_id, mode);
      upsertRoom({
        ...room,
        call_uri: target.call_uri,
        conference_id: target.conference_id,
      });
      const callTarget = preflightSipCall(target.call_uri, account, regState);
      if (!callTarget.ok) {
        toast({ type: "error", title: `${mode === "video" ? "Video" : "Call"} unavailable`, description: callTarget.reason });
        return;
      }
      if (mode === "video") {
        await ipcMakeVideoCall(callTarget.uri);
      } else {
        await ipcMakeCall(callTarget.uri);
      }
    } catch (err) {
      toast({ type: "error", title: `Failed to start ${mode} call`, description: String(err) });
    }
  };

  const joinActiveRoomCall = async () => {
    if (!room.call_uri) return;
    const target = preflightSipCall(room.call_uri, account, regState);
    if (!target.ok) {
      toast({ type: "error", title: "Call unavailable", description: target.reason });
      return;
    }
    try {
      await ipcMakeCall(target.uri);
    } catch (err) {
      toast({ type: "error", title: "Failed to join call", description: String(err) });
    }
  };

  const endActiveRoomCall = async () => {
    if (!connected || !baseUrl || !token || !isServerRoom) return;
    try {
      await paleServerEndRoomCall(baseUrl, token, room.room_id);
      upsertRoom({
        ...room,
        call_uri: null,
        conference_id: null,
      });
    } catch (err) {
      toast({ type: "error", title: "Failed to end call", description: String(err) });
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
        {isServerRoom && (
          <select
            value={notificationLevel}
            onChange={(e) => handleNotificationLevelChange(e.target.value as typeof notificationLevel)}
            className="hidden sm:block h-8 max-w-[9rem] rounded-md bg-surface border border-border-subtle px-2 text-xs text-secondary hover:text-primary focus:outline-none focus:border-border-focus"
            aria-label="Notification level"
            title="Notification level"
          >
            <option value="all">All notifications</option>
            <option value="mentions">Mentions only</option>
            <option value="muted">Muted</option>
          </select>
        )}
        {isServerRoom && (
          <button
            onClick={() => { loadRoomThreads(); setShowThreadList(true); setThreadPanelOpen(false); }}
            className="p-2 rounded-md text-tertiary hover:text-accent hover:bg-elevated transition-colors"
            title="View all threads"
            aria-label="View all threads"
          >
            <MessagesSquare size={17} />
          </button>
        )}
        {canManageConnectors && (
          <button
            onClick={() => setShowConnectors(true)}
            className="p-2 rounded-md text-tertiary hover:text-accent hover:bg-elevated transition-colors"
            title="Channel connectors"
            aria-label="Channel connectors"
          >
            <Plug size={17} />
          </button>
        )}
        <button
          onClick={() => startRoomCall("audio")}
          disabled={!canStartRoomCall}
          className={cn(
            "p-2 rounded-md transition-colors",
            canStartRoomCall
              ? "text-tertiary hover:text-success hover:bg-elevated"
              : "text-tertiary/50 cursor-not-allowed"
          )}
          title={room.is_direct ? "Start voice call" : "Start group voice call"}
          aria-label={room.is_direct ? "Start voice call" : "Start group voice call"}
        >
          <Phone size={17} />
        </button>
        <button
          onClick={() => startRoomCall("video")}
          disabled={!canStartRoomCall}
          className={cn(
            "p-2 rounded-md transition-colors",
            canStartRoomCall
              ? "text-tertiary hover:text-accent hover:bg-elevated"
              : "text-tertiary/50 cursor-not-allowed"
          )}
          title={room.is_direct ? "Start video call" : "Start group video call"}
          aria-label={room.is_direct ? "Start video call" : "Start group video call"}
        >
          <Video size={17} />
        </button>
      </div>

      {showConnectors && baseUrl && token && (
        <ConnectorDialog
          baseUrl={baseUrl}
          token={token}
          roomId={room.room_id}
          roomName={room.name}
          onClose={() => setShowConnectors(false)}
        />
      )}

      {/* Team sub-tabs (Wiki, Tasks) */}
      {isServerRoom && room.team_id && (
        <div className="flex items-center gap-0.5 px-4 py-1.5 border-b border-border-subtle bg-elevated/50">
          <button
            onClick={() => setSubTab("chat")}
            className={cn("px-2.5 py-1 rounded-md text-xs font-medium transition-colors", subTab === "chat" ? "bg-accent text-white" : "text-tertiary hover:text-primary hover:bg-elevated")}
          >
            <MessageSquare size={12} className="inline mr-1" />Chat
          </button>
          <button
            onClick={() => setSubTab("wiki")}
            className={cn("px-2.5 py-1 rounded-md text-xs font-medium transition-colors", subTab === "wiki" ? "bg-accent text-white" : "text-tertiary hover:text-primary hover:bg-elevated")}
          >
            <BookOpen size={12} className="inline mr-1" />Wiki
          </button>
          <button
            onClick={() => setSubTab("tasks")}
            className={cn("px-2.5 py-1 rounded-md text-xs font-medium transition-colors", subTab === "tasks" ? "bg-accent text-white" : "text-tertiary hover:text-primary hover:bg-elevated")}
          >
            <ListTodo size={12} className="inline mr-1" />Tasks
          </button>
          {channelTabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => { setSubTab("tab"); setActiveTabId(tab.id); }}
              className={cn("px-2.5 py-1 rounded-md text-xs font-medium transition-colors", subTab === "tab" && activeTabId === tab.id ? "bg-accent text-white" : "text-tertiary hover:text-primary hover:bg-elevated")}
            >
              <ExternalLink size={12} className="inline mr-1" />{tab.name}
            </button>
          ))}
        </div>
      )}

      {subTab === "wiki" && room.team_id && baseUrl && token && (
        <WikiPanel teamId={room.team_id} baseUrl={baseUrl} token={token} />
      )}

      {subTab === "tasks" && room.team_id && baseUrl && token && (
        <TasksPanel teamId={room.team_id} baseUrl={baseUrl} token={token} />
      )}

      {subTab === "tab" && activeTabId && (() => {
        const tab = channelTabs.find((t) => t.id === activeTabId);
        return tab ? (
          <div className="flex-1 overflow-hidden">
            <iframe
              src={tab.url}
              title={tab.name}
              className="w-full h-full border-0"
              sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
            />
          </div>
        ) : null;
      })()}

      {hasActiveRoomCall && subTab === "chat" && (
        <div className="flex items-center justify-between gap-3 px-4 py-2 border-b border-border-subtle bg-success/10 text-sm">
          <div className="flex items-center gap-2 min-w-0 text-success">
            <Phone size={14} />
            <span className="font-medium truncate">Group call in progress</span>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <button
              onClick={joinActiveRoomCall}
              className="h-8 px-3 rounded-md bg-success text-white hover:opacity-90 transition-opacity text-xs font-medium"
            >
              Join
            </button>
            <button
              onClick={endActiveRoomCall}
              className="h-8 px-3 rounded-md border border-success/30 text-success hover:bg-success/10 transition-colors text-xs font-medium"
            >
              End
            </button>
          </div>
        </div>
      )}

      {subTab === "chat" && <>
      {/* Messages */}
      <div
        ref={messagesContainerRef}
        className={cn(
          "flex-1 overflow-y-auto px-4 py-3",
          chatDensitySpacing(),
        )}
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
        {messages.map((msg) => {
          const thread = roomThreads.find((t) => t.root_message_id === msg.event_id);
          return (
            <MessageBubble
              key={msg.event_id}
              message={msg}
              onReply={handleReply}
              onEdit={handleEdit}
              onForward={handleForward}
              onPin={handlePin}
              onSave={handleSave}
              onTranslate={handleTranslate}
              onImmersiveReader={(m) => setImmersiveReaderMessage(m)}
              onOpenThread={openThread}
              threadReplyCount={thread?.reply_count}
              customEmojis={customEmojis}
            />
          );
        })}
        <div ref={messagesEndRef} />

        {/* Loop Components */}
        {loopComponents.length > 0 && (
          <div className="space-y-2 mt-3 mb-2">
            {loopComponents.map((loop) => (
              <LoopComponentRenderer
                key={loop.id}
                component={loop}
                baseUrl={baseUrl ?? ""}
                token={token ?? ""}
                onUpdate={(updated) => setLoopComponents((prev) => prev.map((c) => c.id === updated.id ? updated : c))}
              />
            ))}
          </div>
        )}
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
      {mentionQuery !== null && (mentionUsers.length > 0 || teamTags.filter(t => t.name.toLowerCase().includes((mentionQuery ?? "").toLowerCase())).length > 0) && (
        <div className="px-3 pb-1">
          <div className="bg-surface border border-border-subtle rounded-lg shadow-lg max-h-32 overflow-y-auto">
            {teamTags
              .filter((t) => t.name.toLowerCase().includes((mentionQuery ?? "").toLowerCase()))
              .map((tag) => (
                <button
                  key={`tag-${tag.id}`}
                  onClick={() => {
                    const before = input.slice(0, input.lastIndexOf("@"));
                    setInput(`${before}@${tag.name} `);
                    setMentionQuery(null);
                    inputRef.current?.focus();
                  }}
                  className="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-elevated transition-colors text-sm"
                >
                  <span className="w-5 h-5 rounded-full bg-warning/20 text-warning flex items-center justify-center text-[10px] font-bold">
                    #
                  </span>
                  <span className="text-primary">@{tag.name}</span>
                  <span className="text-tertiary text-xs ml-auto">{tag.members.length} members</span>
                </button>
              ))}
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

      {/* Loop component insert panel */}
      {showLoopInsert && isServerRoom && connected && baseUrl && token && (
        <div className="px-3 py-2 border-t border-border-subtle bg-elevated/50">
          <div className="flex items-center gap-2">
            <ListTodo size={14} className="text-accent shrink-0" />
            <span className="text-xs text-secondary">Insert live component:</span>
            <button
              onClick={async () => {
                try {
                  await paleServerApi(baseUrl, token, `/v1/rooms/${room.room_id}/loops`, {
                    method: "POST", body: { component_type: "checklist", data: { items: [{ text: "Item 1", checked: false }] } },
                  });
                  const loops = await paleServerApi<LoopComponentItem[]>(baseUrl, token, `/v1/rooms/${room.room_id}/loops`);
                  setLoopComponents(loops);
                  setShowLoopInsert(false);
                  toast({ type: "success", title: "Checklist created" });
                } catch { toast({ type: "error", title: "Failed to create checklist" }); }
              }}
              className="px-2 py-1 rounded text-xs bg-accent/10 text-accent hover:bg-accent/20"
            >
              Checklist
            </button>
            <button
              onClick={async () => {
                try {
                  await paleServerApi(baseUrl, token, `/v1/rooms/${room.room_id}/loops`, {
                    method: "POST", body: { component_type: "table", data: { columns: ["Column 1", "Column 2"], rows: [["", ""]] } },
                  });
                  const loops = await paleServerApi<LoopComponentItem[]>(baseUrl, token, `/v1/rooms/${room.room_id}/loops`);
                  setLoopComponents(loops);
                  setShowLoopInsert(false);
                  toast({ type: "success", title: "Table created" });
                } catch { toast({ type: "error", title: "Failed to create table" }); }
              }}
              className="px-2 py-1 rounded text-xs bg-accent/10 text-accent hover:bg-accent/20"
            >
              Table
            </button>
            <button
              onClick={async () => {
                try {
                  await paleServerApi(baseUrl, token, `/v1/rooms/${room.room_id}/loops`, {
                    method: "POST", body: { component_type: "paragraph", data: { text: "" } },
                  });
                  const loops = await paleServerApi<LoopComponentItem[]>(baseUrl, token, `/v1/rooms/${room.room_id}/loops`);
                  setLoopComponents(loops);
                  setShowLoopInsert(false);
                  toast({ type: "success", title: "Paragraph created" });
                } catch { toast({ type: "error", title: "Failed to create paragraph" }); }
              }}
              className="px-2 py-1 rounded text-xs bg-accent/10 text-accent hover:bg-accent/20"
            >
              Paragraph
            </button>
          </div>
        </div>
      )}

      {/* Slash command menu */}
      {slashMenuOpen && slashExtensions.filter((e) => e.command.toLowerCase().includes(slashQuery)).length > 0 && (
        <div className="px-3 pb-1">
          <div className="bg-surface border border-border-subtle rounded-lg shadow-lg max-h-40 overflow-y-auto">
            {slashExtensions
              .filter((e) => e.command.toLowerCase().includes(slashQuery))
              .map((ext) => (
                <button
                  key={ext.id}
                  onClick={() => invokeSlashCommand(ext.command)}
                  className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-elevated transition-colors text-sm"
                >
                  <span className="text-accent font-mono text-xs">/{ext.command}</span>
                  <span className="text-primary text-xs">{ext.name}</span>
                  <span className="text-tertiary text-[10px] ml-auto truncate max-w-[200px]">{ext.description}</span>
                </button>
              ))}
          </div>
        </div>
      )}

      {/* Schedule send panel */}
      {showScheduleSend && (
        <div className="px-3 py-2 border-t border-border-subtle bg-elevated/50">
          <div className="flex items-center gap-2">
            <Clock size={14} className="text-accent shrink-0" />
            <span className="text-xs text-secondary">Schedule send:</span>
            <input
              type="datetime-local"
              value={scheduleDate}
              onChange={(e) => setScheduleDate(e.target.value)}
              min={datetimeLocalValue(new Date())}
              className="flex-1 h-7 rounded-md bg-surface border border-border-subtle px-2 text-xs text-primary focus:outline-none focus:border-border-focus"
            />
            <button
              onClick={handleScheduleSend}
              disabled={!input.trim() || !scheduleDate}
              className={cn(
                "px-2 py-1 rounded-md text-xs font-medium transition-colors",
                input.trim() && scheduleDate
                  ? "bg-accent text-white hover:bg-accent-hover"
                  : "bg-elevated text-tertiary cursor-not-allowed"
              )}
            >
              Schedule
            </button>
            <button
              onClick={() => { setShowScheduleSend(false); setScheduleDate(""); }}
              className="p-1 text-tertiary hover:text-primary"
            >
              <X size={14} />
            </button>
          </div>
        </div>
      )}

      {/* GIF picker */}
      {showGifPicker && (
        <div className="px-3 py-2 border-t border-border-subtle bg-elevated/50">
          <div className="flex items-center gap-2 mb-2">
            <input
              type="text"
              value={gifQuery}
              onChange={(e) => setGifQuery(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleGifSearch(gifQuery); }}
              placeholder="Search GIFs..."
              className="flex-1 h-7 rounded-md bg-surface border border-border-subtle px-2 text-xs text-primary placeholder:text-tertiary focus:outline-none focus:border-border-focus"
              autoFocus
            />
            <button
              onClick={() => handleGifSearch(gifQuery)}
              disabled={!gifQuery.trim()}
              className="px-2 py-1 rounded-md text-xs font-medium bg-accent text-white hover:bg-accent-hover disabled:bg-elevated disabled:text-tertiary"
            >
              Search
            </button>
            <button onClick={() => { setShowGifPicker(false); setGifQuery(""); setGifResults([]); }} className="p-1 text-tertiary hover:text-primary">
              <X size={14} />
            </button>
          </div>
          {gifLoading && <div className="flex justify-center py-4"><Loader2 size={18} className="animate-spin text-tertiary" /></div>}
          {!gifLoading && gifResults.length > 0 && (
            <div className="grid grid-cols-3 gap-1 max-h-40 overflow-y-auto">
              {gifResults.map((gif, i) => (
                <button
                  key={i}
                  onClick={() => handleGifSelect(gif)}
                  className="rounded overflow-hidden hover:ring-2 hover:ring-accent transition-all"
                  title={gif.title}
                >
                  <img src={gif.preview} alt={gif.title} className="w-full h-20 object-cover" loading="lazy" />
                </button>
              ))}
            </div>
          )}
          {!gifLoading && gifResults.length === 0 && gifQuery.trim() && (
            <p className="text-xs text-tertiary text-center py-3">No GIFs found</p>
          )}
        </div>
      )}

      {/* Compose bar */}
      <div className="px-3 py-2 border-t border-border-subtle shrink-0 bg-base/95">
        <div className="flex items-center gap-1.5 pb-2">
          <button
            className="p-2 text-tertiary hover:text-secondary rounded-md hover:bg-elevated"
            aria-label="Attach file"
          >
            <Paperclip size={18} />
          </button>
          {isServerRoom && !editingMessage && (
            <button
              onClick={() => setShowLoopInsert(!showLoopInsert)}
              className={cn(
                "p-2 rounded-md transition-colors",
                showLoopInsert ? "text-accent bg-accent/10" : "text-tertiary hover:text-secondary hover:bg-elevated"
              )}
              aria-label="Insert loop component"
              title="Insert a live component (checklist, table, paragraph)"
            >
              <ListTodo size={18} />
            </button>
          )}
          {isServerRoom && !editingMessage && (
            <button
              onClick={() => { setShowGifPicker(!showGifPicker); setShowScheduleSend(false); }}
              className={cn(
                "p-2 rounded-md transition-colors",
                showGifPicker ? "text-accent bg-accent/10" : "text-tertiary hover:text-secondary hover:bg-elevated"
              )}
              aria-label="GIF picker"
              title="Send a GIF"
            >
              <ImageLucide size={18} />
            </button>
          )}
          <ComposeEmojiPicker onSelect={insertEmoji} customEmojis={customEmojis} />
          <div className="ml-auto flex items-center gap-1.5">
            <button onClick={() => insertMarkdown("bold")} className="p-2 rounded text-tertiary hover:text-primary hover:bg-elevated" title="Bold (Ctrl+B)">
              <Bold size={15} />
            </button>
            <button onClick={() => insertMarkdown("italic")} className="p-2 rounded text-tertiary hover:text-primary hover:bg-elevated" title="Italic (Ctrl+I)">
              <Italic size={15} />
            </button>
            <button onClick={() => insertMarkdown("code")} className="p-2 rounded text-tertiary hover:text-primary hover:bg-elevated" title="Code">
              <Code size={15} />
            </button>
            <button onClick={() => insertMarkdown("link")} className="p-2 rounded text-tertiary hover:text-primary hover:bg-elevated" title="Link">
              <Link size={15} />
            </button>
          </div>
        </div>
        <div className="flex items-end gap-2 min-w-0">
          <input
            ref={inputRef}
            type="text"
            value={input}
            onChange={(e) => handleInputChange(e.target.value)}
            onKeyDown={handleComposeKeyDown}
            placeholder={editingMessage ? "Edit your message..." : "Type a message..."}
            className={cn(
              "min-w-0 flex-1 h-11 bg-surface border border-border-default rounded-xl",
              "px-3 text-sm text-primary shadow-inner",
              "placeholder:text-secondary",
              "focus:outline-none focus:border-border-focus focus:ring-2 focus:ring-accent/20",
              editingMessage && "border-warning/60"
            )}
          />
          {!editingMessage && (
            <select
              value={messagePriority}
              onChange={(event) => setMessagePriority(event.target.value as typeof messagePriority)}
              className="hidden sm:block h-11 w-[5.75rem] rounded-xl bg-surface border border-border-default px-2 text-xs text-primary focus:outline-none focus:border-border-focus"
              aria-label="Message priority"
            >
              <option value="normal">Normal</option>
              <option value="high">High</option>
              <option value="urgent">Urgent</option>
            </select>
          )}
          {isServerRoom && !editingMessage && (
            <button
              onClick={() => { setShowScheduleSend(!showScheduleSend); setShowGifPicker(false); }}
              className={cn(
                "h-11 w-11 shrink-0 rounded-xl transition-colors flex items-center justify-center",
                showScheduleSend ? "text-accent bg-accent/10" : "text-tertiary hover:text-secondary hover:bg-elevated"
              )}
              aria-label="Schedule send"
              title="Schedule message for later"
            >
              <Clock size={18} />
            </button>
          )}
          <button
            onClick={handleSend}
            disabled={!input.trim()}
            className={cn(
              "h-11 w-11 shrink-0 rounded-xl transition-colors flex items-center justify-center",
              input.trim()
                ? editingMessage
                  ? "bg-warning text-white hover:bg-warning/80"
                  : "bg-accent text-white hover:bg-accent-hover"
                : "bg-elevated text-tertiary cursor-not-allowed"
            )}
            aria-label={editingMessage ? "Save edit" : "Send"}
          >
            {editingMessage ? <Check size={18} /> : <Send size={18} />}
          </button>
        </div>
      </div>
      </>}

      {/* Immersive Reader Modal */}
      {immersiveReaderMessage && (
        <ImmersiveReaderModal
          message={immersiveReaderMessage}
          onClose={() => setImmersiveReaderMessage(null)}
        />
      )}

      {/* Thread Panel (side panel overlay) */}
      {threadPanelOpen && (
        <div className="absolute right-0 top-0 bottom-0 w-80 bg-surface border-l border-border-subtle z-20 flex flex-col shadow-lg">
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-subtle shrink-0">
            <button
              onClick={() => { setThreadPanelOpen(false); setActiveThreadRootId(null); }}
              className="p-1 text-tertiary hover:text-primary rounded-md hover:bg-elevated"
            >
              <X size={16} />
            </button>
            <MessagesSquare size={16} className="text-accent" />
            <span className="text-sm font-semibold text-primary">Thread</span>
          </div>
          <div className="flex-1 overflow-y-auto px-3 py-2 space-y-2">
            {threadLoading && (
              <div className="flex justify-center py-8">
                <Loader2 size={18} className="animate-spin text-tertiary" />
              </div>
            )}
            {!threadLoading && threadMessages.map((msg) => (
              <div
                key={msg.event_id}
                className={cn(
                  "rounded-lg px-3 py-2 text-sm max-w-full",
                  msg.is_own
                    ? "bg-accent text-white ml-4"
                    : "bg-elevated text-primary mr-4"
                )}
              >
                {!msg.is_own && (
                  <p className="text-[10px] font-semibold text-accent mb-0.5">
                    {msg.sender_name ?? msg.sender.split(":")[0]?.replace("@", "")}
                  </p>
                )}
                <p className="whitespace-pre-wrap break-words">{msg.body}</p>
                <p className={cn("text-[9px] mt-1", msg.is_own ? "text-white/60" : "text-tertiary")}>
                  {new Date(msg.timestamp * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
                </p>
              </div>
            ))}
          </div>
          <div className="flex items-center gap-2 px-3 py-2 border-t border-border-subtle shrink-0">
            <input
              type="text"
              value={threadInput}
              onChange={(e) => setThreadInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleThreadReply(); } }}
              placeholder="Reply in thread..."
              className="flex-1 bg-surface border border-border-subtle rounded-lg px-3 py-2 text-sm text-primary placeholder:text-tertiary focus:outline-none focus:border-border-focus"
            />
            <button
              onClick={handleThreadReply}
              disabled={!threadInput.trim()}
              className={cn(
                "p-2 rounded-lg transition-colors",
                threadInput.trim()
                  ? "bg-accent text-white hover:bg-accent-hover"
                  : "text-tertiary cursor-not-allowed"
              )}
            >
              <Send size={16} />
            </button>
          </div>
        </div>
      )}

      {/* Thread List Modal */}
      {showThreadList && (
        <div className="absolute right-0 top-0 bottom-0 w-80 bg-surface border-l border-border-subtle z-20 flex flex-col shadow-lg">
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-subtle shrink-0">
            <button
              onClick={() => setShowThreadList(false)}
              className="p-1 text-tertiary hover:text-primary rounded-md hover:bg-elevated"
            >
              <X size={16} />
            </button>
            <MessagesSquare size={16} className="text-accent" />
            <span className="text-sm font-semibold text-primary">All Threads</span>
          </div>
          <div className="flex-1 overflow-y-auto">
            {roomThreads.length === 0 && (
              <p className="text-sm text-tertiary text-center py-8">No threads yet</p>
            )}
            {roomThreads.map((thread) => {
              const rootMsg = messages.find((m) => m.event_id === thread.root_message_id);
              return (
                <button
                  key={thread.id}
                  onClick={() => {
                    if (rootMsg) openThread(rootMsg);
                    setShowThreadList(false);
                  }}
                  className="w-full text-left px-3 py-2.5 border-b border-border-subtle hover:bg-elevated transition-colors"
                >
                  <p className="text-xs text-primary truncate">
                    {rootMsg?.body ?? "Message"}
                  </p>
                  <div className="flex items-center gap-2 mt-1">
                    <span className="text-[10px] text-accent font-medium">
                      {thread.reply_count} {thread.reply_count === 1 ? "reply" : "replies"}
                    </span>
                    <span className="text-[10px] text-tertiary">
                      Last activity {new Date(thread.last_reply_at).toLocaleDateString()}
                    </span>
                  </div>
                  <div className="flex items-center gap-1 mt-0.5">
                    {thread.participants.slice(0, 3).map((p, i) => (
                      <span key={i} className="text-[9px] text-secondary truncate max-w-[80px]">
                        {p.replace(/^sip:/, "").split("@")[0]}
                      </span>
                    ))}
                    {thread.participants.length > 3 && (
                      <span className="text-[9px] text-tertiary">
                        +{thread.participants.length - 3}
                      </span>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

function ImmersiveReaderModal({ message, onClose }: { message: ChatMessage; onClose: () => void }) {
  const [prefs, setPrefs] = useState(getImmersivePrefs);

  const update = (key: string, value: any) => {
    const next = { ...prefs, [key]: value };
    setPrefs(next);
    setImmersivePrefs(next);
  };

  const fontSizeMap: Record<ImmersiveFontSize, string> = {
    small: "text-base", medium: "text-lg", large: "text-2xl", xlarge: "text-4xl",
  };
  const widthMap: Record<ImmersiveColumnWidth, string> = {
    narrow: "max-w-sm", medium: "max-w-lg", wide: "max-w-3xl",
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div
        className={cn(
          "relative w-full mx-4 rounded-xl shadow-2xl overflow-auto max-h-[90vh] p-8",
          prefs.highContrast ? "bg-black text-white" : "bg-white text-zinc-900 dark:bg-zinc-900 dark:text-zinc-100",
          widthMap[prefs.columnWidth as ImmersiveColumnWidth] || "max-w-lg"
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Close */}
        <button onClick={onClose} className="absolute top-3 right-3 p-1 rounded hover:bg-zinc-200 dark:hover:bg-zinc-700" aria-label="Close">
          <X size={18} />
        </button>

        {/* Controls */}
        <div className="flex flex-wrap items-center gap-3 mb-6 pb-4 border-b border-zinc-200 dark:border-zinc-700">
          <div className="flex items-center gap-1.5">
            <span className="text-xs font-medium">Size:</span>
            {(["small", "medium", "large", "xlarge"] as ImmersiveFontSize[]).map((s) => (
              <button key={s} onClick={() => update("fontSize", s)}
                className={cn("px-2 py-0.5 rounded text-xs", prefs.fontSize === s ? "bg-accent text-white" : "bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700")}>
                {s === "xlarge" ? "XL" : s.charAt(0).toUpperCase() + s.slice(1)}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-xs font-medium">Spacing:</span>
            {[1.2, 1.6, 2.0, 2.5].map((s) => (
              <button key={s} onClick={() => update("lineSpacing", s)}
                className={cn("px-2 py-0.5 rounded text-xs", prefs.lineSpacing === s ? "bg-accent text-white" : "bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700")}>
                {s}x
              </button>
            ))}
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-xs font-medium">Width:</span>
            {(["narrow", "medium", "wide"] as ImmersiveColumnWidth[]).map((w) => (
              <button key={w} onClick={() => update("columnWidth", w)}
                className={cn("px-2 py-0.5 rounded text-xs", prefs.columnWidth === w ? "bg-accent text-white" : "bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700")}>
                {w.charAt(0).toUpperCase() + w.slice(1)}
              </button>
            ))}
          </div>
          <label className="flex items-center gap-1.5 text-xs">
            <input type="checkbox" checked={prefs.dyslexiaFont} onChange={(e) => update("dyslexiaFont", e.target.checked)} className="accent-accent" />
            Dyslexia-friendly font
          </label>
          <label className="flex items-center gap-1.5 text-xs">
            <input type="checkbox" checked={prefs.highContrast} onChange={(e) => update("highContrast", e.target.checked)} className="accent-accent" />
            High contrast
          </label>
        </div>

        {/* Content */}
        <div
          className={cn(fontSizeMap[prefs.fontSize as ImmersiveFontSize] || "text-lg")}
          style={{
            lineHeight: prefs.lineSpacing,
            fontFamily: prefs.dyslexiaFont ? "'OpenDyslexic', 'Comic Sans MS', sans-serif" : "inherit",
          }}
        >
          <p className="text-[10px] text-zinc-400 mb-2">
            {message.sender_name ?? message.sender} - {new Date(message.timestamp * 1000).toLocaleString()}
          </p>
          <div className="whitespace-pre-wrap">{message.body}</div>
        </div>
      </div>
    </div>
  );
}

function LoopComponentRenderer({ component, baseUrl, token, onUpdate }: {
  component: LoopComponentItem;
  baseUrl: string;
  token: string;
  onUpdate: (updated: LoopComponentItem) => void;
}) {
  const saveData = async (data: any) => {
    try {
      const updated = await paleServerApi<LoopComponentItem>(baseUrl, token, `/v1/loops/${component.id}`, {
        method: "PUT", body: { data },
      });
      onUpdate(updated);
    } catch {
      toast({ type: "error", title: "Failed to update component" });
    }
  };

  if (component.component_type === "checklist") {
    const items: { text: string; checked: boolean }[] = component.data?.items ?? [];
    return (
      <div className="p-3 rounded-lg border border-accent/20 bg-accent/5">
        <div className="flex items-center gap-1.5 mb-2">
          <ListTodo size={14} className="text-accent" />
          <span className="text-xs font-semibold text-accent">Live Checklist</span>
          <span className="text-[10px] text-tertiary ml-auto">by {component.created_by.replace("sip:", "")}</span>
        </div>
        {items.map((item, idx) => (
          <div key={idx} className="flex items-center gap-2 py-0.5">
            <input
              type="checkbox"
              checked={item.checked}
              onChange={() => {
                const newItems = [...items];
                newItems[idx] = { ...item, checked: !item.checked };
                saveData({ items: newItems });
              }}
              className="accent-accent"
            />
            <input
              type="text"
              value={item.text}
              onChange={(e) => {
                const newItems = [...items];
                newItems[idx] = { ...item, text: e.target.value };
                saveData({ items: newItems });
              }}
              className="flex-1 text-sm bg-transparent border-none outline-none text-primary"
            />
          </div>
        ))}
        <button
          onClick={() => saveData({ items: [...items, { text: "", checked: false }] })}
          className="mt-1 text-[10px] text-accent hover:underline"
        >
          + Add item
        </button>
      </div>
    );
  }

  if (component.component_type === "table") {
    const columns: string[] = component.data?.columns ?? [];
    const rows: string[][] = component.data?.rows ?? [];
    return (
      <div className="p-3 rounded-lg border border-accent/20 bg-accent/5 overflow-x-auto">
        <div className="flex items-center gap-1.5 mb-2">
          <ExternalLink size={14} className="text-accent" />
          <span className="text-xs font-semibold text-accent">Live Table</span>
          <span className="text-[10px] text-tertiary ml-auto">by {component.created_by.replace("sip:", "")}</span>
        </div>
        <table className="w-full text-xs">
          <thead>
            <tr>
              {columns.map((col, ci) => (
                <th key={ci} className="text-left px-2 py-1 border-b border-accent/20 text-secondary font-medium">
                  <input
                    type="text" value={col}
                    onChange={(e) => {
                      const newCols = [...columns]; newCols[ci] = e.target.value;
                      saveData({ columns: newCols, rows });
                    }}
                    className="w-full bg-transparent border-none outline-none text-secondary font-medium"
                  />
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, ri) => (
              <tr key={ri}>
                {row.map((cell, ci) => (
                  <td key={ci} className="px-2 py-1 border-b border-accent/10">
                    <input
                      type="text" value={cell}
                      onChange={(e) => {
                        const newRows = rows.map((r) => [...r]);
                        newRows[ri][ci] = e.target.value;
                        saveData({ columns, rows: newRows });
                      }}
                      className="w-full bg-transparent border-none outline-none text-primary text-xs"
                    />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        <button
          onClick={() => saveData({ columns, rows: [...rows, columns.map(() => "")] })}
          className="mt-1 text-[10px] text-accent hover:underline"
        >
          + Add row
        </button>
      </div>
    );
  }

  // paragraph
  const text = component.data?.text ?? "";
  return (
    <div className="p-3 rounded-lg border border-accent/20 bg-accent/5">
      <div className="flex items-center gap-1.5 mb-2">
        <FileIcon size={14} className="text-accent" />
        <span className="text-xs font-semibold text-accent">Live Paragraph</span>
        <span className="text-[10px] text-tertiary ml-auto">by {component.created_by.replace("sip:", "")}</span>
      </div>
      <textarea
        value={text}
        onChange={(e) => saveData({ text: e.target.value })}
        className="w-full text-sm bg-transparent border-none outline-none text-primary resize-none min-h-[60px]"
        placeholder="Start typing..."
      />
    </div>
  );
}

function WikiPanel({ teamId, baseUrl, token }: { teamId: string; baseUrl: string; token: string }) {
  const [pages, setPages] = useState<WikiPage[]>([]);
  const [selectedPage, setSelectedPage] = useState<WikiPage | null>(null);
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [editBody, setEditBody] = useState("");
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    paleServerGetWikiPages(baseUrl, token, teamId).then(setPages).catch(() => {});
  }, [teamId, baseUrl, token]);

  const handleCreate = async () => {
    if (!newTitle.trim()) return;
    try {
      const page = await paleServerCreateWikiPage(baseUrl, token, teamId, newTitle.trim());
      setPages((prev) => [...prev, page]);
      setNewTitle("");
      setCreating(false);
      setSelectedPage(page);
    } catch (err) {
      toast({ type: "error", title: "Failed to create wiki page", description: String(err) });
    }
  };

  const handleSave = async () => {
    if (!selectedPage) return;
    try {
      const updated = await paleServerUpdateWikiPage(baseUrl, token, selectedPage.id, { body: editBody });
      setSelectedPage(updated);
      setPages((prev) => prev.map((p) => (p.id === updated.id ? updated : p)));
      setEditing(false);
    } catch (err) {
      toast({ type: "error", title: "Failed to save wiki page", description: String(err) });
    }
  };

  const handleDelete = async (pageId: string) => {
    try {
      await paleServerDeleteWikiPage(baseUrl, token, pageId);
      setPages((prev) => prev.filter((p) => p.id !== pageId));
      if (selectedPage?.id === pageId) setSelectedPage(null);
    } catch (err) {
      toast({ type: "error", title: "Failed to delete wiki page", description: String(err) });
    }
  };

  if (selectedPage) {
    return (
      <div className="flex-1 overflow-y-auto p-4">
        <button onClick={() => { setSelectedPage(null); setEditing(false); }} className="text-xs text-accent hover:underline mb-3">&larr; All pages</button>
        <h2 className="text-lg font-semibold text-primary mb-2">{selectedPage.title}</h2>
        <p className="text-[10px] text-tertiary mb-3">Updated by {selectedPage.updated_by} on {new Date(selectedPage.updated_at).toLocaleDateString()}</p>
        {editing ? (
          <div className="space-y-2">
            <textarea
              value={editBody}
              onChange={(e) => setEditBody(e.target.value)}
              className="w-full h-64 bg-surface border border-border-subtle rounded-lg px-3 py-2 text-sm text-primary focus:outline-none focus:border-border-focus resize-y"
              placeholder="Write your wiki content in markdown..."
            />
            <div className="flex gap-2">
              <button onClick={handleSave} className="px-3 py-1.5 rounded-md bg-accent text-white text-xs font-medium hover:bg-accent-hover">Save</button>
              <button onClick={() => setEditing(false)} className="px-3 py-1.5 rounded-md border border-border-subtle text-xs text-secondary hover:text-primary">Cancel</button>
            </div>
          </div>
        ) : (
          <div>
            <div className="text-sm text-primary whitespace-pre-wrap break-words" dangerouslySetInnerHTML={{ __html: renderMarkdown(selectedPage.body || "No content yet.") }} />
            <button onClick={() => { setEditing(true); setEditBody(selectedPage.body); }} className="mt-3 px-3 py-1.5 rounded-md border border-border-subtle text-xs text-secondary hover:text-primary">
              <Pencil size={12} className="inline mr-1" />Edit
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-primary">Wiki Pages</h3>
        <button onClick={() => setCreating(!creating)} className="p-1 rounded-md text-tertiary hover:text-accent hover:bg-elevated">
          <Plus size={16} />
        </button>
      </div>
      {creating && (
        <div className="flex gap-2 mb-3">
          <input
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            placeholder="Page title..."
            className="flex-1 bg-surface border border-border-subtle rounded-md px-2 py-1.5 text-sm text-primary focus:outline-none focus:border-border-focus"
            autoFocus
          />
          <button onClick={handleCreate} className="px-2.5 py-1.5 rounded-md bg-accent text-white text-xs font-medium hover:bg-accent-hover">Create</button>
        </div>
      )}
      {pages.length === 0 ? (
        <p className="text-sm text-tertiary text-center py-8">No wiki pages yet. Create one to get started.</p>
      ) : (
        <div className="space-y-1">
          {pages.map((page) => (
            <div key={page.id} className="flex items-center justify-between group px-2 py-2 rounded-md hover:bg-elevated cursor-pointer" onClick={() => setSelectedPage(page)}>
              <div className="min-w-0">
                <p className="text-sm font-medium text-primary truncate">{page.title}</p>
                <p className="text-[10px] text-tertiary">Updated {new Date(page.updated_at).toLocaleDateString()}</p>
              </div>
              <button
                onClick={(e) => { e.stopPropagation(); handleDelete(page.id); }}
                className="opacity-0 group-hover:opacity-100 p-1 text-tertiary hover:text-destructive"
                title="Delete"
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function TasksPanel({ teamId, baseUrl, token }: { teamId: string; baseUrl: string; token: string }) {
  const [boards, setBoards] = useState<TaskBoard[]>([]);
  const [selectedBoard, setSelectedBoard] = useState<TaskBoard | null>(null);
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [creatingBoard, setCreatingBoard] = useState(false);
  const [newBoardName, setNewBoardName] = useState("");
  const [creatingTask, setCreatingTask] = useState(false);
  const [newTaskTitle, setNewTaskTitle] = useState("");

  useEffect(() => {
    paleServerGetTaskBoards(baseUrl, token, teamId).then(setBoards).catch(() => {});
  }, [teamId, baseUrl, token]);

  useEffect(() => {
    if (!selectedBoard) { setTasks([]); return; }
    paleServerGetTasks(baseUrl, token, selectedBoard.id).then(setTasks).catch(() => {});
  }, [selectedBoard, baseUrl, token]);

  const handleCreateBoard = async () => {
    if (!newBoardName.trim()) return;
    try {
      const board = await paleServerCreateTaskBoard(baseUrl, token, teamId, newBoardName.trim());
      setBoards((prev) => [...prev, board]);
      setNewBoardName("");
      setCreatingBoard(false);
      setSelectedBoard(board);
    } catch (err) {
      toast({ type: "error", title: "Failed to create board", description: String(err) });
    }
  };

  const handleCreateTask = async () => {
    if (!newTaskTitle.trim() || !selectedBoard) return;
    try {
      const task = await paleServerCreateTask(baseUrl, token, selectedBoard.id, { title: newTaskTitle.trim() });
      setTasks((prev) => [...prev, task]);
      setNewTaskTitle("");
      setCreatingTask(false);
    } catch (err) {
      toast({ type: "error", title: "Failed to create task", description: String(err) });
    }
  };

  const handleUpdateTaskStatus = async (task: TaskItem, newStatus: string) => {
    try {
      const updated = await paleServerUpdateTask(baseUrl, token, task.id, { status: newStatus });
      setTasks((prev) => prev.map((t) => (t.id === updated.id ? updated : t)));
    } catch (err) {
      toast({ type: "error", title: "Failed to update task", description: String(err) });
    }
  };

  const columns = [
    { key: "todo", label: "To Do", color: "border-t-tertiary" },
    { key: "in-progress", label: "In Progress", color: "border-t-warning" },
    { key: "done", label: "Done", color: "border-t-success" },
  ];

  if (selectedBoard) {
    return (
      <div className="flex-1 overflow-y-auto p-4">
        <button onClick={() => setSelectedBoard(null)} className="text-xs text-accent hover:underline mb-3">&larr; All boards</button>
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-semibold text-primary">{selectedBoard.name}</h3>
          <button onClick={() => setCreatingTask(!creatingTask)} className="p-1 rounded-md text-tertiary hover:text-accent hover:bg-elevated" title="Add task">
            <Plus size={16} />
          </button>
        </div>
        {creatingTask && (
          <div className="flex gap-2 mb-3">
            <input
              value={newTaskTitle}
              onChange={(e) => setNewTaskTitle(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleCreateTask()}
              placeholder="Task title..."
              className="flex-1 bg-surface border border-border-subtle rounded-md px-2 py-1.5 text-sm text-primary focus:outline-none focus:border-border-focus"
              autoFocus
            />
            <button onClick={handleCreateTask} className="px-2.5 py-1.5 rounded-md bg-accent text-white text-xs font-medium hover:bg-accent-hover">Add</button>
          </div>
        )}
        <div className="grid grid-cols-3 gap-3">
          {columns.map((col) => {
            const colTasks = tasks.filter((t) => t.status === col.key);
            return (
              <div key={col.key} className={cn("border-t-2 rounded-t-none rounded-b-lg bg-elevated/50 p-2 min-h-[120px]", col.color)}>
                <p className="text-[10px] font-semibold text-tertiary uppercase tracking-wider mb-2">{col.label} ({colTasks.length})</p>
                <div className="space-y-1.5">
                  {colTasks.map((task) => (
                    <div key={task.id} className="bg-surface border border-border-subtle rounded-md p-2">
                      <p className="text-xs font-medium text-primary mb-1">{task.title}</p>
                      {task.assignee && <p className="text-[10px] text-tertiary">{task.assignee}</p>}
                      {task.due_date && <p className="text-[10px] text-tertiary">{new Date(task.due_date).toLocaleDateString()}</p>}
                      <div className="flex gap-1 mt-1.5">
                        {columns.filter((c) => c.key !== task.status).map((c) => (
                          <button
                            key={c.key}
                            onClick={() => handleUpdateTaskStatus(task, c.key)}
                            className="text-[9px] px-1.5 py-0.5 rounded border border-border-subtle text-tertiary hover:text-primary hover:bg-elevated"
                          >
                            {c.label}
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-primary">Task Boards</h3>
        <button onClick={() => setCreatingBoard(!creatingBoard)} className="p-1 rounded-md text-tertiary hover:text-accent hover:bg-elevated">
          <Plus size={16} />
        </button>
      </div>
      {creatingBoard && (
        <div className="flex gap-2 mb-3">
          <input
            value={newBoardName}
            onChange={(e) => setNewBoardName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreateBoard()}
            placeholder="Board name..."
            className="flex-1 bg-surface border border-border-subtle rounded-md px-2 py-1.5 text-sm text-primary focus:outline-none focus:border-border-focus"
            autoFocus
          />
          <button onClick={handleCreateBoard} className="px-2.5 py-1.5 rounded-md bg-accent text-white text-xs font-medium hover:bg-accent-hover">Create</button>
        </div>
      )}
      {boards.length === 0 ? (
        <p className="text-sm text-tertiary text-center py-8">No task boards yet. Create one to get started.</p>
      ) : (
        <div className="space-y-1">
          {boards.map((board) => (
            <div key={board.id} className="flex items-center justify-between group px-2 py-2 rounded-md hover:bg-elevated cursor-pointer" onClick={() => setSelectedBoard(board)}>
              <div className="min-w-0">
                <p className="text-sm font-medium text-primary truncate">{board.name}</p>
                <p className="text-[10px] text-tertiary">Created {new Date(board.created_at).toLocaleDateString()}</p>
              </div>
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
  onCreateMeeting,
}: {
  onSubmit: (user: { display_name: string; sip_uri: string; matrix_user_id?: string | null }) => void;
  onCreateRoom: (name: string, members: string[]) => void;
  onCreateMeeting: (input: {
    title: string;
    starts_at: string;
    ends_at: string;
    participants: string[];
    mode: "audio" | "video";
  }) => void;
}) {
  const [mode, setMode] = useState<"dm" | "room" | "meeting">("dm");
  const [userId, setUserId] = useState("");
  const [roomName, setRoomName] = useState("");
  const now = new Date();
  const defaultStartsAt = datetimeLocalValue(new Date(now.getTime() + 30 * 60_000));
  const defaultEndsAt = datetimeLocalValue(new Date(now.getTime() + 60 * 60_000));
  const [meetingTitle, setMeetingTitle] = useState("");
  const [meetingStartsAt, setMeetingStartsAt] = useState(defaultStartsAt);
  const [meetingEndsAt, setMeetingEndsAt] = useState(defaultEndsAt);
  const [meetingMode, setMeetingMode] = useState<"audio" | "video">("video");
  const [memberQuery, setMemberQuery] = useState("");
  const [selectedMembers, setSelectedMembers] = useState<ServerUser[]>([]);
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

  const memberSuggestions = memberQuery.trim()
    ? serverUsers
        .filter((u) =>
          !selectedMembers.some((member) => member.id === u.id) &&
          (u.display_name.toLowerCase().includes(memberQuery.toLowerCase()) ||
            u.sip_uri.toLowerCase().includes(memberQuery.toLowerCase()))
        )
        .slice(0, 6)
    : [];

  const addRoomMember = (user: ServerUser) => {
    setSelectedMembers((members) =>
      members.some((member) => member.id === user.id) ? members : [...members, user]
    );
    setMemberQuery("");
  };

  const removeRoomMember = (userIdToRemove: string) => {
    setSelectedMembers((members) => members.filter((member) => member.id !== userIdToRemove));
  };

  const createRoom = () => {
    if (!roomName.trim()) return;
    onCreateRoom(roomName.trim(), selectedMembers.map((member) => member.sip_uri));
  };

  const createMeeting = () => {
    if (!meetingTitle.trim() || !meetingStartsAt || !meetingEndsAt) return;
    onCreateMeeting({
      title: meetingTitle.trim(),
      starts_at: new Date(meetingStartsAt).toISOString(),
      ends_at: new Date(meetingEndsAt).toISOString(),
      participants: selectedMembers.map((member) => member.sip_uri),
      mode: meetingMode,
    });
  };

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
        <button
          onClick={() => setMode("meeting")}
          className={cn(
            "px-2 py-1 text-xs rounded-md",
            mode === "meeting" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary"
          )}
        >
          Meeting
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
      ) : mode === "room" ? (
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
          {selectedMembers.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {selectedMembers.map((member) => (
                <span
                  key={member.id}
                  className="inline-flex items-center gap-1 rounded-md bg-accent-muted px-2 py-1 text-xs text-accent"
                >
                  {member.display_name}
                  <button
                    onClick={() => removeRoomMember(member.id)}
                    className="rounded text-accent hover:text-primary"
                    title={`Remove ${member.display_name}`}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
            </div>
          )}
          <div className="relative">
            <input
              type="text"
              value={memberQuery}
              onChange={(e) => setMemberQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && memberSuggestions[0]) {
                  e.preventDefault();
                  addRoomMember(memberSuggestions[0]);
                }
              }}
              placeholder="Search users to add..."
              className={cn(
                "w-full bg-surface border border-border-subtle rounded-lg",
                "px-3 py-2 text-sm text-primary placeholder:text-tertiary",
                "focus:outline-none focus:border-border-focus"
              )}
            />
            {memberSuggestions.length > 0 && (
              <div className="absolute z-10 left-0 right-0 mt-1 bg-surface border border-border-subtle rounded-lg shadow-lg max-h-44 overflow-y-auto">
                {memberSuggestions.map((u) => (
                  <button
                    key={u.id}
                    onClick={() => addRoomMember(u)}
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
          <button
            onClick={createRoom}
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
      ) : (
        <>
          <input
            type="text"
            value={meetingTitle}
            onChange={(e) => setMeetingTitle(e.target.value)}
            placeholder="Meeting title"
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-lg",
              "px-3 py-2 text-sm text-primary placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus"
            )}
            autoFocus
          />
          <div className="grid grid-cols-2 gap-2">
            <input
              type="datetime-local"
              value={meetingStartsAt}
              onChange={(e) => setMeetingStartsAt(e.target.value)}
              className="bg-surface border border-border-subtle rounded-lg px-2 py-2 text-xs text-primary focus:outline-none focus:border-border-focus"
            />
            <input
              type="datetime-local"
              value={meetingEndsAt}
              onChange={(e) => setMeetingEndsAt(e.target.value)}
              className="bg-surface border border-border-subtle rounded-lg px-2 py-2 text-xs text-primary focus:outline-none focus:border-border-focus"
            />
          </div>
          <div className="flex gap-1">
            <button
              onClick={() => setMeetingMode("video")}
              className={cn(
                "flex-1 px-2 py-1.5 text-xs rounded-md",
                meetingMode === "video" ? "bg-accent-muted text-accent" : "bg-surface text-tertiary hover:text-secondary"
              )}
            >
              Video
            </button>
            <button
              onClick={() => setMeetingMode("audio")}
              className={cn(
                "flex-1 px-2 py-1.5 text-xs rounded-md",
                meetingMode === "audio" ? "bg-accent-muted text-accent" : "bg-surface text-tertiary hover:text-secondary"
              )}
            >
              Audio
            </button>
          </div>
          {selectedMembers.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {selectedMembers.map((member) => (
                <span
                  key={member.id}
                  className="inline-flex items-center gap-1 rounded-md bg-accent-muted px-2 py-1 text-xs text-accent"
                >
                  {member.display_name}
                  <button
                    onClick={() => removeRoomMember(member.id)}
                    className="rounded text-accent hover:text-primary"
                    title={`Remove ${member.display_name}`}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
            </div>
          )}
          <div className="relative">
            <input
              type="text"
              value={memberQuery}
              onChange={(e) => setMemberQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && memberSuggestions[0]) {
                  e.preventDefault();
                  addRoomMember(memberSuggestions[0]);
                }
              }}
              placeholder="Search users to invite..."
              className={cn(
                "w-full bg-surface border border-border-subtle rounded-lg",
                "px-3 py-2 text-sm text-primary placeholder:text-tertiary",
                "focus:outline-none focus:border-border-focus"
              )}
            />
            {memberSuggestions.length > 0 && (
              <div className="absolute z-10 left-0 right-0 mt-1 bg-surface border border-border-subtle rounded-lg shadow-lg max-h-44 overflow-y-auto">
                {memberSuggestions.map((u) => (
                  <button
                    key={u.id}
                    onClick={() => addRoomMember(u)}
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
          <button
            onClick={createMeeting}
            disabled={!meetingTitle.trim() || !meetingStartsAt || !meetingEndsAt}
            className={cn(
              "w-full px-3 py-2 rounded-lg text-sm font-medium transition-colors",
              meetingTitle.trim() && meetingStartsAt && meetingEndsAt
                ? "bg-accent text-white hover:bg-accent-hover"
                : "bg-elevated text-tertiary cursor-not-allowed"
            )}
          >
            Schedule Meeting
          </button>
          <p className="text-[10px] text-tertiary">Create a scheduled meeting with selected participants</p>
        </>
      )}
    </div>
  );
}

function EmojiPickerButton({ onSelect, customEmojis }: { onSelect: (emoji: string) => void; customEmojis?: CustomEmoji[] }) {
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
        <div className="absolute bottom-6 right-0 z-50 bg-surface border border-border-subtle rounded-lg shadow-lg p-2 w-56 max-h-64 overflow-y-auto">
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
          {customEmojis && customEmojis.length > 0 && (
            <div className="mb-1.5">
              <p className="text-[9px] font-semibold text-tertiary uppercase tracking-wider mb-0.5 px-0.5">Custom</p>
              <div className="flex flex-wrap gap-0.5">
                {customEmojis.map((emoji) => (
                  <button
                    key={emoji.id}
                    onClick={() => { onSelect(`:${emoji.shortcode}:`); setOpen(false); }}
                    className="w-6 h-6 flex items-center justify-center rounded hover:bg-elevated"
                    title={`:${emoji.shortcode}:`}
                  >
                    <img src={emoji.image_url} alt={emoji.shortcode} className="w-5 h-5 object-contain" />
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ComposeEmojiPicker({ onSelect, customEmojis }: { onSelect: (emoji: string) => void; customEmojis?: CustomEmoji[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        className={cn(
          "p-2 rounded-md transition-colors",
          open ? "text-accent bg-accent/10" : "text-tertiary hover:text-secondary hover:bg-elevated"
        )}
        aria-label="Emoji picker"
        title="Insert emoji"
      >
        <Smile size={18} />
      </button>
      {open && (
        <div className="absolute bottom-10 left-0 z-50 bg-surface border border-border-subtle rounded-lg shadow-lg p-2.5 w-72 max-h-72 overflow-y-auto">
          {EMOJI_CATEGORIES.map((cat) => (
            <div key={cat.label} className="mb-2">
              <p className="text-[10px] font-semibold text-tertiary uppercase tracking-wider mb-1 px-0.5">{cat.label}</p>
              <div className="flex flex-wrap gap-0.5">
                {cat.emojis.map((emoji) => (
                  <button
                    key={emoji}
                    onClick={() => { onSelect(emoji); setOpen(false); }}
                    className="w-8 h-8 flex items-center justify-center rounded-md hover:bg-elevated text-lg transition-colors"
                  >
                    {emoji}
                  </button>
                ))}
              </div>
            </div>
          ))}
          {customEmojis && customEmojis.length > 0 && (
            <div className="mb-2">
              <p className="text-[10px] font-semibold text-tertiary uppercase tracking-wider mb-1 px-0.5">Custom</p>
              <div className="flex flex-wrap gap-0.5">
                {customEmojis.map((emoji) => (
                  <button
                    key={emoji.id}
                    onClick={() => { onSelect(`:${emoji.shortcode}:`); setOpen(false); }}
                    className="w-8 h-8 flex items-center justify-center rounded-md hover:bg-elevated transition-colors"
                    title={`:${emoji.shortcode}:`}
                  >
                    <img src={emoji.image_url} alt={emoji.shortcode} className="w-5 h-5 object-contain" />
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function TypingIndicator({ users }: { users: string[] }) {
  const names = users.map((id) => id.replace(/^sips?:/i, "").split("@")[0] || id);
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

const TRANSLATE_LANGUAGES = [
  { code: "en", label: "English" },
  { code: "es", label: "Spanish" },
  { code: "fr", label: "French" },
  { code: "de", label: "German" },
  { code: "pt", label: "Portuguese" },
  { code: "zh", label: "Chinese" },
  { code: "ja", label: "Japanese" },
  { code: "ko", label: "Korean" },
  { code: "ar", label: "Arabic" },
  { code: "ru", label: "Russian" },
];

function TranslateMenu({ message, onTranslate }: { message: ChatMessage; onTranslate: (msg: ChatMessage, lang: string) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="p-0.5 rounded text-tertiary hover:text-accent"
        title="Translate"
      >
        <Languages size={12} />
      </button>
      {open && (
        <div className="absolute bottom-6 right-0 z-50 bg-surface border border-border-subtle rounded-lg shadow-lg p-1 w-32">
          {TRANSLATE_LANGUAGES.map((lang) => (
            <button
              key={lang.code}
              onClick={() => { onTranslate(message, lang.code); setOpen(false); }}
              className="w-full text-left px-2 py-1 text-xs rounded hover:bg-elevated text-primary"
            >
              {lang.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function MessageBubble({
  message,
  onReply,
  onEdit,
  onForward,
  onPin,
  onSave,
  onTranslate,
  onImmersiveReader,
  onOpenThread,
  threadReplyCount,
  customEmojis,
}: {
  message: ChatMessage;
  onReply?: (msg: ChatMessage) => void;
  onEdit?: (msg: ChatMessage) => void;
  onForward?: (msg: ChatMessage) => void;
  onPin?: (msg: ChatMessage) => void;
  onSave?: (msg: ChatMessage) => void;
  onTranslate?: (msg: ChatMessage, targetLang: string) => void;
  onImmersiveReader?: (msg: ChatMessage) => void;
  onOpenThread?: (msg: ChatMessage) => void;
  threadReplyCount?: number;
  customEmojis?: CustomEmoji[];
}) {
  const time = new Date(message.timestamp * 1000).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
  const kind = msgTypeLabel(message.msg_type);
  const { baseUrl, token, connected } = useServerStore();
  const updateMessage = useChatStore((s) => s.updateMessage);
  const currentSipUri = useAccountStore((s) => s.account?.sipUri);
  const currentUserUri = currentSipUri
    ? currentSipUri.startsWith("sip:")
      ? currentSipUri
      : `sip:${currentSipUri}`
    : "";
  const saved = currentUserUri ? (message.saved_by ?? []).includes(currentUserUri) : false;
  const readCount = (message.read_by ?? []).filter((uri) => uri !== currentUserUri).length;

  const handleDelete = async () => {
    if (!connected || !baseUrl || !token) return;
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
      const uri = currentUserUri || "me";
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
          {/* Thread */}
          <button
            onClick={() => onOpenThread?.(message)}
            className="p-0.5 rounded text-tertiary hover:text-accent"
            title="Reply in thread"
          >
            <MessagesSquare size={12} />
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
          <button
            onClick={() => onSave?.(message)}
            className={cn("p-0.5 rounded", saved ? "text-warning" : "text-tertiary hover:text-warning")}
            title={saved ? "Unsave" : "Save"}
          >
            <Star size={12} />
          </button>
          {/* Forward */}
          <button
            onClick={() => onForward?.(message)}
            className="p-0.5 rounded text-tertiary hover:text-accent"
            title="Forward"
          >
            <Forward size={12} />
          </button>
          {/* Immersive Reader */}
          <button
            onClick={() => onImmersiveReader?.(message)}
            className="p-0.5 rounded text-tertiary hover:text-accent"
            title="Immersive Reader"
          >
            <BookOpen size={12} />
          </button>
          {/* Translate */}
          {!message.is_own && onTranslate && (
            <TranslateMenu message={message} onTranslate={onTranslate} />
          )}
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
            <EmojiPickerButton onSelect={handleReaction} customEmojis={customEmojis} />
          )}
        </div>
      )}
      <div className="max-w-[80%]">
        <div
          className={cn(
            "rounded-2xl relative",
            chatDensityBubble(),
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
          {saved && (
            <div className="flex items-center gap-1 text-[9px] text-warning mb-1">
              <Star size={9} /> Saved
            </div>
          )}
          {message.priority && message.priority !== "normal" && (
            <div className={cn(
              "flex items-center gap-1 text-[9px] mb-1",
              message.priority === "urgent" ? "text-destructive" : "text-warning"
            )}>
              <AlertTriangle size={9} /> {message.priority === "urgent" ? "Urgent" : "High priority"}
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

          {/* Adaptive card rendering */}
          {message.card_payload && (
            <div className={cn(
              "mt-2 rounded-lg border p-3",
              message.is_own ? "border-white/20 bg-white/10" : "border-border-subtle bg-elevated"
            )}>
              {message.card_payload.image_url && (
                <img src={message.card_payload.image_url} alt="" className="rounded max-w-full max-h-40 object-contain mb-2" />
              )}
              {message.card_payload.title && (
                <p className="font-semibold text-sm mb-1">{message.card_payload.title}</p>
              )}
              {message.card_payload.body && (
                <p className="text-xs opacity-80 mb-2">{message.card_payload.body}</p>
              )}
              {message.card_payload.actions.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {message.card_payload.actions.map((action, idx) => (
                    <button
                      key={idx}
                      onClick={() => {
                        if (action.url) window.open(action.url, "_blank", "noopener,noreferrer");
                      }}
                      className={cn(
                        "text-xs px-2.5 py-1 rounded-md font-medium transition-colors",
                        action.url
                          ? "bg-accent text-white hover:bg-accent-hover cursor-pointer"
                          : "bg-elevated border border-border-subtle text-primary"
                      )}
                    >
                      {action.url && <ExternalLink size={10} className="inline mr-1" />}
                      {action.title}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Translation */}
          {message.translated_text && (
            <div className={cn(
              "mt-1.5 pt-1.5 border-t text-xs italic",
              message.is_own ? "border-white/20 text-white/70" : "border-border-subtle text-secondary"
            )}>
              <Languages size={10} className="inline mr-1" />
              {message.translated_text}
            </div>
          )}

          <p
            className={cn(
              "text-[9px] mt-1 flex items-center gap-1",
              message.is_own ? "text-white/60" : "text-tertiary"
            )}
          >
            {time}
            {message.edited_at && <span>(edited)</span>}
            {message.scheduled_at && !message.delivery_status?.startsWith("sent") && (
              <span className="inline-flex items-center ml-0.5" title={`Scheduled: ${new Date(message.scheduled_at).toLocaleString()}`}>
                <Clock size={9} className="text-warning" />
              </span>
            )}
            {/* Delivery status + read receipt indicators (own messages only) */}
            {message.is_own && (
              <span className="inline-flex items-center ml-0.5" title={message.delivery_status === "failed" ? "Delivery failed" : message.delivery_status === "pending" ? "Pending" : readCount > 0 ? "Delivered and read" : "Sent"}>
                {message.delivery_status === "failed" ? (
                  <AlertTriangle size={10} className="text-red-400" />
                ) : message.delivery_status === "pending" ? (
                  <Clock size={10} className="text-yellow-300" />
                ) : readCount > 0 ? (
                  <CheckCheck size={10} className="text-blue-300" />
                ) : (
                  <Check size={10} />
                )}
              </span>
            )}
          </p>
          {message.is_own && readCount > 0 && (
            <p className="text-[9px] mt-0.5 text-white/60">
              Seen by {readCount}
            </p>
          )}
        </div>

        {/* Thread reply count link */}
        {threadReplyCount != null && threadReplyCount > 0 && (
          <button
            onClick={() => onOpenThread?.(message)}
            className={cn(
              "flex items-center gap-1 mt-1 text-[11px] font-medium hover:underline",
              message.is_own ? "text-blue-200" : "text-accent"
            )}
          >
            <MessagesSquare size={12} />
            {threadReplyCount} {threadReplyCount === 1 ? "reply" : "replies"}
          </button>
        )}

        {/* Reactions display */}
        {message.reactions && Object.keys(message.reactions).length > 0 && (
          <div className={cn("flex flex-wrap gap-1 mt-1", message.is_own ? "justify-end" : "justify-start")}>
            {Object.entries(message.reactions).map(([emoji, users]) => {
              const isOwn = currentUserUri !== "" && users.includes(currentUserUri);
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

// ─── Approvals Panel (accessible via /approvals command or external import) ───

export function ApprovalsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [approvals, setApprovals] = useState<any[]>([]);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [approvers, setApprovers] = useState("");
  const [showCreate, setShowCreate] = useState(false);

  useEffect(() => {
    paleServerApi<any[]>(baseUrl, token, "/v1/approvals")
      .then(setApprovals)
      .catch(() => {});
  }, [baseUrl, token]);

  const create = async () => {
    if (!title || !approvers) return;
    try {
      const approval = await paleServerApi<any>(baseUrl, token, "/v1/approvals", {
        method: "POST",
        body: {
          title,
          description: description || undefined,
          approvers: approvers.split(",").map((s) => s.trim()).filter(Boolean),
        },
      });
      setApprovals([approval, ...approvals]);
      setTitle("");
      setDescription("");
      setApprovers("");
      setShowCreate(false);
    } catch (err) {
      toast({ type: "error", title: "Failed to create approval", description: String(err) });
    }
  };

  const respond = async (id: string, decision: string) => {
    try {
      const updated = await paleServerApi<any>(baseUrl, token, `/v1/approvals/${id}/respond`, {
        method: "POST",
        body: { decision },
      });
      setApprovals(approvals.map((a) => (a.id === id ? updated : a)));
    } catch (err) {
      toast({ type: "error", title: "Failed to respond", description: String(err) });
    }
  };

  return (
    <div className="p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-primary">Approvals</h3>
        <button
          onClick={() => setShowCreate(!showCreate)}
          className="text-xs text-accent hover:underline"
        >
          {showCreate ? "Cancel" : "New Request"}
        </button>
      </div>

      {showCreate && (
        <div className="space-y-2 p-3 rounded-lg border border-border-subtle bg-surface">
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Title"
            className="w-full bg-elevated border border-border-subtle rounded px-2 py-1 text-sm"
          />
          <input
            type="text"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Description (optional)"
            className="w-full bg-elevated border border-border-subtle rounded px-2 py-1 text-sm"
          />
          <input
            type="text"
            value={approvers}
            onChange={(e) => setApprovers(e.target.value)}
            placeholder="Approvers (comma-separated SIP URIs)"
            className="w-full bg-elevated border border-border-subtle rounded px-2 py-1 text-sm"
          />
          <button
            onClick={create}
            disabled={!title || !approvers}
            className="px-3 py-1 rounded bg-accent text-inverse text-xs font-medium disabled:opacity-60"
          >
            Submit Request
          </button>
        </div>
      )}

      {approvals.length === 0 ? (
        <p className="text-sm text-tertiary">No approval requests.</p>
      ) : (
        approvals.map((a) => (
          <div key={a.id} className="p-3 rounded-lg border border-border-subtle bg-surface space-y-1">
            <div className="flex items-center justify-between">
              <p className="text-sm font-medium text-primary">{a.title}</p>
              <span
                className={cn(
                  "text-[10px] px-1.5 py-0.5 rounded",
                  a.status === "approved"
                    ? "bg-success/10 text-success"
                    : a.status === "rejected"
                    ? "bg-destructive/10 text-destructive"
                    : "bg-accent/10 text-accent"
                )}
              >
                {a.status}
              </span>
            </div>
            {a.description && (
              <p className="text-xs text-secondary">{a.description}</p>
            )}
            <p className="text-[10px] text-tertiary">
              By {a.requestor} &middot; {new Date(a.created_at).toLocaleString()}
            </p>
            {a.status === "pending" && (
              <div className="flex gap-2 pt-1">
                <button
                  onClick={() => respond(a.id, "approve")}
                  className="px-2 py-0.5 rounded bg-success/10 text-success text-xs hover:bg-success/20"
                >
                  Approve
                </button>
                <button
                  onClick={() => respond(a.id, "reject")}
                  className="px-2 py-0.5 rounded bg-destructive/10 text-destructive text-xs hover:bg-destructive/20"
                >
                  Reject
                </button>
              </div>
            )}
          </div>
        ))
      )}
    </div>
  );
}
