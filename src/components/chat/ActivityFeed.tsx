import { MessageSquare, Phone, AtSign, Info, X, CheckCheck } from "lucide-react";
import { cn } from "@/lib/cn";
import { useActivityStore, type ActivityItem } from "@/store/activityStore";
import { useChatStore } from "@/store/chatStore";
import { useUiStore } from "@/store/uiStore";

const typeIcons = {
  message: MessageSquare,
  mention: AtSign,
  missed_call: Phone,
  system: Info,
};

const typeColors = {
  message: "text-accent",
  mention: "text-warning",
  missed_call: "text-destructive",
  system: "text-tertiary",
};

export function ActivityFeed({ onClose }: { onClose: () => void }) {
  const { items, markRead, markAllRead, clearAll } = useActivityStore();
  const setActiveRoomId = useChatStore((s) => s.setActiveRoomId);
  const setActiveTab = useUiStore((s) => s.setActiveTab);

  const handleClick = (item: ActivityItem) => {
    markRead(item.id);
    if (item.room_id) {
      setActiveRoomId(item.room_id);
      setActiveTab("chat");
      onClose();
    }
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts * 1000);
    const now = Date.now();
    const diff = Math.floor((now - d.getTime()) / 1000);
    if (diff < 60) return "now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
    return d.toLocaleDateString([], { month: "short", day: "numeric" });
  };

  return (
    <div className="absolute top-full right-0 mt-1 w-72 max-h-80 bg-surface border border-border-subtle rounded-lg shadow-xl z-50 flex flex-col">
      <div className="flex items-center justify-between px-3 py-2 border-b border-border-subtle shrink-0">
        <span className="text-xs font-semibold text-primary">Activity</span>
        <div className="flex items-center gap-1">
          <button
            onClick={markAllRead}
            className="text-[10px] text-accent hover:underline"
            title="Mark all read"
          >
            <CheckCheck size={12} />
          </button>
          <button
            onClick={clearAll}
            className="text-[10px] text-tertiary hover:text-destructive"
            title="Clear all"
          >
            <X size={12} />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        {items.length === 0 ? (
          <div className="flex items-center justify-center h-20">
            <p className="text-xs text-tertiary">No activity yet</p>
          </div>
        ) : (
          items.map((item) => {
            const Icon = typeIcons[item.type];
            return (
              <button
                key={item.id}
                onClick={() => handleClick(item)}
                className={cn(
                  "w-full flex items-start gap-2 px-3 py-2 text-left hover:bg-elevated transition-colors",
                  !item.read && "bg-accent/5"
                )}
              >
                <Icon size={14} className={cn("shrink-0 mt-0.5", typeColors[item.type])} />
                <div className="flex-1 min-w-0">
                  <p className={cn("text-xs truncate", item.read ? "text-secondary" : "text-primary font-medium")}>
                    {item.title}
                  </p>
                  <p className="text-[10px] text-tertiary truncate">{item.body}</p>
                </div>
                <span className="text-[9px] text-tertiary shrink-0">{formatTime(item.timestamp)}</span>
                {!item.read && (
                  <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0 mt-1" />
                )}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
