import { Phone, Pause } from "lucide-react";
import { cn } from "@/lib/cn";
import { useCallStore } from "@/store/callStore";

/**
 * Shows a compact line indicator when multiple calls exist.
 * Allows switching between active calls.
 */
export function CallLineIndicator() {
  const { sessions, activeCallId, setActiveCallId, setHeld } = useCallStore();

  if (sessions.length <= 1) return null;

  return (
    <div className="flex items-center gap-1 px-4 pb-2">
      {sessions.map((session) => {
        const isActive = session.id === activeCallId;
        const isHeld = session.isHeld || session.state === "on_hold";

        return (
          <button
            key={session.id}
            onClick={() => {
              if (!isActive) {
                // Hold current call, switch to this one
                if (activeCallId !== null) {
                  setHeld(activeCallId, true);
                }
                setActiveCallId(session.id);
                setHeld(session.id, false);
              }
            }}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium",
              "transition-all",
              isActive
                ? "bg-accent text-white"
                : isHeld
                  ? "bg-warning-muted text-warning border border-warning/30"
                  : "bg-elevated text-secondary"
            )}
          >
            {isHeld ? <Pause size={10} /> : <Phone size={10} />}
            <span className="truncate max-w-[80px]">
              {session.remoteName || session.remoteUri.split("@")[0]}
            </span>
          </button>
        );
      })}
    </div>
  );
}
