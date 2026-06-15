import { useEffect, useState, useCallback } from "react";
import { PhoneOutgoing, PhoneIncoming, PhoneMissed, Trash2, Phone, Voicemail, Mic } from "lucide-react";
import { cn } from "@/lib/cn";
import { VoicemailView } from "./VoicemailView";
import { RecordingsView } from "./RecordingsView";
import {
  getCallHistory,
  deleteCallRecord,
  clearCallHistory,
  paleServerGetCallHistory,
  type CallRecord,
} from "@/lib/tauri";
import { useServerStore } from "@/store/serverStore";
import { toast } from "@/components/ui/Toast";

type RecentTab = "calls" | "voicemail" | "recordings";

type CallFilter = "all" | "missed" | "incoming" | "outgoing";

export function RecentCallsList() {
  const [recentTab, setRecentTab] = useState<RecentTab>("calls");
  const [records, setRecords] = useState<CallRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [callFilter, setCallFilter] = useState<CallFilter>("all");
  const { baseUrl, token, connected } = useServerStore();

  const loadHistory = useCallback(async () => {
    try {
      const localData = await getCallHistory().catch(() => [] as CallRecord[]);

      // Merge with server call history if connected
      if (connected && baseUrl && token) {
        const serverData = await paleServerGetCallHistory(baseUrl, token).catch(() => []);
        const serverRecords: CallRecord[] = serverData.map((entry) => ({
          id: -1, // Server entries don't have local IDs
          direction: entry.direction,
          remote_uri: entry.remote_uri,
          remote_name: entry.remote_name,
          start_time: entry.start_time,
          duration_secs: entry.duration_secs,
          answered: entry.answered,
        }));

        // Merge: deduplicate by start_time + remote_uri + direction
        const merged = [...localData];
        for (const sr of serverRecords) {
          const srTime = new Date(sr.start_time).getTime();
          const exists = merged.some(
            (lr) =>
              new Date(lr.start_time).getTime() === srTime &&
              lr.remote_uri === sr.remote_uri &&
              lr.direction === sr.direction
          );
          if (!exists) merged.push(sr);
        }

        // Sort by start_time descending
        merged.sort((a, b) => new Date(b.start_time).getTime() - new Date(a.start_time).getTime());
        setRecords(merged);
      } else {
        setRecords(localData);
      }
    } catch {
      setRecords([]);
    } finally {
      setLoading(false);
    }
  }, [connected, baseUrl, token]);

  useEffect(() => {
    loadHistory();
  }, [loadHistory]);

  const handleDelete = useCallback(
    async (id: number) => {
      try {
        await deleteCallRecord(id);
        setRecords((prev) => prev.filter((r) => r.id !== id));
      } catch {
        toast({ type: "error", title: "Failed to delete record" });
      }
    },
    []
  );

  const handleClearAll = useCallback(async () => {
    try {
      await clearCallHistory();
      setRecords([]);
      toast({ type: "success", title: "Call history cleared" });
    } catch {
      toast({ type: "error", title: "Failed to clear history" });
    }
  }, []);

  // Apply call filter
  const filteredRecords = records.filter((r) => {
    if (callFilter === "all") return true;
    if (callFilter === "missed") return r.direction === "inbound" && !r.answered;
    if (callFilter === "incoming") return r.direction === "inbound";
    if (callFilter === "outgoing") return r.direction === "outbound";
    return true;
  });

  // Group records by date
  const grouped = groupByDate(filteredRecords);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold text-primary">Recent</h1>
        {recentTab === "calls" && records.length > 0 && (
          <button
            onClick={handleClearAll}
            className="text-xs text-tertiary hover:text-destructive transition-colors"
          >
            Clear All
          </button>
        )}
      </div>

      <div className="flex gap-1 px-4 pb-2">
        {([
          { id: "calls" as RecentTab, label: "Calls", icon: Phone },
          { id: "voicemail" as RecentTab, label: "Voicemail", icon: Voicemail },
          { id: "recordings" as RecentTab, label: "Recordings", icon: Mic },
        ]).map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setRecentTab(id)}
            className={cn(
              "flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors",
              recentTab === id ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary hover:bg-elevated"
            )}
          >
            <Icon size={12} />
            {label}
          </button>
        ))}
      </div>

      {recentTab === "voicemail" && <div className="flex-1 overflow-y-auto"><VoicemailView /></div>}
      {recentTab === "recordings" && <div className="flex-1 overflow-y-auto"><RecordingsView /></div>}
      {recentTab === "calls" && <div className="flex-1 overflow-y-auto">
        {/* Call filter pills */}
        <div className="flex gap-1 px-4 pb-2">
          {(["all", "missed", "incoming", "outgoing"] as CallFilter[]).map((f) => (
            <button
              key={f}
              onClick={() => setCallFilter(f)}
              className={cn(
                "px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors capitalize",
                callFilter === f
                  ? "bg-accent-muted text-accent"
                  : "text-tertiary hover:text-secondary hover:bg-elevated"
              )}
            >
              {f}
            </button>
          ))}
        </div>
        {loading ? (
          <div className="flex items-center justify-center h-32">
            <p className="text-sm text-tertiary">Loading...</p>
          </div>
        ) : records.length === 0 ? (
          <div className="flex items-center justify-center h-32">
            <p className="text-sm text-tertiary">No recent calls</p>
          </div>
        ) : (
          Object.entries(grouped).map(([dateLabel, calls]) => (
            <div key={dateLabel} className="px-2">
              <p className="px-2 py-1.5 text-[10px] font-semibold text-tertiary uppercase tracking-wider">
                {dateLabel}
              </p>
              {calls.map((call) => (
                <RecentCallItem
                  key={call.id}
                  call={call}
                  onDelete={handleDelete}
                />
              ))}
            </div>
          ))
        )}
      </div>}
    </div>
  );
}

