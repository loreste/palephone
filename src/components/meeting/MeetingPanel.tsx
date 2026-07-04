import { useState, useEffect, useCallback, useRef } from "react";
import {
  Hand,
  DoorOpen,
  BarChart3,
  MessageCircleQuestion,
  LayoutGrid,
  Captions,
  Check,
  X,
  ChevronUp,
  Download,
  Lock,
  MessageCircle,
  Mic,
  MicOff,
  Pencil,
  PenTool,
  Shield,
  Smile,
  Sparkles,
  Square,
  Star,
  Type,
  Unlock,
  UserMinus,
  Users,
  Circle,
  Highlighter,
  Trash2,
} from "lucide-react";
import { cn } from "@/lib/cn";
import {
  useMeetingStore,
  type ConferenceLobby,
  type HandRaise,
  type MeetingPoll,
  type QaQuestion,
  type BreakoutSession,
  type ConferenceSummary,
  type ConferenceParticipant,
  type ConferenceAttendanceRecord,
  type GreenRoomState,
} from "@/store/meetingStore";
import { useServerStore } from "@/store/serverStore";
import { paleServerApi } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";

function err(title: string) { toast({ type: "error", title }); }

type MeetingTab = "people" | "lobby" | "hands" | "polls" | "qa" | "breakout" | "captions" | "reactions" | "chat" | "greenroom" | "annotate" | "whiteboard";

export function MeetingPanel({ conferenceId }: { conferenceId: string }) {
  const [tab, setTab] = useState<MeetingTab>("lobby");
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);

  const tabs: { id: MeetingTab; icon: typeof Hand; label: string }[] = [
    { id: "people", icon: Users, label: "People" },
    { id: "lobby", icon: DoorOpen, label: "Lobby" },
    { id: "hands", icon: Hand, label: "Hands" },
    { id: "reactions", icon: Smile, label: "React" },
    { id: "chat", icon: MessageCircle, label: "Chat" },
    { id: "greenroom", icon: Sparkles, label: "Green" },
    { id: "polls", icon: BarChart3, label: "Polls" },
    { id: "qa", icon: MessageCircleQuestion, label: "Q&A" },
    { id: "breakout", icon: LayoutGrid, label: "Rooms" },
    { id: "captions", icon: Captions, label: "Captions" },
    { id: "annotate", icon: PenTool, label: "Annotate" },
    { id: "whiteboard", icon: Pencil, label: "Board" },
  ];

  const handleTabKeyDown = (e: React.KeyboardEvent, tabId: MeetingTab) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      setTab(tabId);
    }
  };

  return (
    <div className="flex flex-col h-full border-l border-border-subtle w-[320px]">
      <div className="flex border-b border-border-subtle" role="tablist" aria-label="Meeting panel tabs">
        {tabs.map(({ id, icon: Icon, label }) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            aria-label={label}
            onClick={() => setTab(id)}
            onKeyDown={(e) => handleTabKeyDown(e, id)}
            className={cn(
              "flex-1 flex flex-col items-center gap-0.5 py-2 text-[10px]",
              tab === id ? "text-accent border-b-2 border-accent" : "text-secondary hover:text-primary"
            )}
          >
            <Icon size={16} aria-hidden="true" />
            {label}
          </button>
        ))}
      </div>
      <div className="flex-1 overflow-y-auto p-3">
        {tab === "people" && <ParticipantsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "lobby" && <LobbyPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "hands" && <HandsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "reactions" && <ReactionsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "chat" && <MeetingChatPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "greenroom" && <GreenRoomPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "polls" && <PollsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "qa" && <QaPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "breakout" && <BreakoutPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "captions" && <CaptionsPanel conferenceId={conferenceId} />}
        {tab === "annotate" && <AnnotationPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "whiteboard" && <WhiteboardPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
      </div>
    </div>
  );
}

// ── Participants Panel ─────────────────────────────────────────────

function ParticipantsPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const conference = useMeetingStore((s) => s.conferences[conferenceId]);
  const setConference = useMeetingStore((s) => s.setConference);
  const [attendance, setAttendance] = useState<ConferenceAttendanceRecord[]>([]);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const conferences = await paleServerApi<ConferenceSummary[]>(baseUrl, token, "/v1/conferences");
      const current = conferences.find((item) => item.id === conferenceId);
      if (current) setConference(current);
      const report = await paleServerApi<ConferenceAttendanceRecord[]>(baseUrl, token, `/v1/conferences/${conferenceId}/attendance`);
      setAttendance(report);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setConference]);

  useEffect(() => { load(); }, [load]);

  const updateParticipant = async (participant: ConferenceParticipant, body: Record<string, unknown>) => {
    if (!baseUrl || !token) return;
    try {
      const next = await paleServerApi<ConferenceSummary>(
        baseUrl,
        token,
        `/v1/conferences/${conferenceId}/participants/${participant.user_id}`,
        { method: "PUT", body }
      );
      setConference(next);
      await load();
    } catch {
      err("Only hosts and moderators can manage participants");
    }
  };

  const toggleLock = async () => {
    if (!baseUrl || !token || !conference) return;
    try {
      const next = await paleServerApi<ConferenceSummary>(
        baseUrl,
        token,
        `/v1/conferences/${conferenceId}/lock`,
        { method: "PUT", body: { locked: !conference.locked } }
      );
      setConference(next);
    } catch {
      err("Only hosts and moderators can lock meetings");
    }
  };

  const participants = conference?.participants ?? [];
  const active = participants.filter((participant) => !participant.removed);
  const removed = participants.filter((participant) => participant.removed);
  const downloadAttendance = () => {
    const blob = new Blob([JSON.stringify(attendance, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `attendance-${conferenceId}-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const downloadAttendanceCsv = async () => {
    if (!baseUrl || !token) return;
    try {
      const csv = await paleServerApi<string>(baseUrl, token, `/v1/conferences/${conferenceId}/attendance/export?format=csv`);
      const blob = new Blob([csv], { type: "text/csv" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `attendance-${conferenceId}-${new Date().toISOString().slice(0, 10)}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch { err("Failed to export CSV"); }
  };

  const setSpotlight = async (participantId: string | null) => {
    if (!baseUrl || !token) return;
    try {
      const next = await paleServerApi<ConferenceSummary>(
        baseUrl,
        token,
        `/v1/conferences/${conferenceId}/spotlight`,
        { method: "POST", body: { participant_id: participantId } }
      );
      setConference(next);
    } catch { err("Failed to set spotlight"); }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Participants ({active.length})</span>
        <div className="flex items-center gap-1">
          <button
            onClick={toggleLock}
            className={cn(
              "h-7 px-2 rounded-md text-xs inline-flex items-center gap-1",
              conference?.locked ? "bg-amber-500/15 text-amber-600" : "bg-hover text-secondary hover:text-primary"
            )}
            title={conference?.locked ? "Unlock meeting" : "Lock meeting"}
          >
            {conference?.locked ? <Lock size={12} /> : <Unlock size={12} />}
            {conference?.locked ? "Locked" : "Open"}
          </button>
          <button onClick={load} className="text-xs text-accent hover:underline">Refresh</button>
        </div>
      </div>
      {active.length === 0 ? (
        <p className="text-xs text-secondary text-center py-4">No active participants</p>
      ) : (
        <div className="space-y-2">
          {active.map((participant) => (
            <div key={participant.user_id} className="rounded-md bg-hover p-2 space-y-2">
              <div className="flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="text-sm truncate">{participant.sip_uri.replace(/^sip:/, "")}</div>
                  <div className="text-[11px] text-secondary capitalize flex items-center gap-1">
                    <Shield size={11} />
                    {participant.role}
                    {participant.muted ? " · muted" : ""}
                  </div>
                </div>
                <button
                  onClick={() => updateParticipant(participant, { muted: !participant.muted })}
                  className={cn(
                    "h-8 w-8 rounded-md inline-flex items-center justify-center hover:bg-elevated",
                    participant.muted ? "text-amber-500" : "text-secondary"
                  )}
                  title={participant.muted ? "Unmute participant" : "Mute participant"}
                >
                  {participant.muted ? <MicOff size={15} /> : <Mic size={15} />}
                </button>
              </div>
              <div className="grid grid-cols-[1fr_auto_auto] gap-2">
                <select
                  value={participant.role}
                  onChange={(event) => updateParticipant(participant, { role: event.target.value })}
                  className="h-8 rounded-md bg-base border border-border-default px-2 text-xs outline-none focus:border-border-focus"
                >
                  <option value="member">Member</option>
                  <option value="moderator">Moderator</option>
                  <option value="host">Host</option>
                </select>
                <button
                  onClick={() => setSpotlight(
                    conference?.spotlight_participant_id === participant.user_id ? null : participant.user_id
                  )}
                  className={cn(
                    "h-8 px-2 rounded-md text-xs inline-flex items-center gap-1",
                    conference?.spotlight_participant_id === participant.user_id
                      ? "bg-yellow-500/15 text-yellow-600"
                      : "bg-hover text-secondary hover:text-primary"
                  )}
                  title={conference?.spotlight_participant_id === participant.user_id ? "Remove spotlight" : "Spotlight"}
                >
                  <Star size={14} />
                </button>
                <button
                  onClick={() => updateParticipant(participant, { removed: true, removal_reason: "removed_by_moderator" })}
                  className="h-8 px-2 rounded-md text-xs text-destructive hover:bg-destructive/10 inline-flex items-center gap-1"
                >
                  <UserMinus size={14} />
                  Remove
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      {removed.length > 0 && (
        <div className="pt-2 border-t border-border-subtle space-y-1">
          <div className="text-xs text-tertiary">Removed</div>
          {removed.map((participant) => (
            <div key={participant.user_id} className="flex items-center justify-between gap-2 rounded bg-base px-2 py-1">
              <span className="text-xs text-secondary truncate">{participant.sip_uri.replace(/^sip:/, "")}</span>
              <button
                onClick={() => updateParticipant(participant, { removed: false, muted: false })}
                className="text-xs text-accent hover:underline"
              >
                Restore
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="pt-2 border-t border-border-subtle space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium">Attendance ({attendance.length})</span>
          <div className="flex gap-2">
            <button
              onClick={downloadAttendanceCsv}
              disabled={attendance.length === 0}
              className="text-xs text-accent hover:underline disabled:text-tertiary disabled:no-underline"
            >
              <Download size={12} className="inline mr-1" />
              CSV
            </button>
            <button
              onClick={downloadAttendance}
              disabled={attendance.length === 0}
              className="text-xs text-accent hover:underline disabled:text-tertiary disabled:no-underline"
            >
              <Download size={12} className="inline mr-1" />
              JSON
            </button>
          </div>
        </div>
        {attendance.length === 0 ? (
          <p className="text-xs text-secondary text-center py-2">No attendance records</p>
        ) : (
          <div className="space-y-1 max-h-48 overflow-y-auto">
            {attendance.slice(-8).reverse().map((record) => (
              <div key={record.id} className="rounded bg-base px-2 py-1">
                <div className="text-xs truncate">{record.sip_uri.replace(/^sip:/, "")}</div>
                <div className="text-[11px] text-secondary">
                  {record.left_at ? record.leave_reason ?? "left" : "in meeting"}
                  {record.duration_secs != null ? ` · ${Math.max(0, Math.round(record.duration_secs / 60))} min` : ""}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Lobby Panel ────────────────────────────────────────────────────

function LobbyPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const lobby = useMeetingStore((s) => s.lobby);
  const setLobby = useMeetingStore((s) => s.setLobby);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<ConferenceLobby>(baseUrl, token, `/v1/conferences/${conferenceId}/lobby`);
      setLobby(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setLobby]);

  useEffect(() => { load(); }, [load]);

  const toggleLobby = async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<ConferenceLobby>(baseUrl, token, `/v1/conferences/${conferenceId}/lobby`, {
        method: "PUT",
        body: { enabled: !lobby?.enabled },
      });
      setLobby(data);
    } catch { err("Failed to update lobby settings"); }
  };

  const admit = async (userId: string, allow: boolean) => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<ConferenceLobby>(baseUrl, token, `/v1/conferences/${conferenceId}/lobby/admit`, {
        method: "POST",
        body: { user_id: userId, admit: allow },
      });
      setLobby(data);
    } catch { err("Failed"); }
  };

  const admitAll = async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<ConferenceLobby>(baseUrl, token, `/v1/conferences/${conferenceId}/lobby/admit-all`, {
        method: "POST",
        body: {},
      });
      setLobby(data);
    } catch { err("Failed"); }
  };

  const waiting = lobby?.participants.filter((p) => p.state === "waiting") ?? [];

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Meeting Lobby</span>
        <button
          onClick={toggleLobby}
          className={cn(
            "text-xs px-2 py-1 rounded",
            lobby?.enabled ? "bg-accent text-white" : "bg-hover text-secondary"
          )}
        >
          {lobby?.enabled ? "Enabled" : "Disabled"}
        </button>
      </div>
      {waiting.length > 0 && (
        <>
          <button
            onClick={admitAll}
            className="w-full text-xs py-1.5 bg-accent/10 text-accent rounded hover:bg-accent/20"
          >
            Admit All ({waiting.length})
          </button>
          <div className="space-y-1">
            {waiting.map((p) => (
              <div key={p.user_id} className="flex items-center justify-between p-2 rounded bg-hover">
                <span className="text-sm truncate flex-1">{p.display_name || p.sip_uri}</span>
                <div className="flex gap-1">
                  <button onClick={() => admit(p.user_id, true)} className="p-1 hover:bg-green-500/20 rounded text-green-500">
                    <Check size={14} />
                  </button>
                  <button onClick={() => admit(p.user_id, false)} className="p-1 hover:bg-red-500/20 rounded text-red-500">
                    <X size={14} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
      {waiting.length === 0 && lobby?.enabled && (
        <p className="text-xs text-secondary text-center py-4">No one is waiting</p>
      )}
    </div>
  );
}

// ── Hands Panel ────────────────────────────────────────────────────

function HandsPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const hands = useMeetingStore((s) => s.raisedHands);
  const setHands = useMeetingStore((s) => s.setRaisedHands);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<HandRaise[]>(baseUrl, token, `/v1/conferences/${conferenceId}/hands`);
      setHands(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setHands]);

  useEffect(() => { load(); }, [load]);

  const lowerAll = async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<HandRaise[]>(baseUrl, token, `/v1/conferences/${conferenceId}/hands/lower-all`, {
        method: "POST",
        body: {},
      });
      setHands(data);
    } catch { err("Failed"); }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Raised Hands ({hands.length})</span>
        {hands.length > 0 && (
          <button onClick={lowerAll} className="text-xs text-accent hover:underline">
            Lower All
          </button>
        )}
      </div>
      {hands.length === 0 ? (
        <p className="text-xs text-secondary text-center py-4">No hands raised</p>
      ) : (
        <div className="space-y-1">
          {hands.map((h, i) => (
            <div key={h.user_id} className="flex items-center gap-2 p-2 rounded bg-hover">
              <span className="text-sm font-semibold text-accent">{i + 1}</span>
              <Hand size={14} className="text-yellow-500" />
              <span className="text-sm truncate">{h.sip_uri.replace(/^sip:/, "")}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Polls Panel ───────────────────────────────────────────────────

function PollsPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const polls = useMeetingStore((s) => s.polls);
  const setPolls = useMeetingStore((s) => s.setPolls);
  const [creating, setCreating] = useState(false);
  const [question, setQuestion] = useState("");
  const [options, setOptions] = useState(["", ""]);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<MeetingPoll[]>(baseUrl, token, `/v1/conferences/${conferenceId}/polls`);
      setPolls(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setPolls]);

  useEffect(() => { load(); }, [load]);

  const createPoll = async () => {
    if (!baseUrl || !token || !question) return;
    const opts = options.filter(Boolean);
    if (opts.length < 2) return;
    try {
      await paleServerApi<MeetingPoll>(baseUrl, token, `/v1/conferences/${conferenceId}/polls`, {
        method: "POST",
        body: { question, options: opts },
      });
      setCreating(false);
      setQuestion("");
      setOptions(["", ""]);
      load();
    } catch { err("Failed to create poll"); }
  };

  const launchPoll = async (pollId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<MeetingPoll>(baseUrl, token, `/v1/polls/${pollId}/launch`, { method: "POST", body: {} });
      load();
    } catch { err("Failed"); }
  };

  const closePoll = async (pollId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<MeetingPoll>(baseUrl, token, `/v1/polls/${pollId}/close`, { method: "POST", body: {} });
      load();
    } catch { err("Failed"); }
  };

  const vote = async (pollId: string, optionId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<MeetingPoll>(baseUrl, token, `/v1/polls/${pollId}/vote`, {
        method: "POST",
        body: { option_ids: [optionId] },
      });
      load();
    } catch { err("Failed"); }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Polls</span>
        <button onClick={() => setCreating(!creating)} className="text-xs text-accent hover:underline">
          {creating ? "Cancel" : "+ New Poll"}
        </button>
      </div>

      {creating && (
        <div className="space-y-2 p-2 border border-border-subtle rounded">
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Question"
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
          />
          {options.map((opt, i) => (
            <input
              key={i}
              className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
              placeholder={`Option ${i + 1}`}
              value={opt}
              onChange={(e) => {
                const next = [...options];
                next[i] = e.target.value;
                setOptions(next);
              }}
            />
          ))}
          <div className="flex gap-2">
            <button
              onClick={() => setOptions([...options, ""])}
              className="text-xs text-accent hover:underline"
            >
              + Add option
            </button>
          </div>
          <button
            onClick={createPoll}
            className="w-full py-1.5 bg-accent text-white rounded text-sm hover:bg-accent/90"
          >
            Create Poll
          </button>
        </div>
      )}

      {polls.map((poll) => {
        const totalVotes = poll.options.reduce((sum, o) => sum + o.votes.length, 0);
        return (
          <div key={poll.id} className="p-2 border border-border-subtle rounded space-y-2">
            <div className="flex items-start justify-between">
              <span className="text-sm font-medium">{poll.question}</span>
              <span className={cn(
                "text-[10px] px-1.5 py-0.5 rounded",
                poll.status === "active" ? "bg-green-500/20 text-green-500" :
                poll.status === "closed" ? "bg-red-500/20 text-red-500" :
                "bg-yellow-500/20 text-yellow-500"
              )}>
                {poll.status}
              </span>
            </div>
            {poll.options.map((opt) => {
              const pct = totalVotes > 0 ? Math.round((opt.votes.length / totalVotes) * 100) : 0;
              return (
                <button
                  key={opt.id}
                  onClick={() => poll.status === "active" && vote(poll.id, opt.id)}
                  disabled={poll.status !== "active"}
                  className="w-full text-left relative"
                >
                  <div className="flex items-center justify-between text-xs p-1.5 rounded bg-hover relative z-10">
                    <span>{opt.text}</span>
                    <span className="text-secondary">{opt.votes.length} ({pct}%)</span>
                  </div>
                  <div
                    className="absolute inset-0 bg-accent/10 rounded"
                    style={{ width: `${pct}%` }}
                  />
                </button>
              );
            })}
            {poll.status === "draft" && (
              <button onClick={() => launchPoll(poll.id)} className="w-full py-1 text-xs bg-accent text-white rounded">
                Launch Poll
              </button>
            )}
            {poll.status === "active" && (
              <button onClick={() => closePoll(poll.id)} className="w-full py-1 text-xs bg-red-500 text-white rounded">
                Close Poll
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ── Q&A Panel ─────────────────────────────────────────────────────

function QaPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const questions = useMeetingStore((s) => s.questions);
  const setQuestions = useMeetingStore((s) => s.setQuestions);
  const [text, setText] = useState("");

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<QaQuestion[]>(baseUrl, token, `/v1/conferences/${conferenceId}/questions`);
      setQuestions(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setQuestions]);

  useEffect(() => { load(); }, [load]);

  const ask = async () => {
    if (!baseUrl || !token || !text.trim()) return;
    try {
      await paleServerApi<QaQuestion>(baseUrl, token, `/v1/conferences/${conferenceId}/questions`, {
        method: "POST",
        body: { text: text.trim() },
      });
      setText("");
      load();
    } catch { err("Failed"); }
  };

  const upvote = async (qId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<QaQuestion>(baseUrl, token, `/v1/questions/${qId}/upvote`, { method: "POST", body: {} });
      load();
    } catch { /* ignore */ }
  };

  const answer = async (qId: string) => {
    const ans = prompt("Enter answer:");
    if (!ans || !baseUrl || !token) return;
    try {
      await paleServerApi<QaQuestion>(baseUrl, token, `/v1/questions/${qId}/answer`, {
        method: "POST",
        body: { answer: ans },
      });
      load();
    } catch { err("Failed"); }
  };

  const sorted = [...questions].sort((a, b) => b.upvotes.length - a.upvotes.length);

  return (
    <div className="space-y-3">
      <span className="text-sm font-medium">Q&A ({questions.length})</span>
      <div className="flex gap-2">
        <input
          className="flex-1 text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
          placeholder="Ask a question..."
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && ask()}
        />
        <button onClick={ask} className="px-3 py-1.5 bg-accent text-white rounded text-sm">
          Ask
        </button>
      </div>
      {sorted.map((q) => (
        <div key={q.id} className={cn("p-2 rounded border", q.answered ? "border-green-500/30 bg-green-500/5" : "border-border-subtle")}>
          <div className="flex gap-2">
            <button onClick={() => upvote(q.id)} className="flex flex-col items-center text-secondary hover:text-accent">
              <ChevronUp size={16} />
              <span className="text-xs font-medium">{q.upvotes.length}</span>
            </button>
            <div className="flex-1 min-w-0">
              <p className="text-sm">{q.text}</p>
              <p className="text-[10px] text-secondary mt-1">{q.asked_by.replace(/^sip:/, "")}</p>
              {q.answered && q.answer && (
                <div className="mt-1.5 p-1.5 bg-green-500/10 rounded text-xs">
                  <span className="font-medium">A:</span> {q.answer}
                </div>
              )}
            </div>
            {!q.answered && (
              <button onClick={() => answer(q.id)} className="text-xs text-accent hover:underline self-start">
                Answer
              </button>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Breakout Panel ────────────────────────────────────────────────

function BreakoutPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const breakouts = useMeetingStore((s) => s.breakouts);
  const setBreakouts = useMeetingStore((s) => s.setBreakouts);
  const [creating, setCreating] = useState(false);
  const [roomCount, setRoomCount] = useState(2);
  const [duration, setDuration] = useState(10);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<BreakoutSession[]>(baseUrl, token, `/v1/conferences/${conferenceId}/breakouts`);
      setBreakouts(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setBreakouts]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!baseUrl || !token) return;
    const rooms = Array.from({ length: roomCount }, (_, i) => ({
      name: `Room ${i + 1}`,
      participants: [],
    }));
    try {
      await paleServerApi<BreakoutSession>(baseUrl, token, `/v1/conferences/${conferenceId}/breakouts`, {
        method: "POST",
        body: { rooms, duration_secs: duration * 60 },
      });
      setCreating(false);
      load();
    } catch { err("Failed"); }
  };

  const start = async (sessionId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<BreakoutSession>(baseUrl, token, `/v1/breakouts/${sessionId}/start`, { method: "POST", body: {} });
      load();
    } catch { err("Failed"); }
  };

  const close = async (sessionId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<BreakoutSession>(baseUrl, token, `/v1/breakouts/${sessionId}/close`, { method: "POST", body: {} });
      load();
    } catch { err("Failed"); }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Breakout Rooms</span>
        <button onClick={() => setCreating(!creating)} className="text-xs text-accent hover:underline">
          {creating ? "Cancel" : "+ Create"}
        </button>
      </div>

      {creating && (
        <div className="space-y-2 p-2 border border-border-subtle rounded">
          <label className="text-xs text-secondary">Number of rooms</label>
          <input
            type="number"
            min={2}
            max={50}
            value={roomCount}
            onChange={(e) => setRoomCount(Number(e.target.value))}
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
          />
          <label className="text-xs text-secondary">Duration (minutes)</label>
          <input
            type="number"
            min={1}
            value={duration}
            onChange={(e) => setDuration(Number(e.target.value))}
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
          />
          <button onClick={create} className="w-full py-1.5 bg-accent text-white rounded text-sm">
            Create Rooms
          </button>
        </div>
      )}

      {breakouts.map((session) => (
        <div key={session.id} className="p-2 border border-border-subtle rounded space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium">
              {session.rooms.length} rooms
            </span>
            <span className={cn(
              "text-[10px] px-1.5 py-0.5 rounded",
              session.status === "active" ? "bg-green-500/20 text-green-500" :
              session.status === "closed" ? "bg-red-500/20 text-red-500" :
              "bg-yellow-500/20 text-yellow-500"
            )}>
              {session.status}
            </span>
          </div>
          {session.duration_secs && (
            <span className="text-xs text-secondary">{Math.round(session.duration_secs / 60)} min</span>
          )}
          <div className="grid grid-cols-2 gap-1">
            {session.rooms.map((room) => (
              <div key={room.id} className="p-1.5 bg-hover rounded text-xs">
                <div className="font-medium">{room.name}</div>
                <div className="flex items-center gap-1 text-secondary">
                  <Users size={10} /> {room.participants.length}
                </div>
              </div>
            ))}
          </div>
          {session.status === "pending" && (
            <button onClick={() => start(session.id)} className="w-full py-1 text-xs bg-accent text-white rounded">
              Open Rooms
            </button>
          )}
          {session.status === "active" && (
            <button onClick={() => close(session.id)} className="w-full py-1 text-xs bg-red-500 text-white rounded">
              Close All Rooms
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

// ── Reactions Panel ───────────────────────────────────────────────

const REACTION_EMOJIS = [
  { emoji: "\u{1F44D}", label: "Thumbs up" },
  { emoji: "\u{1F44F}", label: "Clap" },
  { emoji: "\u{2764}\u{FE0F}", label: "Heart" },
  { emoji: "\u{1F602}", label: "Laugh" },
  { emoji: "\u{1F914}", label: "Thinking" },
  { emoji: "\u{1F389}", label: "Party" },
  { emoji: "\u{1F525}", label: "Fire" },
  { emoji: "\u{1F680}", label: "Rocket" },
];

function ReactionsPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const reactions = useMeetingStore((s) => s.reactions);
  const conferenceReactions = reactions.filter(
    (_r) => true // All reactions visible since SSE scopes to conference
  );

  const sendReaction = async (emoji: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/reactions`, {
        method: "POST",
        body: { emoji },
      });
    } catch { err("Failed to send reaction"); }
  };

  return (
    <div className="space-y-3">
      <span className="text-sm font-medium">Reactions</span>
      <div className="grid grid-cols-4 gap-2">
        {REACTION_EMOJIS.map(({ emoji, label }) => (
          <button
            key={emoji}
            onClick={() => sendReaction(emoji)}
            className="h-12 flex items-center justify-center rounded-lg bg-hover hover:bg-elevated text-2xl transition-transform hover:scale-110"
            title={label}
          >
            {emoji}
          </button>
        ))}
      </div>
      {conferenceReactions.length > 0 && (
        <div className="pt-2 border-t border-border-subtle space-y-1">
          <span className="text-xs text-secondary">Recent reactions</span>
          <div className="flex flex-wrap gap-1">
            {conferenceReactions.slice(-12).reverse().map((r, i) => (
              <div
                key={`${r.timestamp}-${i}`}
                className="inline-flex items-center gap-1 px-2 py-1 bg-hover rounded-full text-xs animate-bounce"
                style={{ animationDuration: "0.6s", animationIterationCount: 1 }}
              >
                <span className="text-base">{r.emoji}</span>
                <span className="text-secondary truncate max-w-[60px]">{r.user_name}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Meeting Chat Panel ───────────────────────────────────────────

function MeetingChatPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [chatRoomId, setChatRoomId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Array<{ id: string; sender_uri: string; body: string; created_at: string }>>([]);
  const [text, setText] = useState("");

  const loadChatRoom = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<{ chat_room_id: string }>(
        baseUrl, token, `/v1/conferences/${conferenceId}/chat-room`
      );
      setChatRoomId(data.chat_room_id);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId]);

  const loadMessages = useCallback(async () => {
    if (!baseUrl || !token || !chatRoomId) return;
    try {
      const msgs = await paleServerApi<Array<{ id: string; sender_uri: string; body: string; created_at: string }>>(
        baseUrl, token, `/v1/rooms/${chatRoomId}/messages`
      );
      setMessages(msgs);
    } catch { /* ignore */ }
  }, [baseUrl, token, chatRoomId]);

  useEffect(() => { loadChatRoom(); }, [loadChatRoom]);
  useEffect(() => { loadMessages(); const iv = setInterval(loadMessages, 5000); return () => clearInterval(iv); }, [loadMessages]);

  const send = async () => {
    if (!baseUrl || !token || !chatRoomId || !text.trim()) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/rooms/${chatRoomId}/messages`, {
        method: "POST",
        body: { body: text.trim() },
      });
      setText("");
      loadMessages();
    } catch { err("Failed to send"); }
  };

  return (
    <div className="flex flex-col h-full space-y-2">
      <span className="text-sm font-medium">Meeting Chat</span>
      {!chatRoomId ? (
        <p className="text-xs text-secondary text-center py-4">Loading chat room...</p>
      ) : (
        <>
          <div className="flex-1 overflow-y-auto space-y-1 max-h-[400px]">
            {messages.length === 0 ? (
              <p className="text-xs text-secondary text-center py-4">No messages yet</p>
            ) : (
              messages.map((msg) => (
                <div key={msg.id} className="rounded bg-hover px-2 py-1">
                  <div className="text-[11px] text-accent">{msg.sender_uri.replace(/^sip:/, "")}</div>
                  <div className="text-xs">{msg.body}</div>
                </div>
              ))
            )}
          </div>
          <div className="flex gap-2">
            <input
              className="flex-1 text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
              placeholder="Type a message..."
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && send()}
            />
            <button onClick={send} className="px-3 py-1.5 bg-accent text-white rounded text-sm">
              Send
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// ── Green Room Panel ─────────────────────────────────────────────

function GreenRoomPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const greenRoom = useMeetingStore((s) => s.greenRoom);
  const setGreenRoom = useMeetingStore((s) => s.setGreenRoom);
  const conference = useMeetingStore((s) => s.conferences[conferenceId]);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<GreenRoomState>(baseUrl, token, `/v1/conferences/${conferenceId}/green-room`);
      setGreenRoom(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId, setGreenRoom]);

  useEffect(() => { load(); }, [load]);

  const toggleEnabled = async () => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/green-room`, {
        method: "PUT",
        body: { enabled: !conference?.green_room_enabled },
      });
      load();
    } catch { err("Failed to update green room"); }
  };

  const markReady = async (userId: string) => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<GreenRoomState>(baseUrl, token, `/v1/conferences/${conferenceId}/green-room/ready`, {
        method: "POST",
        body: { user_id: userId },
      });
      setGreenRoom(data);
    } catch { err("Failed"); }
  };

  const participants = greenRoom?.participants ?? [];
  const readyCount = participants.filter((p) => p.ready).length;

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Green Room</span>
        <button
          onClick={toggleEnabled}
          className={cn(
            "text-xs px-2 py-1 rounded",
            conference?.green_room_enabled ? "bg-accent text-white" : "bg-hover text-secondary"
          )}
        >
          {conference?.green_room_enabled ? "Enabled" : "Disabled"}
        </button>
      </div>
      {conference?.green_room_enabled && (
        <>
          <div className="text-xs text-secondary">
            {readyCount}/{participants.length} presenters ready
          </div>
          {participants.length === 0 ? (
            <p className="text-xs text-secondary text-center py-4">No presenters in green room</p>
          ) : (
            <div className="space-y-1">
              {participants.map((p) => (
                <div key={p.user_id} className="flex items-center justify-between p-2 rounded bg-hover">
                  <div>
                    <span className="text-sm truncate">{p.sip_uri.replace(/^sip:/, "")}</span>
                    <span className={cn(
                      "ml-2 text-[10px] px-1.5 py-0.5 rounded",
                      p.ready ? "bg-green-500/20 text-green-500" : "bg-yellow-500/20 text-yellow-500"
                    )}>
                      {p.ready ? "Ready" : "Not ready"}
                    </span>
                  </div>
                  {!p.ready && (
                    <button
                      onClick={() => markReady(p.user_id)}
                      className="text-xs text-accent hover:underline"
                    >
                      Mark Ready
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

// ── Captions Panel ────────────────────────────────────────────────

function CaptionsPanel({ conferenceId }: { conferenceId: string }) {
  const captions = useMeetingStore((s) => s.captions);
  const enabled = useMeetingStore((s) => s.captionsEnabled);
  const setEnabled = useMeetingStore((s) => s.setCaptionsEnabled);

  const conferenceCaptions = captions.filter((c) => c.conference_id === conferenceId);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Live Captions</span>
        <button
          onClick={() => setEnabled(!enabled)}
          className={cn(
            "text-xs px-2 py-1 rounded",
            enabled ? "bg-accent text-white" : "bg-hover text-secondary"
          )}
        >
          {enabled ? "On" : "Off"}
        </button>
      </div>
      {enabled && (
        <div className="space-y-1 max-h-[400px] overflow-y-auto">
          {conferenceCaptions.length === 0 ? (
            <p className="text-xs text-secondary text-center py-4">Waiting for captions...</p>
          ) : (
            conferenceCaptions.map((c) => (
              <div key={c.id} className="text-xs">
                <span className="font-medium text-accent">
                  {c.speaker_name || c.speaker_uri.replace(/^sip:/, "")}:
                </span>{" "}
                <span className={cn(!c.is_final && "italic text-secondary")}>{c.text}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}

// ── Screen Share Annotation Panel ────────────────────────────────

interface AnnotationItem {
  id: string;
  conference_id: string;
  annotation_type: string;
  data: { x: number; y: number; width: number; height: number; color: string; text?: string };
  author_uri: string;
  created_at: string;
}

function AnnotationPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [annotations, setAnnotations] = useState<AnnotationItem[]>([]);
  const [tool, setTool] = useState<"draw" | "text" | "highlight">("draw");
  const [color, setColor] = useState("#ff0000");
  const svgRef = useRef<SVGSVGElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [start, setStart] = useState<{ x: number; y: number } | null>(null);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<AnnotationItem[]>(baseUrl, token, `/v1/conferences/${conferenceId}/annotations`);
      setAnnotations(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);

  const addAnnotation = async (x: number, y: number, width: number, height: number) => {
    if (!baseUrl || !token) return;
    try {
      const ann = await paleServerApi<AnnotationItem>(baseUrl, token, `/v1/conferences/${conferenceId}/annotations`, {
        method: "POST",
        body: {
          type: tool,
          data: { x, y, width, height, color, text: tool === "text" ? "Text" : undefined },
        },
      });
      setAnnotations((prev) => [...prev, ann]);
    } catch { err("Failed to add annotation"); }
  };

  const clearAll = async () => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/annotations`, { method: "DELETE" });
      setAnnotations([]);
    } catch { err("Failed to clear annotations"); }
  };

  const handleMouseDown = (e: React.MouseEvent<SVGSVGElement>) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    setIsDrawing(true);
    setStart({ x: e.clientX - rect.left, y: e.clientY - rect.top });
  };

  const handleMouseUp = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!isDrawing || !start) return;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const endX = e.clientX - rect.left;
    const endY = e.clientY - rect.top;
    const x = Math.min(start.x, endX);
    const y = Math.min(start.y, endY);
    const width = Math.abs(endX - start.x);
    const height = Math.abs(endY - start.y);
    if (width > 2 || height > 2) addAnnotation(x, y, width, height);
    setIsDrawing(false);
    setStart(null);
  };

  const tools: { id: "draw" | "text" | "highlight"; icon: typeof PenTool; label: string }[] = [
    { id: "draw", icon: PenTool, label: "Pen" },
    { id: "text", icon: Type, label: "Text" },
    { id: "highlight", icon: Highlighter, label: "Highlight" },
  ];

  const colors = ["#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff", "#ffffff"];

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-1 flex-wrap">
        {tools.map(({ id, icon: Icon, label }) => (
          <button key={id} onClick={() => setTool(id)} title={label}
            className={cn("p-1.5 rounded", tool === id ? "bg-accent text-white" : "text-secondary hover:text-primary")}>
            <Icon size={14} />
          </button>
        ))}
        <button onClick={clearAll} title="Clear all" className="p-1.5 rounded text-secondary hover:text-destructive ml-auto">
          <Trash2 size={14} />
        </button>
      </div>
      <div className="flex gap-1">
        {colors.map((c) => (
          <button key={c} onClick={() => setColor(c)}
            className={cn("w-5 h-5 rounded-full border-2", color === c ? "border-primary" : "border-transparent")}
            style={{ backgroundColor: c }} />
        ))}
      </div>
      <div className="border border-border-subtle rounded bg-surface-secondary relative" style={{ height: 200 }}>
        <svg ref={svgRef} className="w-full h-full cursor-crosshair"
          onMouseDown={handleMouseDown} onMouseUp={handleMouseUp}>
          {annotations.map((ann) => {
            const d = ann.data;
            if (ann.annotation_type === "highlight") {
              return <rect key={ann.id} x={d.x} y={d.y} width={d.width} height={d.height} fill={d.color} opacity={0.3} />;
            }
            if (ann.annotation_type === "text") {
              return <text key={ann.id} x={d.x} y={d.y + 14} fill={d.color} fontSize={14}>{d.text}</text>;
            }
            return <rect key={ann.id} x={d.x} y={d.y} width={d.width} height={d.height} stroke={d.color} strokeWidth={2} fill="none" />;
          })}
        </svg>
      </div>
      <p className="text-[10px] text-tertiary">{annotations.length} annotation(s)</p>
    </div>
  );
}

// ── Whiteboard Panel ─────────────────────────────────────────────

interface WhiteboardData {
  id: string;
  conference_id: string;
  name: string;
  elements: any[];
  created_at: string;
  updated_at: string;
}

type WbTool = "freehand" | "rectangle" | "circle" | "text";

function WhiteboardPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [whiteboard, setWhiteboard] = useState<WhiteboardData | null>(null);
  const [wbTool, setWbTool] = useState<WbTool>("freehand");
  const [wbColor, setWbColor] = useState("#000000");
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [drawing, setDrawing] = useState(false);
  const [drawStart, setDrawStart] = useState<{ x: number; y: number } | null>(null);
  const [freehandPoints, setFreehandPoints] = useState<{ x: number; y: number }[]>([]);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const wb = await paleServerApi<WhiteboardData>(baseUrl, token, `/v1/conferences/${conferenceId}/whiteboard`);
      setWhiteboard(wb);
    } catch {
      // Create if not found
      try {
        const wb = await paleServerApi<WhiteboardData>(baseUrl, token, `/v1/conferences/${conferenceId}/whiteboard`, {
          method: "POST", body: { name: "Meeting Whiteboard" },
        });
        setWhiteboard(wb);
      } catch { /* ignore */ }
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);

  // Re-render canvas when whiteboard changes
  useEffect(() => {
    if (!whiteboard || !canvasRef.current) return;
    const ctx = canvasRef.current.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
    for (const el of whiteboard.elements) {
      ctx.strokeStyle = el.color || "#000";
      ctx.fillStyle = el.color || "#000";
      ctx.lineWidth = 2;
      if (el.type === "freehand" && el.points) {
        ctx.beginPath();
        el.points.forEach((p: { x: number; y: number }, i: number) => {
          if (i === 0) ctx.moveTo(p.x, p.y);
          else ctx.lineTo(p.x, p.y);
        });
        ctx.stroke();
      } else if (el.type === "rectangle") {
        ctx.strokeRect(el.x, el.y, el.width, el.height);
      } else if (el.type === "circle") {
        ctx.beginPath();
        ctx.ellipse(el.x + el.width / 2, el.y + el.height / 2, Math.abs(el.width) / 2, Math.abs(el.height) / 2, 0, 0, Math.PI * 2);
        ctx.stroke();
      } else if (el.type === "text") {
        ctx.font = "14px sans-serif";
        ctx.fillText(el.text || "Text", el.x, el.y);
      }
    }
  }, [whiteboard]);

  const addElement = async (element: any) => {
    if (!baseUrl || !token) return;
    try {
      const wb = await paleServerApi<WhiteboardData>(baseUrl, token, `/v1/conferences/${conferenceId}/whiteboard/elements`, {
        method: "POST", body: { element },
      });
      setWhiteboard(wb);
    } catch { err("Failed to add element"); }
  };

  const handleCanvasMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    setDrawing(true);
    setDrawStart({ x, y });
    if (wbTool === "freehand") setFreehandPoints([{ x, y }]);
  };

  const handleCanvasMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!drawing || wbTool !== "freehand") return;
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    setFreehandPoints((prev) => [...prev, { x: e.clientX - rect.left, y: e.clientY - rect.top }]);
  };

  const handleCanvasMouseUp = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!drawing || !drawStart) return;
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const endX = e.clientX - rect.left;
    const endY = e.clientY - rect.top;
    setDrawing(false);

    if (wbTool === "freehand" && freehandPoints.length > 1) {
      addElement({ type: "freehand", points: freehandPoints, color: wbColor });
      setFreehandPoints([]);
    } else if (wbTool === "rectangle") {
      addElement({ type: "rectangle", x: Math.min(drawStart.x, endX), y: Math.min(drawStart.y, endY), width: Math.abs(endX - drawStart.x), height: Math.abs(endY - drawStart.y), color: wbColor });
    } else if (wbTool === "circle") {
      addElement({ type: "circle", x: Math.min(drawStart.x, endX), y: Math.min(drawStart.y, endY), width: Math.abs(endX - drawStart.x), height: Math.abs(endY - drawStart.y), color: wbColor });
    } else if (wbTool === "text") {
      const text = prompt("Enter text:");
      if (text) addElement({ type: "text", x: drawStart.x, y: drawStart.y, text, color: wbColor });
    }
    setDrawStart(null);
  };

  const wbTools: { id: WbTool; icon: typeof PenTool; label: string }[] = [
    { id: "freehand", icon: PenTool, label: "Freehand" },
    { id: "rectangle", icon: Square, label: "Rectangle" },
    { id: "circle", icon: Circle, label: "Circle" },
    { id: "text", icon: Type, label: "Text" },
  ];

  const wbColors = ["#000000", "#ff0000", "#00aa00", "#0000ff", "#ff8800", "#aa00ff"];

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-1 flex-wrap">
        {wbTools.map(({ id, icon: Icon, label }) => (
          <button key={id} onClick={() => setWbTool(id)} title={label}
            className={cn("p-1.5 rounded", wbTool === id ? "bg-accent text-white" : "text-secondary hover:text-primary")}>
            <Icon size={14} />
          </button>
        ))}
      </div>
      <div className="flex gap-1">
        {wbColors.map((c) => (
          <button key={c} onClick={() => setWbColor(c)}
            className={cn("w-5 h-5 rounded-full border-2", wbColor === c ? "border-primary" : "border-transparent")}
            style={{ backgroundColor: c }} />
        ))}
      </div>
      <div className="border border-border-subtle rounded bg-white relative" style={{ height: 250 }}>
        <canvas ref={canvasRef} width={280} height={250} className="cursor-crosshair"
          onMouseDown={handleCanvasMouseDown} onMouseMove={handleCanvasMouseMove} onMouseUp={handleCanvasMouseUp} />
      </div>
      <p className="text-[10px] text-tertiary">{whiteboard?.elements.length ?? 0} element(s)</p>
    </div>
  );
}
