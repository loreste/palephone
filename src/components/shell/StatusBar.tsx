import { Volume2, PhoneIncoming } from "lucide-react";
import { cn } from "@/lib/cn";
import { useAccountStore } from "@/store/accountStore";
import { useCallStore } from "@/store/callStore";
import type { RegState } from "@/types";

const regConfig: Record<RegState, { color: string; label: string; animate: boolean }> = {
  registered: { color: "bg-success", label: "Registered", animate: false },
  registering: { color: "bg-warning", label: "Registering...", animate: true },
  unregistered: { color: "bg-destructive", label: "Unregistered", animate: false },
  none: { color: "bg-tertiary", label: "No Account", animate: false },
};

export function StatusBar() {
  const { account, regState } = useAccountStore();
  const { setIncomingCall } = useCallStore();
  const config = regConfig[regState];

  const simulateIncoming = () => {
    setIncomingCall({
      id: Date.now(),
      direction: "inbound",
      state: "ringing",
      remoteUri: "sip:bob@example.com",
      remoteName: "Bob Chen",
      startTime: Date.now(),
      connectTime: null,
      isMuted: false,
      isHeld: false,
    });
  };

  return (
    <div
      className={cn(
        "flex items-center justify-between h-[36px] px-3",
        "bg-surface border-b border-border-subtle",
        "shrink-0"
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        {/* Status dot */}
        <span className="relative flex items-center justify-center w-2 h-2 shrink-0">
          <span
            className={cn(
              "w-2 h-2 rounded-full",
              config.color,
              config.animate && "animate-status-pulse"
            )}
          />
          {regState === "registered" && (
            <span
              className="absolute inset-0 rounded-full bg-success opacity-30 blur-[3px]"
              aria-hidden
            />
          )}
        </span>

        {/* Account info */}
        <span className="text-xs text-secondary truncate">
          {account?.sipUri ?? config.label}
        </span>
      </div>

      <div className="flex items-center gap-1">
        {/* Dev: simulate incoming call */}
        <button
          onClick={simulateIncoming}
          className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-info transition-colors"
          aria-label="Simulate incoming call"
          title="Dev: Simulate incoming call"
        >
          <PhoneIncoming size={13} />
        </button>

        <button
          className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Audio settings"
        >
          <Volume2 size={14} />
        </button>
      </div>
    </div>
  );
}
