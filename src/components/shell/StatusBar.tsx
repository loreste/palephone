import { useState, useRef, useEffect } from "react";
import { Search, Server, Volume2 } from "lucide-react";
import { cn } from "@/lib/cn";
import { useAccountStore } from "@/store/accountStore";
import { useServerStore } from "@/store/serverStore";
import { usePresenceStore, type PresenceStatus } from "@/store/presenceStore";
import { paleServerSetPresence } from "@/lib/tauri";
import type { RegState } from "@/types";

const regConfig: Record<RegState, { color: string; label: string; animate: boolean }> = {
  registered: { color: "bg-success", label: "Registered", animate: false },
  registering: { color: "bg-warning", label: "Registering...", animate: true },
  unregistered: { color: "bg-destructive", label: "Unregistered", animate: false },
  none: { color: "bg-tertiary", label: "No Account", animate: false },
};

const presenceOptions: { status: PresenceStatus; label: string; color: string }[] = [
  { status: "online", label: "Online", color: "bg-green-500" },
  { status: "busy", label: "Busy", color: "bg-red-500" },
  { status: "on_call", label: "On a call", color: "bg-red-500" },
  { status: "away", label: "Away", color: "bg-yellow-500" },
  { status: "dnd", label: "Do Not Disturb", color: "bg-red-600" },
  { status: "offline", label: "Appear Offline", color: "bg-gray-400" },
];

export function StatusBar() {
  const { account, regState } = useAccountStore();
  const { baseUrl, token, connected: serverConnected } = useServerStore();
  const config = regConfig[regState];

  return (
    <div
      className={cn(
        "flex items-center justify-between h-[40px] px-3",
        "bg-surface/95 border-b border-border-subtle",
        "shrink-0"
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        <span className="hidden sm:inline-flex text-[11px] font-semibold uppercase tracking-[0.08em] text-tertiary">
          Pale
        </span>
        <span className="hidden sm:block h-4 w-px bg-border-subtle" aria-hidden />

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

        <span className="text-xs text-secondary truncate">
          {account?.displayName ?? account?.sipUri ?? config.label}
        </span>

        {/* Server + presence indicator */}
        <PresenceIndicator
          serverConnected={serverConnected}
          baseUrl={baseUrl}
          token={token}
        />
      </div>

      <div className="flex items-center gap-1">
        <button
          onClick={() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true }))}
          className="p-1.5 rounded-md hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Search messages"
          title="Search (Cmd+F)"
        >
          <Search size={13} />
        </button>

        <button
          className="p-1.5 rounded-md hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Audio settings"
        >
          <Volume2 size={14} />
        </button>
      </div>
    </div>
  );
}

function PresenceIndicator({
  serverConnected,
  baseUrl,
  token,
}: {
  serverConnected: boolean;
  baseUrl: string | null;
  token: string | null;
}) {
  const [open, setOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const setPresence = usePresenceStore((s) => s.setPresence);

  // Determine current presence from own entry
  const presenceMap = usePresenceStore((s) => s.presenceMap);
  const account = useAccountStore((s) => s.account);
  const ownUri = account?.sipUri
    ? account.sipUri.startsWith("sip:")
      ? account.sipUri
      : `sip:${account.sipUri}`
    : null;
  const ownPresence = ownUri ? presenceMap[ownUri] : undefined;
  const currentStatus = ownPresence?.status ?? (serverConnected ? "online" : "offline");
  const currentColor = presenceOptions.find((p) => p.status === currentStatus)?.color ?? "bg-gray-400";

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const [statusMessage, setStatusMessage] = useState(ownPresence?.note ?? "");

  const handleSelect = async (status: PresenceStatus) => {
    setOpen(false);
    if (!baseUrl || !token) return;
    try {
      const result = await paleServerSetPresence(baseUrl, token, status, statusMessage || null);
      setPresence(result.sip_uri, result);
    } catch { /* ignore */ }
  };

  const handleStatusMessageSubmit = async () => {
    if (!baseUrl || !token) return;
    try {
      const result = await paleServerSetPresence(baseUrl, token, currentStatus, statusMessage || null);
      setPresence(result.sip_uri, result);
    } catch { /* ignore */ }
  };

  return (
    <div className="relative ml-2 shrink-0" ref={dropdownRef}>
      <button
        onClick={() => serverConnected && setOpen(!open)}
        className={cn(
          "flex items-center gap-1.5 h-6 px-1.5 rounded-md text-xs transition-colors",
          serverConnected
            ? "text-tertiary hover:text-secondary hover:bg-elevated"
            : "text-tertiary cursor-default"
        )}
        title={serverConnected ? `Status: ${currentStatus}` : "Server disconnected"}
      >
        <Server size={12} />
        <span className={cn("w-2 h-2 rounded-full", serverConnected ? currentColor : "bg-tertiary")} />
      </button>

      {open && (
        <div className="absolute top-full left-0 mt-1 w-52 bg-surface border border-border-subtle rounded-md shadow-lg z-50 py-1">
          {presenceOptions.map((opt) => (
            <button
              key={opt.status}
              onClick={() => handleSelect(opt.status)}
              className={cn(
                "w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left",
                "hover:bg-elevated transition-colors",
                currentStatus === opt.status && "text-accent font-medium"
              )}
            >
              <span className={cn("w-2 h-2 rounded-full shrink-0", opt.color)} />
              {opt.label}
            </button>
          ))}
          <div className="border-t border-border-subtle mt-1 pt-1 px-2 pb-1">
            <input
              type="text"
              value={statusMessage}
              onChange={(e) => setStatusMessage(e.target.value)}
              onBlur={handleStatusMessageSubmit}
              onKeyDown={(e) => { if (e.key === "Enter") handleStatusMessageSubmit(); }}
              placeholder="Set a status message..."
              className={cn(
                "w-full bg-elevated border border-border-subtle rounded px-2 py-1 text-[10px] text-primary",
                "placeholder:text-tertiary focus:outline-none focus:border-border-focus"
              )}
            />
          </div>
        </div>
      )}
    </div>
  );
}
