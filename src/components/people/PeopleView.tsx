import { useState, useEffect, useCallback } from "react";
import { Search, Phone, MessageSquare, Users } from "lucide-react";
import { cn } from "@/lib/cn";
import { useServerStore } from "@/store/serverStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { paleServerGetUsers, makeCall, type ServerUser } from "@/lib/tauri";
import { matrixCreateDm } from "@/lib/tauri";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { toast } from "@/components/ui/Toast";

const presenceColors: Record<PresenceStatus, string> = {
  online: "bg-green-500",
  busy: "bg-red-500",
  on_call: "bg-red-500",
  away: "bg-yellow-500",
  dnd: "bg-red-600",
  offline: "bg-gray-400",
};

const presenceLabels: Record<PresenceStatus, string> = {
  online: "Online",
  busy: "Busy",
  on_call: "On a call",
  away: "Away",
  dnd: "Do Not Disturb",
  offline: "Offline",
};

export function PeopleView() {
  const { baseUrl, token, connected } = useServerStore();
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const [users, setUsers] = useState<ServerUser[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(false);

  const loadUsers = useCallback(async () => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      setUsers(await paleServerGetUsers(baseUrl, token));
    } catch {
      // Server might not be connected
    } finally {
      setLoading(false);
    }
  }, [baseUrl, token]);

  useEffect(() => {
    if (connected) loadUsers();
  }, [connected, loadUsers]);

  const filtered = searchQuery
    ? users.filter(
        (u) =>
          u.display_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          u.sip_uri.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : users;

  // Sort: online users first, then alphabetical
  const sorted = [...filtered].sort((a, b) => {
    const pa = presenceMap[a.sip_uri]?.status ?? "offline";
    const pb = presenceMap[b.sip_uri]?.status ?? "offline";
    if (pa === "online" && pb !== "online") return -1;
    if (pa !== "online" && pb === "online") return 1;
    return a.display_name.localeCompare(b.display_name);
  });

  const onlineCount = users.filter(
    (u) => (presenceMap[u.sip_uri]?.status ?? "offline") !== "offline"
  ).length;

  if (!connected) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 px-6">
        <Users size={32} className="text-tertiary" />
        <p className="text-sm text-tertiary text-center">
          Connect to a Pale server to see your organization's directory
        </p>
        <p className="text-xs text-tertiary">Go to Settings &rarr; Server</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-4 pt-4 pb-2 flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-primary">People</h1>
          <p className="text-xs text-tertiary">
            {onlineCount} online &middot; {users.length} total
          </p>
        </div>
      </div>

      <div className="px-4 pb-3">
        <div className="relative">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-tertiary" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search people..."
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-lg",
              "pl-8 pr-3 py-2 text-sm text-primary",
              "placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus"
            )}
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2">
        {loading && users.length === 0 ? (
          <div className="flex items-center justify-center h-32">
            <p className="text-sm text-tertiary">Loading...</p>
          </div>
        ) : sorted.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 gap-2">
            <Users size={32} className="text-tertiary" />
            <p className="text-sm text-tertiary">
              {searchQuery ? "No matches found" : "No users in directory"}
            </p>
          </div>
        ) : (
          sorted.map((user) => (
            <PersonRow key={user.id} user={user} />
          ))
        )}
      </div>
    </div>
  );
}

function PersonRow({ user }: { user: ServerUser }) {
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const setActiveRoomId = useChatStore((s) => s.setActiveRoomId);
  const rooms = useChatStore((s) => s.rooms);
  const presence = presenceMap[user.sip_uri];
  const status: PresenceStatus = presence?.status ?? "offline";

  const handleCall = async () => {
    try {
      await makeCall(user.sip_uri);
    } catch (err) {
      toast({ type: "error", title: "Call failed", description: String(err) });
    }
  };

  const handleChat = async () => {
    // Check if DM room already exists
    const existing = rooms.find(
      (r) => r.is_direct && r.name.toLowerCase().includes(user.display_name.toLowerCase())
    );
    if (existing) {
      setActiveRoomId(existing.room_id);
      setActiveTab("chat");
      return;
    }

    // Create new DM if user has a Matrix ID
    if (user.matrix_user_id) {
      try {
        const roomId = await matrixCreateDm(user.matrix_user_id);
        setActiveRoomId(roomId);
        setActiveTab("chat");
      } catch (err) {
        toast({ type: "error", title: "Could not start chat", description: String(err) });
      }
    } else {
      toast({ type: "info", title: "No Matrix ID", description: "This user doesn't have a chat account configured" });
    }
  };

  return (
    <div
      className={cn(
        "group flex items-center gap-3 px-3 py-2.5 rounded-lg",
        "hover:bg-elevated transition-colors"
      )}
    >
      <div className="relative shrink-0">
        <CallerAvatar name={user.display_name} size="sm" />
        {status === "on_call" ? (
          <span className="absolute -bottom-0.5 -right-0.5 w-4 h-4 rounded-full border-2 border-surface bg-red-500 flex items-center justify-center">
            <Phone size={8} className="text-white animate-pulse" fill="currentColor" />
          </span>
        ) : (
          <span
            className={cn(
              "absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-surface",
              presenceColors[status]
            )}
          />
        )}
      </div>

      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-primary truncate">{user.display_name}</p>
        <p className="text-[10px] text-tertiary truncate">
          {presence?.note ?? presenceLabels[status]}
          {user.sip_uri && ` · ${user.sip_uri}`}
        </p>
      </div>

      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
        <button
          onClick={handleCall}
          className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-accent/10 transition-colors"
          title="Call"
        >
          <Phone size={14} />
        </button>
        <button
          onClick={handleChat}
          className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-accent/10 transition-colors"
          title="Chat"
        >
          <MessageSquare size={14} />
        </button>
      </div>
    </div>
  );
}
