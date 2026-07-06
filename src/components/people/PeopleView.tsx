import { useState, useEffect, useCallback } from "react";
import { Search, Phone, MessageSquare, Users, Star, X, Video } from "lucide-react";
import { cn } from "@/lib/cn";
import { useServerStore } from "@/store/serverStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { paleServerCreateDirectRoom, paleServerGetUsers, makeCall, makeVideoCall, paleServerGetFavorites, paleServerAddFavorite, paleServerRemoveFavorite, type ServerRoom, type ServerUser } from "@/lib/tauri";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import { useAccountStore } from "@/store/accountStore";
import { preflightSipCall } from "@/lib/callTargets";
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

function serverRoomToSummary(room: ServerRoom) {
  return {
    room_id: room.id,
    name: room.name,
    is_direct: room.is_direct,
    is_encrypted: false,
    last_message: null,
    last_message_sender: null,
    last_message_ts: null,
    unread_count: 0,
  };
}

export function PeopleView() {
  const { baseUrl, token, connected } = useServerStore();
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const [users, setUsers] = useState<ServerUser[]>([]);
  const [favorites, setFavorites] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [profileUser, setProfileUser] = useState<ServerUser | null>(null);

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

  const loadFavorites = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      setFavorites(await paleServerGetFavorites(baseUrl, token));
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => {
    if (connected) {
      loadUsers();
      loadFavorites();
    }
  }, [connected, loadUsers, loadFavorites]);

  const toggleFavorite = async (sipUri: string) => {
    if (!baseUrl || !token) return;
    try {
      if (favorites.includes(sipUri)) {
        await paleServerRemoveFavorite(baseUrl, token, sipUri);
        setFavorites((f) => f.filter((u) => u !== sipUri));
      } else {
        await paleServerAddFavorite(baseUrl, token, sipUri);
        setFavorites((f) => [...f, sipUri]);
      }
    } catch (err) {
      toast({ type: "error", title: "Failed to update favorite", description: String(err) });
    }
  };

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
          <>
            {/* Favorites section */}
            {!searchQuery && favorites.length > 0 && (
              <>
                <p className="px-2 py-1.5 text-[10px] font-semibold text-tertiary uppercase tracking-wider">
                  Favorites
                </p>
                {sorted
                  .filter((u) => favorites.includes(u.sip_uri))
                  .map((user) => (
                    <PersonRow
                      key={`fav-${user.id}`}
                      user={user}
                      isFavorite
                      onToggleFavorite={toggleFavorite}
                      onShowProfile={setProfileUser}
                    />
                  ))}
                <p className="px-2 py-1.5 text-[10px] font-semibold text-tertiary uppercase tracking-wider mt-2">
                  All Users
                </p>
              </>
            )}
            {sorted.map((user) => (
              <PersonRow
                key={user.id}
                user={user}
                isFavorite={favorites.includes(user.sip_uri)}
                onToggleFavorite={toggleFavorite}
                onShowProfile={setProfileUser}
              />
            ))}
          </>
        )}
      </div>

      {/* Profile card modal */}
      {profileUser && (
        <UserProfileCard
          user={profileUser}
          onClose={() => setProfileUser(null)}
        />
      )}
    </div>
  );
}

