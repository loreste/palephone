import { Phone, MessageSquare, Users, FolderLock, Clock, CalendarDays, ShieldCheck, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/cn";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import { useServerStore } from "@/store/serverStore";
import type { Tab } from "@/types";

const allTabs: { id: Tab; labelKey: string; icon: typeof Phone; adminOnly?: boolean }[] = [
  { id: "dialpad", labelKey: "nav.calls", icon: Phone },
  { id: "chat", labelKey: "nav.chat", icon: MessageSquare },
  { id: "people", labelKey: "nav.people", icon: Users },
  { id: "files", labelKey: "nav.files", icon: FolderLock },
  { id: "recent", labelKey: "nav.recent", icon: Clock },
  { id: "calendar", labelKey: "nav.calendar", icon: CalendarDays },
  { id: "admin", labelKey: "nav.admin", icon: ShieldCheck, adminOnly: true },
  { id: "settings", labelKey: "nav.settings", icon: Settings },
];

export function BottomNav({ variant = "bottom" }: { variant?: "bottom" | "rail" }) {
  const { t } = useTranslation();
  const { activeTab, setActiveTab } = useUiStore();
  const userRole = useServerStore((s) => s.userRole);
  const tabs = allTabs.filter((tab) => !tab.adminOnly || userRole === "admin");
  const totalUnread = useChatStore((s) =>
    s.rooms.reduce((sum, r) => sum + r.unread_count, 0)
  );
  const isRail = variant === "rail";

  return (
    <nav
      className={cn(
        "bg-surface/95 border-border-subtle shrink-0",
        isRail
          ? "w-[76px] border-r py-3 flex flex-col items-center gap-1"
          : "flex items-stretch h-[56px] border-t"
      )}
    >
      {tabs.map(({ id, labelKey, icon: Icon }) => {
        const label = t(labelKey);
        const isActive = activeTab === id;
        return (
          <button
            key={id}
            onClick={() => setActiveTab(id)}
            className={cn(
              "group relative transition-colors",
              isRail
                ? "w-[60px] h-[54px] rounded-md flex flex-col items-center justify-center gap-1"
                : "flex-1 flex flex-col items-center justify-center gap-0.5",
              isActive
                ? "text-accent bg-accent-muted"
                : "text-tertiary hover:text-secondary hover:bg-elevated/70"
            )}
            aria-label={label}
            aria-current={isActive ? "page" : undefined}
            title={label}
          >
            {/* Active indicator bar */}
            {isActive && (
              <span
                className={cn(
                  "absolute bg-accent rounded-full",
                  isRail ? "left-0 top-3 bottom-3 w-[3px]" : "top-0 left-1/4 right-1/4 h-[2px]"
                )}
              />
            )}
            <span className="relative">
              <Icon size={20} strokeWidth={isActive ? 2 : 1.5} />
              {id === "chat" && totalUnread > 0 && (
                <span className="absolute -top-1 -right-2 w-3.5 h-3.5 rounded-full bg-accent text-white text-[8px] font-bold flex items-center justify-center">
                  {totalUnread > 9 ? "9+" : totalUnread}
                </span>
              )}
            </span>
            <span className="text-[10px] font-medium leading-none max-w-[58px] truncate">{label}</span>
          </button>
        );
      })}
    </nav>
  );
}
