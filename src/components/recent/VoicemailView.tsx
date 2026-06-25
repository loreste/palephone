import { useState, useEffect, useCallback } from "react";
import { Voicemail, Play, Trash2, Check } from "lucide-react";
import { cn } from "@/lib/cn";
import { useServerStore } from "@/store/serverStore";
import { toast } from "@/components/ui/Toast";

interface VoicemailEntry {
  id: string;
  callee_uri: string;
  caller_uri: string;
  caller_name: string;
  duration_secs: number;
  file_id: string | null;
  listened: boolean;
  created_at: string;
}

export function VoicemailView() {
  const { baseUrl, token, connected } = useServerStore();
  const [voicemails, setVoicemails] = useState<VoicemailEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [playingId, setPlayingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!baseUrl || !token) { setLoading(false); return; }
    try {
      const res = await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/voicemail`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!res.ok) throw new Error(res.statusText);
      setVoicemails(await res.json());
    } catch { setVoicemails([]); }
    setLoading(false);
  }, [baseUrl, token]);

  useEffect(() => { if (connected) load(); }, [connected, load]);

  const markListened = async (id: string) => {
    if (!baseUrl || !token) return;
    try {
      await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/voicemail/${id}/listen`, {
        method: "PUT",
        headers: { Authorization: `Bearer ${token}` },
      });
      setVoicemails((prev) => prev.map((v) => (v.id === id ? { ...v, listened: true } : v)));
    } catch {
      toast({ type: "error", title: "Failed to mark as listened" });
    }
  };

  const remove = async (id: string) => {
    if (!baseUrl || !token) return;
    try {
      await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/voicemail/${id}`, {
        method: "DELETE",
        headers: { Authorization: `Bearer ${token}` },
      });
      setVoicemails((prev) => prev.filter((v) => v.id !== id));
      toast({ type: "success", title: "Voicemail deleted" });
    } catch {
      toast({ type: "error", title: "Failed to delete" });
    }
  };

  const playAudio = (vm: VoicemailEntry) => {
    if (!vm.file_id || !baseUrl) return;
    setPlayingId(vm.id);
    const audio = new Audio(`${baseUrl.replace(/\/+$/, "")}/v1/files/${vm.file_id}`);
    audio.onended = () => {
      setPlayingId(null);
      if (!vm.listened) markListened(vm.id);
    };
    audio.play().catch(() => setPlayingId(null));
  };

  if (!connected) {
    return (
      <div className="flex flex-col items-center justify-center h-32 gap-2">
        <Voicemail size={32} className="text-tertiary" />
        <p className="text-sm text-tertiary">Connect to server to view voicemails</p>
      </div>
    );
  }

  const unlistened = voicemails.filter((v) => !v.listened).length;

  return (
    <div className="px-2">
      {unlistened > 0 && (
        <p className="px-2 py-1 text-xs text-accent font-medium">{unlistened} new voicemail{unlistened > 1 ? "s" : ""}</p>
      )}
      {loading ? (
        <p className="text-sm text-tertiary text-center py-8">Loading...</p>
      ) : voicemails.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-32 gap-2">
          <Voicemail size={32} className="text-tertiary" />
          <p className="text-sm text-tertiary">No voicemails</p>
        </div>
      ) : (
        voicemails.map((vm) => (
          <div
            key={vm.id}
            className={cn(
              "group flex items-center gap-3 px-2 py-2.5 rounded-lg hover:bg-elevated transition-colors",
              !vm.listened && "bg-accent/5"
            )}
          >
            <Voicemail size={16} className={vm.listened ? "text-tertiary" : "text-accent"} />
            <div className="flex-1 min-w-0">
              <p className={cn("text-sm font-medium truncate", !vm.listened && "text-accent")}>
                {vm.caller_name || vm.caller_uri}
              </p>
              <p className="text-xs text-tertiary">
                {vm.duration_secs}s &middot;{" "}
                {new Date(vm.created_at).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}
              </p>
            </div>
            <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
              {vm.file_id && (
                <button
                  onClick={() => playAudio(vm)}
                  disabled={playingId === vm.id}
                  className="p-1 rounded-md text-tertiary hover:text-accent"
                  title="Play"
                >
                  <Play size={14} />
                </button>
              )}
              {!vm.listened && (
                <button onClick={() => markListened(vm.id)} className="p-1 rounded-md text-tertiary hover:text-success" title="Mark listened">
                  <Check size={14} />
                </button>
              )}
              <button onClick={() => remove(vm.id)} className="p-1 rounded-md text-tertiary hover:text-destructive" title="Delete">
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        ))
      )}
    </div>
  );
}
