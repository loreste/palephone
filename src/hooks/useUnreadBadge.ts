import { useEffect } from "react";
import { useChatStore } from "@/store/chatStore";
import { setAppBadge } from "@/lib/nativeNotify";

/**
 * Mirror the total unread message count onto the app's dock/taskbar badge,
 * Teams-style. Recomputes whenever any room's unread count changes.
 */
export function useUnreadBadge() {
  const totalUnread = useChatStore((s) =>
    s.rooms.reduce((sum, room) => sum + (room.unread_count || 0), 0),
  );

  useEffect(() => {
    setAppBadge(totalUnread);
  }, [totalUnread]);
}