function RecentCallItem({
  call,
  onDelete,
}: {
  call: CallRecord;
  onDelete: (id: number) => void;
}) {
  const isMissed = call.direction === "inbound" && !call.answered;
  const Icon = isMissed
    ? PhoneMissed
    : call.direction === "outbound"
      ? PhoneOutgoing
      : PhoneIncoming;

  const duration =
    call.duration_secs > 0
      ? `${Math.floor(call.duration_secs / 60)}m ${call.duration_secs % 60}s`
      : "\u2014";

  const time = formatTime(call.start_time);

  return (
    <div
      className={cn(
        "group w-full flex items-center gap-3 px-2 py-2.5 rounded-lg",
        "hover:bg-elevated transition-colors"
      )}
    >
      <Icon
        size={16}
        className={cn(
          "shrink-0",
          isMissed ? "text-destructive" : "text-tertiary"
        )}
      />
      <div className="flex-1 min-w-0">
        <p
          className={cn(
            "text-sm font-medium truncate",
            isMissed ? "text-destructive" : "text-primary"
          )}
        >
          {call.remote_name || "Unknown"}
        </p>
        <p className="text-xs text-tertiary truncate">{call.remote_uri}</p>
      </div>
      <div className="text-right shrink-0 flex items-center gap-2">
        <div>
          <p className="text-xs text-secondary tabular-nums">{duration}</p>
          <p className="text-[10px] text-tertiary">{time}</p>
        </div>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDelete(call.id);
          }}
          className="p-1 rounded-md text-tertiary hover:text-destructive opacity-0 group-hover:opacity-100 transition-opacity"
          aria-label="Delete"
        >
          <Trash2 size={12} />
        </button>
      </div>
    </div>
  );
}

function formatTime(isoString: string): string {
  try {
    return new Date(isoString).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

function groupByDate(records: CallRecord[]): Record<string, CallRecord[]> {
  const groups: Record<string, CallRecord[]> = {};
  const today = new Date().toDateString();
  const yesterday = new Date(Date.now() - 86400_000).toDateString();

  for (const record of records) {
    let label: string;
    try {
      const dateStr = new Date(record.start_time).toDateString();
      if (dateStr === today) label = "Today";
      else if (dateStr === yesterday) label = "Yesterday";
      else label = dateStr;
    } catch {
      label = "Unknown";
    }

    if (!groups[label]) groups[label] = [];
    groups[label].push(record);
  }

  return groups;
}