function PersonRow({
  user,
  isFavorite,
  onToggleFavorite,
  onShowProfile,
}: {
  user: ServerUser;
  isFavorite?: boolean;
  onToggleFavorite?: (sipUri: string) => void;
  onShowProfile?: (user: ServerUser) => void;
}) {
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const setActiveRoomId = useChatStore((s) => s.setActiveRoomId);
  const upsertRoom = useChatStore((s) => s.upsertRoom);
  const rooms = useChatStore((s) => s.rooms);
  const { baseUrl, token } = useServerStore();
  const account = useAccountStore((s) => s.account);
  const regState = useAccountStore((s) => s.regState);
  const presence = presenceMap[user.sip_uri];
  const status: PresenceStatus = presence?.status ?? "offline";

  const handleCall = async () => {
    const target = preflightSipCall(user.sip_uri, account, regState);
    if (!target.ok) {
      toast({ type: "error", title: "Call unavailable", description: target.reason });
      return;
    }
    try {
      await makeCall(target.uri);
    } catch (err) {
      toast({ type: "error", title: "Call failed", description: String(err) });
    }
  };

  const handleVideoCall = async () => {
    const target = preflightSipCall(user.sip_uri, account, regState);
    if (!target.ok) {
      toast({ type: "error", title: "Video unavailable", description: target.reason });
      return;
    }
    try {
      await makeVideoCall(target.uri);
    } catch (err) {
      toast({ type: "error", title: "Video call failed", description: String(err) });
    }
  };

  const handleChat = async () => {
    const existing = rooms.find(
      (r) => r.is_direct && r.name.toLowerCase().includes(user.display_name.toLowerCase())
    );
    if (existing) {
      setActiveRoomId(existing.room_id);
      setActiveTab("chat");
      return;
    }

    if (!baseUrl || !token) {
      toast({ type: "error", title: "Not connected to server" });
      return;
    }

    try {
      const room = await paleServerCreateDirectRoom(baseUrl, token, user);
      upsertRoom(serverRoomToSummary(room));
      setActiveRoomId(room.id);
      setActiveTab("chat");
    } catch (err) {
      toast({ type: "error", title: "Could not start chat", description: String(err) });
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

      <div className="flex-1 min-w-0 cursor-pointer" onClick={() => onShowProfile?.(user)}>
        <p className="text-sm font-medium text-primary truncate">{user.display_name}</p>
        <p className="text-[10px] text-tertiary truncate">
          {presence?.note ?? presenceLabels[status]}
          {user.sip_uri && ` · ${user.sip_uri}`}
        </p>
      </div>

      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
        <button
          onClick={() => onToggleFavorite?.(user.sip_uri)}
          className={cn(
            "p-1.5 rounded-md transition-colors",
            isFavorite ? "text-yellow-500" : "text-tertiary hover:text-yellow-500 hover:bg-yellow-500/10"
          )}
          title={isFavorite ? "Remove from favorites" : "Add to favorites"}
        >
          <Star size={14} fill={isFavorite ? "currentColor" : "none"} />
        </button>
        <button
          onClick={handleCall}
          className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-accent/10 transition-colors"
          title="Call"
        >
          <Phone size={14} />
        </button>
        <button
          onClick={handleVideoCall}
          className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-accent/10 transition-colors"
          title="Video call"
        >
          <Video size={14} />
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

function UserProfileCard({ user, onClose }: { user: ServerUser; onClose: () => void }) {
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const setActiveRoomId = useChatStore((s) => s.setActiveRoomId);
  const upsertRoom = useChatStore((s) => s.upsertRoom);
  const rooms = useChatStore((s) => s.rooms);
  const { baseUrl, token } = useServerStore();
  const account = useAccountStore((s) => s.account);
  const regState = useAccountStore((s) => s.regState);
  const presence = presenceMap[user.sip_uri];
  const status: PresenceStatus = presence?.status ?? "offline";

  const handleCall = async () => {
    onClose();
    const target = preflightSipCall(user.sip_uri, account, regState);
    if (!target.ok) {
      toast({ type: "error", title: "Call unavailable", description: target.reason });
      return;
    }
    try {
      await makeCall(target.uri);
    } catch (err) {
      toast({ type: "error", title: "Call failed", description: String(err) });
    }
  };

  const handleVideoCall = async () => {
    onClose();
    const target = preflightSipCall(user.sip_uri, account, regState);
    if (!target.ok) {
      toast({ type: "error", title: "Video unavailable", description: target.reason });
      return;
    }
    try {
      await makeVideoCall(target.uri);
    } catch (err) {
      toast({ type: "error", title: "Video call failed", description: String(err) });
    }
  };

  const handleChat = async () => {
    onClose();
    const existing = rooms.find(
      (r) => r.is_direct && r.name.toLowerCase().includes(user.display_name.toLowerCase())
    );
    if (existing) {
      setActiveRoomId(existing.room_id);
      setActiveTab("chat");
      return;
    }
    if (!baseUrl || !token) {
      toast({ type: "error", title: "Not connected to server" });
      return;
    }

    try {
      const room = await paleServerCreateDirectRoom(baseUrl, token, user);
      upsertRoom(serverRoomToSummary(room));
      setActiveRoomId(room.id);
      setActiveTab("chat");
    } catch (err) {
      toast({ type: "error", title: "Could not start chat", description: String(err) });
    }
  };

  return (
    <div className="fixed inset-0 bg-black/40 z-50 flex items-center justify-center" onClick={onClose}>
      <div
        className="bg-surface border border-border-subtle rounded-xl shadow-xl w-72 p-5 space-y-3"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <CallerAvatar name={user.display_name} size="md" />
            <div>
              <p className="text-sm font-semibold text-primary">{user.display_name}</p>
              <div className="flex items-center gap-1 mt-0.5">
                <span className={cn("w-2 h-2 rounded-full", presenceColors[status])} />
                <span className="text-[10px] text-tertiary">{presenceLabels[status]}</span>
              </div>
            </div>
          </div>
          <button onClick={onClose} className="p-1 rounded text-tertiary hover:text-primary">
            <X size={14} />
          </button>
        </div>

        {presence?.note && (
          <p className="text-xs text-secondary italic">{presence.note}</p>
        )}

        <div className="space-y-1.5 text-xs">
          <div className="flex justify-between">
            <span className="text-tertiary">SIP URI</span>
            <span className="text-primary font-mono">{user.sip_uri}</span>
          </div>
          {user.matrix_user_id && (
            <div className="flex justify-between">
              <span className="text-tertiary">Matrix</span>
              <span className="text-primary font-mono">{user.matrix_user_id}</span>
            </div>
          )}
        </div>

        <div className="grid grid-cols-3 gap-2 pt-1">
          <button
            onClick={handleCall}
            className={cn(
              "flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium",
              "bg-success/10 text-success hover:bg-success/20 transition-colors"
            )}
          >
            <Phone size={13} /> Call
          </button>
          <button
            onClick={handleVideoCall}
            className={cn(
              "flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium",
              "bg-accent/10 text-accent hover:bg-accent/20 transition-colors"
            )}
          >
            <Video size={13} /> Video
          </button>
          <button
            onClick={handleChat}
            className={cn(
              "flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium",
              "bg-accent/10 text-accent hover:bg-accent/20 transition-colors"
            )}
          >
            <MessageSquare size={13} /> Chat
          </button>
        </div>
      </div>
    </div>
  );
}
