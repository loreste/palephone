import { Phone, MessageSquare, Users, FolderLock, Clock, ShieldCheck, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/cn";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import type { Tab } from "@/types";

const tabs: { id: Tab; labelKey: string; icon: typeof Phone }[] = [
  { id: "dialpad", labelKey: "nav.calls", icon: Phone },
  { id: "chat", labelKey: "nav.chat", icon: MessageSquare },
  { id: "people", labelKey: "nav.people", icon: Users },
  { id: "files", labelKey: "nav.files", icon: FolderLock },
  { id: "recent", labelKey: "nav.recent", icon: Clock },
  { id: "admin", labelKey: "nav.admin", icon: ShieldCheck },
  { id: "settings", labelKey: "nav.settings", icon: Settings },
];

export function BottomNav() {
  const { t } = useTranslation();
  const { activeTab, setActiveTab } = useUiStore();
  const totalUnread = useChatStore((s) =>
    s.rooms.reduce((sum, r) => sum + r.unread_count, 0)
  );

  return (
    <nav
      className={cn(
        "flex items-stretch h-[56px] md:h-[48px]",
        "bg-surface border-t border-border-subtle",
        "shrink-0"
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
              "flex-1 flex flex-col items-center justify-center gap-0.5",
              "transition-colors relative",
              isActive ? "text-accent" : "text-tertiary hover:text-secondary"
            )}
            aria-label={label}
            aria-current={isActive ? "page" : undefined}
          >
            {/* Active indicator bar */}
            {isActive && (
              <span className="absolute top-0 left-1/4 right-1/4 h-[2px] bg-accent rounded-full" />
            )}
            <span className="relative">
              <Icon size={20} strokeWidth={isActive ? 2 : 1.5} />
              {id === "chat" && totalUnread > 0 && (
                <span className="absolute -top-1 -right-2 w-3.5 h-3.5 rounded-full bg-accent text-white text-[8px] font-bold flex items-center justify-center">
                  {totalUnread > 9 ? "9+" : totalUnread}
                </span>
              )}
            </span>
            <span className="text-[10px] font-medium">{label}</span>
          </button>
        );
      })}
    </nav>
  );
}
