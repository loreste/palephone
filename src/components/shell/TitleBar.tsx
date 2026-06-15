import { useState, useRef, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Bell } from "lucide-react";
import { cn } from "@/lib/cn";
import { useActivityStore } from "@/store/activityStore";
import { ActivityFeed } from "@/components/chat/ActivityFeed";

const appWindow = getCurrentWindow();

export function TitleBar() {
  const [showActivity, setShowActivity] = useState(false);
  const activityRef = useRef<HTMLDivElement>(null);
  const unreadCount = useActivityStore((s) => s.items.filter((i) => !i.read).length);

  // Close on outside click
  useEffect(() => {
    if (!showActivity) return;
    const handleClick = (e: MouseEvent) => {
      if (activityRef.current && !activityRef.current.contains(e.target as Node)) {
        setShowActivity(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [showActivity]);

  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex items-center justify-between h-[28px] px-3",
        "bg-base border-b border-border-subtle",
        "shrink-0"
      )}
    >
      {/* macOS traffic lights get space on the left */}
      <div className="flex items-center gap-2 pl-[68px]">
        <span className="text-xs font-semibold text-secondary tracking-wide">
          PALE
        </span>
      </div>

      {/* Window controls (Windows/Linux only — macOS uses native) */}
      <div className="flex items-center gap-0.5 -mr-1">
        {/* Activity bell */}
        <div className="relative" ref={activityRef}>
          <button
            onClick={() => setShowActivity(!showActivity)}
            className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-secondary transition-colors relative"
            aria-label="Activity feed"
          >
            <Bell size={13} />
            {unreadCount > 0 && (
              <span className="absolute -top-0.5 -right-0.5 w-3.5 h-3.5 rounded-full bg-accent text-white text-[8px] font-bold flex items-center justify-center">
                {unreadCount > 9 ? "9+" : unreadCount}
              </span>
            )}
          </button>
          {showActivity && <ActivityFeed onClose={() => setShowActivity(false)} />}
        </div>

        <button
          onClick={() => appWindow.minimize()}
          className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Minimize"
        >
          <Minus size={14} />
        </button>
        <button
          onClick={() => appWindow.toggleMaximize()}
          className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Maximize"
        >
          <Square size={12} />
        </button>
        <button
          onClick={() => appWindow.close()}
          className="p-1 rounded-sm hover:bg-destructive/20 text-tertiary hover:text-destructive transition-colors"
          aria-label="Close"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
