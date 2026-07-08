import { useState, useEffect, useCallback } from "react";
import { Mic, Play, Trash2, Download } from "lucide-react";
import { useServerStore } from "@/store/serverStore";
import { toast } from "@/components/ui/Toast";
import { paleFetch } from "@/lib/tauri";
import { Badge } from "@/components/ui/Badge";

interface RecordingEntry {
  id: string;
  call_id: string | null;
  caller_uri: string;
  callee_uri: string;
  duration_secs: number;
  file_id: string | null;
  recorded_by: string;
  conference_id?: string | null;
  transcript_segment_count?: number;
  legal_hold?: boolean;
  deleted_at?: string | null;
  deleted_by?: string | null;
  created_at: string;
}

interface TranscriptionJob {
  id: string;
  recording_id: string;
  status: "blocked" | "queued" | "processing" | "completed" | "failed";
  provider_configured: boolean;
  error?: string | null;
  transcript_segment_count: number;
  updated_at: string;
}

export function RecordingsView() {
  const { baseUrl, token, connected } = useServerStore();
  const [recordings, setRecordings] = useState<RecordingEntry[]>([]);
  const [jobsByRecording, setJobsByRecording] = useState<Record<string, TranscriptionJob[]>>({});
  const [loading, setLoading] = useState(true);
  const [playingId, setPlayingId] = useState<string | null>(null);

  const loadTranscriptionJobs = useCallback(async (recordingId: string) => {
    if (!baseUrl || !token) return;
    try {
      const res = await paleFetch(`${baseUrl.replace(/\/+$/, "")}/v1/recordings/${recordingId}/transcription`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const jobs = await res.json();
        setJobsByRecording((current) => ({ ...current, [recordingId]: jobs }));
      }
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  const load = useCallback(async () => {
    if (!baseUrl || !token) { setLoading(false); return; }
    try {
      const res = await paleFetch(`${baseUrl.replace(/\/+$/, "")}/v1/recordings`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const data = await res.json();
        setRecordings(data);
        data.slice(0, 20).forEach((recording: RecordingEntry) => loadTranscriptionJobs(recording.id));
      }
    } catch { /* ignore */ }
    setLoading(false);
  }, [baseUrl, token, loadTranscriptionJobs]);

  useEffect(() => { if (connected) load(); }, [connected, load]);

  const remove = async (id: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleFetch(`${baseUrl.replace(/\/+$/, "")}/v1/recordings/${id}`, {
        method: "DELETE",
        headers: { Authorization: `Bearer ${token}` },
      });
      setRecordings((prev) => prev.filter((r) => r.id !== id));
      toast({ type: "success", title: "Recording deleted" });
    } catch {
      toast({ type: "error", title: "Failed to delete" });
    }
  };

  const playAudio = (rec: RecordingEntry) => {
    if (!rec.file_id || !baseUrl) return;
    setPlayingId(rec.id);
    const audio = new Audio(`${baseUrl.replace(/\/+$/, "")}/v1/files/${rec.file_id}`);
    audio.onended = () => setPlayingId(null);
    audio.play().catch(() => setPlayingId(null));
  };

  const queueTranscription = async (recordingId: string) => {
    if (!baseUrl || !token) return;
    try {
      const res = await paleFetch(`${baseUrl.replace(/\/+$/, "")}/v1/recordings/${recordingId}/transcription`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify({ language: "en" }),
      });
      if (!res.ok) throw new Error("failed");
      await loadTranscriptionJobs(recordingId);
      toast({ type: "success", title: "Transcription queued" });
    } catch {
      toast({ type: "error", title: "Failed to queue transcription" });
    }
  };

  if (!connected) {
    return (
      <div className="flex flex-col items-center justify-center h-32 gap-2">
        <Mic size={32} className="text-tertiary" />
        <p className="text-sm text-tertiary">Connect to server to view recordings</p>
      </div>
    );
  }

  return (
    <div className="px-2">
      {loading ? (
        <p className="text-sm text-tertiary text-center py-8">Loading...</p>
      ) : recordings.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-32 gap-2">
          <Mic size={32} className="text-tertiary" />
          <p className="text-sm text-tertiary">No recordings</p>
        </div>
      ) : (
        recordings.map((rec) => (
          (() => {
            const jobs = jobsByRecording[rec.id] || [];
            const latestJob = jobs[0];
            return (
          <div
            key={rec.id}
            className="group flex items-center gap-3 px-2 py-2.5 rounded-lg hover:bg-elevated transition-colors"
          >
            <Mic size={16} className="text-tertiary" />
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium truncate">
                {rec.caller_uri} &rarr; {rec.callee_uri}
              </p>
              <p className="text-xs text-tertiary">
                {rec.duration_secs}s &middot;{" "}
                {new Date(rec.created_at).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}
              </p>
              <div className="mt-1 flex flex-wrap gap-1">
                {rec.legal_hold && <Badge variant="warning">Legal hold</Badge>}
                {rec.transcript_segment_count ? <Badge variant="accent">{rec.transcript_segment_count} transcript lines</Badge> : null}
                {rec.conference_id && <Badge variant="success">Meeting</Badge>}
                {latestJob && <Badge variant={latestJob.status === "completed" ? "success" : latestJob.status === "blocked" ? "warning" : "accent"}>{latestJob.status}</Badge>}
                {latestJob?.error && <Badge variant="warning">{latestJob.error.replace(/_/g, " ")}</Badge>}
              </div>
            </div>
            <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
              {rec.conference_id && (
                <button
                  onClick={() => queueTranscription(rec.id)}
                  className="p-1 rounded-md text-tertiary hover:text-accent"
                  title="Queue transcription"
                >
                  <Mic size={14} />
                </button>
              )}
              {rec.file_id && (
                <>
                  <button
                    onClick={() => playAudio(rec)}
                    disabled={playingId === rec.id}
                    className="p-1 rounded-md text-tertiary hover:text-accent"
                    title="Play"
                  >
                    <Play size={14} />
                  </button>
                  <a
                    href={`${baseUrl?.replace(/\/+$/, "")}/v1/files/${rec.file_id}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="p-1 rounded-md text-tertiary hover:text-accent"
                    title="Download"
                  >
                    <Download size={14} />
                  </a>
                </>
              )}
              <button onClick={() => remove(rec.id)} className="p-1 rounded-md text-tertiary hover:text-destructive" title="Delete">
                <Trash2 size={14} />
              </button>
            </div>
          </div>
            );
          })()
        ))
      )}
    </div>
  );
}
