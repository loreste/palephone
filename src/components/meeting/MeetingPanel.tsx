import { useState, useEffect, useCallback } from "react";
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
  Shield,
  Smile,
  Sparkles,
  Star,
  Unlock,
  UserMinus,
  Users,
  ClipboardList,
  Globe,
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

type MeetingTab = "people" | "lobby" | "hands" | "polls" | "qa" | "breakout" | "captions" | "reactions" | "chat" | "greenroom" | "registration";

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
    { id: "registration", icon: ClipboardList, label: "Register" },
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
        {tab === "captions" && <CaptionsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "registration" && <RegistrationPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
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

const CAPTION_LANGUAGES = [
  { code: "en", label: "English" },
  { code: "es", label: "Spanish" },
  { code: "fr", label: "French" },
  { code: "de", label: "German" },
  { code: "pt", label: "Portuguese" },
  { code: "zh", label: "Chinese" },
  { code: "ja", label: "Japanese" },
  { code: "ko", label: "Korean" },
  { code: "ar", label: "Arabic" },
  { code: "hi", label: "Hindi" },
];

function CaptionsPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const captions = useMeetingStore((s) => s.captions);
  const enabled = useMeetingStore((s) => s.captionsEnabled);
  const setEnabled = useMeetingStore((s) => s.setCaptionsEnabled);
  const [captionLang, setCaptionLang] = useState("en");

  const conferenceCaptions = captions.filter((c) => c.conference_id === conferenceId);

  const handleLanguageChange = async (lang: string) => {
    setCaptionLang(lang);
    if (!baseUrl || !token) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/captions/language`, {
        method: "POST",
        body: { language: lang },
      });
    } catch { /* ignore */ }
  };

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
        <>
          <div className="flex items-center gap-2">
            <Globe size={14} className="text-secondary" />
            <select
              value={captionLang}
              onChange={(e) => handleLanguageChange(e.target.value)}
              className="text-xs bg-hover border border-border-subtle rounded px-2 py-1 flex-1"
            >
              {CAPTION_LANGUAGES.map((l) => (
                <option key={l.code} value={l.code}>{l.label}</option>
              ))}
            </select>
          </div>
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
        </>
      )}
    </div>
  );
}

// ── Registration Panel ────────────────────────────────────────────

interface WebinarRegistration {
  id: string;
  conference_id: string;
  name: string;
  email: string;
  status: string;
  registered_at: string;
  custom_fields: Record<string, unknown>;
}

function RegistrationPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [registrations, setRegistrations] = useState<WebinarRegistration[]>([]);
  const [regName, setRegName] = useState("");
  const [regEmail, setRegEmail] = useState("");
  const [isOrganizer, setIsOrganizer] = useState(false);

  const loadRegistrations = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const regs = await paleServerApi<WebinarRegistration[]>(baseUrl, token, `/v1/conferences/${conferenceId}/registrations`);
      setRegistrations(regs);
      setIsOrganizer(true);
    } catch {
      setIsOrganizer(false);
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { loadRegistrations(); }, [loadRegistrations]);

  const handleRegister = async () => {
    if (!baseUrl || !token || !regName || !regEmail) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/register`, {
        method: "POST",
        body: { name: regName, email: regEmail },
      });
      setRegName("");
      setRegEmail("");
      toast({ type: "success", title: "Registered successfully" });
      loadRegistrations();
    } catch { err("Registration failed"); }
  };

  const handleUpdateStatus = async (regId: string, status: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/registrations/${regId}`, {
        method: "PUT",
        body: { status },
      });
      loadRegistrations();
    } catch { err("Failed to update status"); }
  };

  return (
    <div className="space-y-3">
      <h3 className="text-sm font-medium">Webinar Registration</h3>
      <div className="space-y-2">
        <input
          type="text"
          placeholder="Name"
          value={regName}
          onChange={(e) => setRegName(e.target.value)}
          className="w-full text-xs px-2 py-1.5 rounded bg-hover border border-border-subtle"
        />
        <input
          type="email"
          placeholder="Email"
          value={regEmail}
          onChange={(e) => setRegEmail(e.target.value)}
          className="w-full text-xs px-2 py-1.5 rounded bg-hover border border-border-subtle"
        />
        <button
          onClick={handleRegister}
          disabled={!regName || !regEmail}
          className="w-full text-xs px-3 py-1.5 rounded bg-accent text-white hover:bg-accent/90 disabled:opacity-40"
        >
          Register
        </button>
      </div>

      {isOrganizer && registrations.length > 0 && (
        <div className="space-y-1 mt-4">
          <h4 className="text-xs font-medium text-secondary">Registrations ({registrations.length})</h4>
          {registrations.map((reg) => (
            <div key={reg.id} className="flex items-center justify-between text-xs py-1 border-b border-border-subtle">
              <div>
                <span className="font-medium">{reg.name}</span>
                <span className="text-secondary ml-1">({reg.email})</span>
                <span className={cn("ml-2 px-1.5 py-0.5 rounded text-[10px]",
                  reg.status === "approved" ? "bg-green-500/20 text-green-400" :
                  reg.status === "rejected" ? "bg-red-500/20 text-red-400" :
                  reg.status === "waitlisted" ? "bg-yellow-500/20 text-yellow-400" :
                  "bg-blue-500/20 text-blue-400"
                )}>{reg.status}</span>
              </div>
              <div className="flex gap-1">
                {reg.status !== "approved" && (
                  <button onClick={() => handleUpdateStatus(reg.id, "approved")} className="text-green-400 hover:text-green-300"><Check size={12} /></button>
                )}
                {reg.status !== "rejected" && (
                  <button onClick={() => handleUpdateStatus(reg.id, "rejected")} className="text-red-400 hover:text-red-300"><X size={12} /></button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
