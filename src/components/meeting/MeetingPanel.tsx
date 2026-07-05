import { useState, useEffect, useCallback, useRef, type ReactNode } from "react";
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
  ClipboardList,
  Globe,
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
import { currentMediaRuntimeCapabilities, type MediaRuntimeCapabilities } from "@/lib/mediaCapabilities";
import { toast } from "@/components/ui/Toast";

function err(title: string) { toast({ type: "error", title }); }

type MeetingTab = "people" | "lobby" | "hands" | "polls" | "qa" | "breakout" | "media" | "stream" | "townhall" | "present" | "captions" | "assistant" | "reactions" | "chat" | "greenroom" | "registration" | "annotate" | "whiteboard";

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
    { id: "media", icon: Mic, label: "Media" },
    { id: "stream", icon: Shield, label: "Stream" },
    { id: "townhall", icon: Users, label: "Town" },
    { id: "present", icon: ClipboardList, label: "Present" },
    { id: "captions", icon: Captions, label: "Captions" },
    { id: "assistant", icon: Sparkles, label: "AI" },
    { id: "registration", icon: ClipboardList, label: "Register" },
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
        {tab === "media" && <MeetingMediaPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "stream" && <StreamPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "townhall" && <TownHallPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "present" && <PresentationPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "captions" && <CaptionsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "assistant" && <MeetingAssistantPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "registration" && <RegistrationPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
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

// ── Meeting Media Panel ───────────────────────────────────────────

interface MeetingMediaSettings {
  user_uri: string;
  echo_cancellation: boolean;
  noise_suppression: boolean;
  auto_gain: boolean;
  background_mode: "none" | "blur" | "image";
  background_image_url?: string | null;
  noise_suppression_configured: boolean;
  virtual_backgrounds_configured: boolean;
  updated_at: string;
}

interface ConferenceLayoutState {
  conference_id: string;
  mode: "speaker" | "gallery" | "together";
  max_visible: number;
  together_scene?: string | null;
  stage_participant_ids: string[];
  sfu_layout_configured: boolean;
  gallery_capacity: number;
  together_scene_supported: boolean;
  layout_blockers: string[];
  updated_by?: string | null;
  updated_at: string;
}

function MeetingMediaPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [settings, setSettings] = useState<MeetingMediaSettings | null>(null);
  const [layout, setLayout] = useState<ConferenceLayoutState | null>(null);
  const [backgroundUrl, setBackgroundUrl] = useState("");
  const [runtime, setRuntime] = useState<MediaRuntimeCapabilities | null>(null);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const [mediaSettings, layoutState] = await Promise.all([
        paleServerApi<MeetingMediaSettings>(baseUrl, token, "/v1/media/settings"),
        paleServerApi<ConferenceLayoutState>(baseUrl, token, `/v1/conferences/${conferenceId}/layout`),
      ]);
      setSettings(mediaSettings);
      setLayout(layoutState);
      setBackgroundUrl(mediaSettings.background_image_url || "");
    } catch {
      err("Failed to load media settings");
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);
  useEffect(() => {
    setRuntime(currentMediaRuntimeCapabilities());
  }, []);

  const saveSettings = async (patch: Partial<MeetingMediaSettings>) => {
    if (!baseUrl || !token || !settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    try {
      const saved = await paleServerApi<MeetingMediaSettings>(baseUrl, token, "/v1/media/settings", {
        method: "PUT",
        body: {
          echo_cancellation: next.echo_cancellation,
          noise_suppression: next.noise_suppression,
          auto_gain: next.auto_gain,
          background_mode: next.background_mode,
          background_image_url: next.background_mode === "image" ? backgroundUrl : null,
        },
      });
      setSettings(saved);
      setBackgroundUrl(saved.background_image_url || "");
    } catch {
      err("Failed to save media settings");
      load();
    }
  };

  const saveLayout = async (patch: Partial<ConferenceLayoutState>) => {
    if (!baseUrl || !token || !layout) return;
    const next = { ...layout, ...patch };
    setLayout(next);
    try {
      const saved = await paleServerApi<ConferenceLayoutState>(baseUrl, token, `/v1/conferences/${conferenceId}/layout`, {
        method: "PUT",
        body: {
          mode: next.mode,
          max_visible: next.max_visible,
          together_scene: next.together_scene,
          stage_participant_ids: next.stage_participant_ids,
        },
      });
      setLayout(saved);
    } catch {
      err("Only hosts and moderators can change layout");
      load();
    }
  };

  if (!settings || !layout) {
    return <p className="text-xs text-secondary text-center py-4">Loading media settings...</p>;
  }

  return (
    <div className="space-y-4">
      <div>
        <span className="text-sm font-medium">Media Effects</span>
        <p className="text-[11px] text-secondary">
          Noise {settings.noise_suppression_configured ? "ready" : "not configured"} · Backgrounds {settings.virtual_backgrounds_configured ? "ready" : "not configured"}
        </p>
        {runtime && (
          <p className="text-[11px] text-secondary">
            Client noise {runtime.noiseSuppression ? "available" : "unavailable"} · Client backgrounds {runtime.virtualBackgrounds ? "available" : runtime.backgroundBlur ? "blur only" : "unavailable"}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <label className="flex items-center justify-between text-xs">
          <span>Echo cancellation</span>
          <input type="checkbox" checked={settings.echo_cancellation} disabled={runtime ? !runtime.echoCancellation : false} onChange={(e) => saveSettings({ echo_cancellation: e.target.checked })} />
        </label>
        <label className="flex items-center justify-between text-xs">
          <span>Noise suppression</span>
          <input type="checkbox" checked={settings.noise_suppression} disabled={runtime ? !runtime.noiseSuppression : false} onChange={(e) => saveSettings({ noise_suppression: e.target.checked })} />
        </label>
        <label className="flex items-center justify-between text-xs">
          <span>Auto gain</span>
          <input type="checkbox" checked={settings.auto_gain} disabled={runtime ? !runtime.autoGainControl : false} onChange={(e) => saveSettings({ auto_gain: e.target.checked })} />
        </label>
      </div>

      <div className="space-y-2">
        <div className="text-xs font-medium text-secondary">Video background</div>
        <div className="grid grid-cols-3 gap-1">
          {(["none", "blur", "image"] as const).map((mode) => (
            <button
              key={mode}
              disabled={mode === "blur" ? runtime ? !runtime.backgroundBlur : false : mode === "image" ? runtime ? !runtime.virtualBackgrounds : false : false}
              onClick={() => saveSettings({ background_mode: mode })}
              className={cn(
                "py-1.5 text-xs rounded border capitalize",
                settings.background_mode === mode ? "border-accent/40 bg-accent/10 text-accent" : "border-border-subtle bg-hover text-secondary",
                (mode === "blur" ? runtime ? !runtime.backgroundBlur : false : mode === "image" ? runtime ? !runtime.virtualBackgrounds : false : false) && "opacity-40 cursor-not-allowed"
              )}
            >
              {mode}
            </button>
          ))}
        </div>
        {runtime && runtime.blockers.length > 0 && (
          <p className="text-[11px] text-tertiary">
            Limited by {runtime.blockers.slice(0, 3).join(", ")}
          </p>
        )}
        {settings.background_mode === "image" && (
          <div className="flex gap-1">
            <input
              className="flex-1 text-xs rounded border border-border-subtle bg-input px-2 py-1.5"
              value={backgroundUrl}
              onChange={(e) => setBackgroundUrl(e.target.value)}
              placeholder="Image URL"
            />
            <button onClick={() => saveSettings({ background_mode: "image" })} className="px-2 text-xs rounded bg-accent text-white">Save</button>
          </div>
        )}
      </div>

      <div className="border-t border-border-subtle pt-3 space-y-2">
        <div>
          <span className="text-sm font-medium">Meeting Layout</span>
          <p className="text-[11px] text-secondary">
            Gallery capacity {layout.gallery_capacity} · Together mode {layout.layout_blockers.length === 0 || layout.mode !== "together" ? "ready" : "blocked"}
          </p>
        </div>
        <div className="grid grid-cols-3 gap-1">
          {(["speaker", "gallery", "together"] as const).map((mode) => (
            <button
              key={mode}
              disabled={mode === "together" && !layout.sfu_layout_configured}
              onClick={() => saveLayout({ mode })}
              className={cn(
                "py-1.5 text-xs rounded border capitalize",
                layout.mode === mode ? "border-accent/40 bg-accent/10 text-accent" : "border-border-subtle bg-hover text-secondary",
                mode === "together" && !layout.sfu_layout_configured && "opacity-40 cursor-not-allowed"
              )}
            >
              {mode}
            </button>
          ))}
        </div>
        <label className="block text-xs text-secondary">
          Visible tiles
          <input
            type="number"
            min={1}
            max={layout.gallery_capacity || 49}
            value={layout.max_visible}
            onChange={(e) => saveLayout({ max_visible: Number(e.target.value) || 1 })}
            className="mt-1 w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5 text-primary"
          />
        </label>
        {layout.mode === "together" && (
          <label className="block text-xs text-secondary">
            Scene
            <select
              value={layout.together_scene || "auditorium"}
              onChange={(e) => saveLayout({ together_scene: e.target.value })}
              className="mt-1 w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5 text-primary"
            >
              <option value="auditorium">Auditorium</option>
              <option value="conference">Conference</option>
              <option value="classroom">Classroom</option>
            </select>
          </label>
        )}
        {layout.layout_blockers.length > 0 && (
          <p className="text-[11px] text-tertiary">
            Limited by {layout.layout_blockers.join(", ")}
          </p>
        )}
      </div>

      <LiveKitRecordingControls conferenceId={conferenceId} baseUrl={baseUrl} token={token} />
    </div>
  );
}

// ── LiveKit Recording Controls ──────────────────────────────────

function LiveKitRecordingControls({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const conference = useMeetingStore((s) => s.conferences[conferenceId]);
  const setConference = useMeetingStore((s) => s.setConference);
  const [loading, setLoading] = useState(false);

  if (!conference?.livekit_room) return null;

  const recording = !!conference.livekit_egress_id;

  const toggleRecording = async () => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      if (recording) {
        await paleServerApi(baseUrl, token, `/v1/conferences/${conferenceId}/livekit-recording`, { method: "DELETE" });
        setConference({ ...conference, livekit_egress_id: null });
      } else {
        const resp = await paleServerApi<{ egress_id: string }>(baseUrl, token, `/v1/conferences/${conferenceId}/livekit-recording`, { method: "POST" });
        setConference({ ...conference, livekit_egress_id: resp.egress_id });
      }
    } catch {
      err(recording ? "Failed to stop recording" : "Failed to start recording");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="border-t border-border-subtle pt-3 space-y-2">
      <span className="text-sm font-medium">LiveKit Recording</span>
      <p className="text-[11px] text-secondary">
        {recording ? "Recording in progress" : "Not recording"}
      </p>
      <button
        onClick={toggleRecording}
        disabled={loading}
        className={cn(
          "w-full py-1.5 rounded text-sm",
          recording ? "bg-red-600 text-white hover:bg-red-500" : "bg-accent text-white hover:bg-accent/90",
          loading && "opacity-50 cursor-not-allowed"
        )}
      >
        {loading ? "..." : recording ? "Stop Recording" : "Start Recording"}
      </button>
    </div>
  );
}

// ── Streaming Panel ───────────────────────────────────────────────

interface MeetingStreamSession {
  id: string;
  conference_id: string;
  target_kind: "rtmp" | "ndi";
  name: string;
  destination: string;
  status: "pending" | "live" | "stopped" | "failed";
  started_by: string;
  started_at: string;
  stopped_at?: string | null;
  health: string;
  gateway_configured: boolean;
}

function StreamPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [sessions, setSessions] = useState<MeetingStreamSession[]>([]);
  const [kind, setKind] = useState<"rtmp" | "ndi">("rtmp");
  const [name, setName] = useState("Program");
  const [destination, setDestination] = useState("");

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      setSessions(await paleServerApi<MeetingStreamSession[]>(baseUrl, token, `/v1/conferences/${conferenceId}/streams`));
    } catch {
      err("Failed to load streams");
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);

  const start = async () => {
    if (!baseUrl || !token || !destination.trim()) return;
    try {
      await paleServerApi<MeetingStreamSession>(baseUrl, token, `/v1/conferences/${conferenceId}/streams`, {
        method: "POST",
        body: { target_kind: kind, name, destination },
      });
      setDestination("");
      load();
    } catch {
      err("Streaming gateway is not configured or target is invalid");
    }
  };

  const stop = async (sessionId: string) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<MeetingStreamSession>(baseUrl, token, `/v1/streams/${sessionId}`, { method: "DELETE" });
      load();
    } catch {
      err("Only hosts and moderators can stop streams");
    }
  };

  const active = sessions.filter((session) => session.status === "live");
  const gatewayConfigured = sessions[0]?.gateway_configured ?? false;

  return (
    <div className="space-y-3">
      <div>
        <span className="text-sm font-medium">NDI / RTMP Streaming</span>
        <p className="text-[11px] text-secondary">
          Gateway {gatewayConfigured ? "configured" : "required"} · {active.length} live
        </p>
      </div>

      <div className="space-y-2 p-2 border border-border-subtle rounded">
        <div className="grid grid-cols-2 gap-1">
          {(["rtmp", "ndi"] as const).map((value) => (
            <button
              key={value}
              onClick={() => setKind(value)}
              className={cn(
                "py-1.5 text-xs rounded border uppercase",
                kind === value ? "border-accent/40 bg-accent/10 text-accent" : "border-border-subtle bg-hover text-secondary"
              )}
            >
              {value}
            </button>
          ))}
        </div>
        <input
          className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Stream name"
        />
        <input
          className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
          value={destination}
          onChange={(e) => setDestination(e.target.value)}
          placeholder={kind === "rtmp" ? "rtmps://live.example/app/key" : "NDI output name"}
        />
        <button onClick={start} disabled={!destination.trim()} className="w-full py-1.5 bg-accent text-white rounded text-sm hover:bg-accent/90 disabled:opacity-40">
          Start Stream
        </button>
      </div>

      <div className="space-y-1">
        {sessions.map((session) => (
          <div key={session.id} className="p-2 rounded border border-border-subtle bg-hover text-xs space-y-1">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium truncate">{session.name}</span>
              <span className={cn(
                "px-1.5 py-0.5 rounded text-[10px]",
                session.status === "live" ? "bg-green-500/15 text-green-500" : "bg-surface text-secondary"
              )}>
                {session.status}
              </span>
            </div>
            <div className="text-secondary truncate">{session.target_kind.toUpperCase()} · {session.destination}</div>
            <div className="text-tertiary">{session.health}</div>
            {session.status === "live" && (
              <button onClick={() => stop(session.id)} className="w-full py-1 rounded bg-red-500 text-white text-xs">
                Stop
              </button>
            )}
          </div>
        ))}
        {sessions.length === 0 && <p className="text-xs text-secondary text-center py-4">No streams started</p>}
      </div>
    </div>
  );
}

// ── Town Hall Panel ───────────────────────────────────────────────

interface TownHallConfig {
  conference_id: string;
  enabled: boolean;
  max_viewers: number;
  registration_required: boolean;
  presenter_only_video: boolean;
  attendee_mic_disabled: boolean;
  qna_moderation_required: boolean;
  overflow_url?: string | null;
  broadcast_provider_configured: boolean;
  broadcast_capacity: number;
  attendee_mode: string;
  broadcast_ready: boolean;
  broadcast_blockers: string[];
  updated_by?: string | null;
  updated_at: string;
}

function TownHallPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [config, setConfig] = useState<TownHallConfig | null>(null);
  const [overflowUrl, setOverflowUrl] = useState("");

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<TownHallConfig>(baseUrl, token, `/v1/conferences/${conferenceId}/town-hall`);
      setConfig(data);
      setOverflowUrl(data.overflow_url || "");
    } catch {
      err("Town hall requires an active webinar");
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);

  const save = async (patch: Partial<TownHallConfig>) => {
    if (!baseUrl || !token || !config) return;
    const next = { ...config, ...patch };
    setConfig(next);
    try {
      const saved = await paleServerApi<TownHallConfig>(baseUrl, token, `/v1/conferences/${conferenceId}/town-hall`, {
        method: "PUT",
        body: {
          enabled: next.enabled,
          max_viewers: next.max_viewers,
          registration_required: next.registration_required,
          presenter_only_video: next.presenter_only_video,
          attendee_mic_disabled: next.attendee_mic_disabled,
          qna_moderation_required: next.qna_moderation_required,
          overflow_url: overflowUrl,
        },
      });
      setConfig(saved);
      setOverflowUrl(saved.overflow_url || "");
    } catch {
      err("Only webinar hosts and moderators can update town hall");
      load();
    }
  };

  if (!config) {
    return <p className="text-xs text-secondary text-center py-4">Loading town hall settings...</p>;
  }

  return (
    <div className="space-y-3">
      <div>
        <span className="text-sm font-medium">Town Hall Broadcast</span>
        <p className="text-[11px] text-secondary">
          {config.attendee_mode || "interactive"} · effective capacity {(config.broadcast_capacity || config.max_viewers).toLocaleString()} · requested {config.max_viewers.toLocaleString()}
        </p>
        <p className="text-[11px] text-secondary">
          Broadcast {config.broadcast_ready ? "ready" : config.broadcast_provider_configured ? "configured" : "provider required"}
        </p>
      </div>

      <label className="flex items-center justify-between text-xs rounded border border-border-subtle bg-hover px-2 py-2">
        <span>Broadcast mode</span>
        <input type="checkbox" checked={config.enabled} onChange={(e) => save({ enabled: e.target.checked })} />
      </label>

      <label className="block text-xs text-secondary">
        Max viewers
        <input
          type="number"
          min={1}
          max={100000}
          value={config.max_viewers}
          onChange={(e) => save({ max_viewers: Number(e.target.value) || 1 })}
          className="mt-1 w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5 text-primary"
        />
      </label>

      <div className="space-y-2">
        <TownHallToggle label="Require registration" checked={config.registration_required} onChange={(value) => save({ registration_required: value })} />
        <TownHallToggle label="Presenter video only" checked={config.presenter_only_video} onChange={(value) => save({ presenter_only_video: value })} />
        <TownHallToggle label="Disable attendee mics" checked={config.attendee_mic_disabled} onChange={(value) => save({ attendee_mic_disabled: value })} />
        <TownHallToggle label="Moderate Q&A" checked={config.qna_moderation_required} onChange={(value) => save({ qna_moderation_required: value })} />
        {config.broadcast_blockers.length > 0 && (
          <p className="text-[11px] text-tertiary">
            Limited by {config.broadcast_blockers.join(", ")}
          </p>
        )}
      </div>

      <label className="block text-xs text-secondary">
        Overflow URL
        <div className="mt-1 flex gap-1">
          <input
            value={overflowUrl}
            onChange={(e) => setOverflowUrl(e.target.value)}
            placeholder="https://cdn.example/overflow"
            className="flex-1 text-xs rounded border border-border-subtle bg-input px-2 py-1.5 text-primary"
          />
          <button onClick={() => save({ overflow_url: overflowUrl })} className="px-2 text-xs rounded bg-accent text-white">
            Save
          </button>
        </div>
      </label>
    </div>
  );
}

function TownHallToggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <label className="flex items-center justify-between text-xs">
      <span>{label}</span>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

// ── PowerPoint Live Panel ─────────────────────────────────────────

interface PresentationSlide {
  index: number;
  title: string;
  notes?: string | null;
  render_url?: string | null;
}

interface PresentationSession {
  id: string;
  conference_id: string;
  title: string;
  source_file_id?: string | null;
  presenter_uri: string;
  slides: PresentationSlide[];
  current_slide: number;
  attendee_navigation_enabled: boolean;
  renderer_configured: boolean;
  ended_at?: string | null;
  created_at: string;
  updated_at: string;
}

function PresentationPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [sessions, setSessions] = useState<PresentationSession[]>([]);
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [slideText, setSlideText] = useState("Opening\nPlan\nRisks\nNext steps");
  const [attendeeNav, setAttendeeNav] = useState(false);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<PresentationSession[]>(baseUrl, token, `/v1/conferences/${conferenceId}/presentations`);
      setSessions(data);
    } catch {
      err("Failed to load presentations");
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);

  const active = sessions.find((session) => !session.ended_at) ?? sessions[0];
  const slide = active?.slides[active.current_slide];

  const create = async () => {
    if (!baseUrl || !token) return;
    const slides = slideText
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => ({ title: line, notes: null, render_url: null }));
    if (slides.length === 0) return;
    try {
      await paleServerApi<PresentationSession>(baseUrl, token, `/v1/conferences/${conferenceId}/presentations`, {
        method: "POST",
        body: {
          title: title.trim() || "Presentation",
          source_file_id: null,
          slides,
          attendee_navigation_enabled: attendeeNav,
        },
      });
      setCreating(false);
      setTitle("");
      load();
    } catch {
      err("Only hosts and moderators can start presentations");
    }
  };

  const update = async (session: PresentationSession, patch: Partial<Pick<PresentationSession, "current_slide" | "attendee_navigation_enabled" | "presenter_uri">>) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<PresentationSession>(baseUrl, token, `/v1/presentations/${session.id}`, {
        method: "PUT",
        body: patch,
      });
      load();
    } catch {
      err("Only hosts and moderators can control presentations");
    }
  };

  const end = async (session: PresentationSession) => {
    if (!baseUrl || !token) return;
    try {
      await paleServerApi<PresentationSession>(baseUrl, token, `/v1/presentations/${session.id}`, { method: "DELETE" });
      load();
    } catch {
      err("Failed to end presentation");
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <span className="text-sm font-medium">PowerPoint Live</span>
          <p className="text-[11px] text-secondary">
            {active?.renderer_configured ? "Renderer configured" : "Presenter-control mode"}
          </p>
        </div>
        <button onClick={() => setCreating(!creating)} className="text-xs text-accent hover:underline">
          {creating ? "Cancel" : "+ New deck"}
        </button>
      </div>

      {creating && (
        <div className="space-y-2 p-2 border border-border-subtle rounded">
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Deck title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <textarea
            className="w-full h-24 text-sm rounded border border-border-subtle bg-input px-2 py-1.5 resize-none"
            value={slideText}
            onChange={(e) => setSlideText(e.target.value)}
            placeholder="One slide title per line"
          />
          <label className="flex items-center gap-2 text-xs text-secondary">
            <input type="checkbox" checked={attendeeNav} onChange={(e) => setAttendeeNav(e.target.checked)} />
            Allow attendee navigation
          </label>
          <button onClick={create} className="w-full py-1.5 bg-accent text-white rounded text-sm hover:bg-accent/90">
            Start Presentation
          </button>
        </div>
      )}

      {!active ? (
        <p className="text-xs text-secondary text-center py-4">No active presentation</p>
      ) : (
        <div className="space-y-3">
          <div className={cn("rounded border p-3", active.ended_at ? "border-border-subtle bg-hover/40" : "border-accent/30 bg-accent/5")}>
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <div className="text-sm font-medium truncate">{active.title}</div>
                <div className="text-[11px] text-secondary">
                  Slide {active.current_slide + 1} of {active.slides.length} · {active.presenter_uri.replace(/^sip:/, "")}
                </div>
              </div>
              <span className={cn("text-[10px] px-1.5 py-0.5 rounded shrink-0", active.ended_at ? "bg-hover text-secondary" : "bg-green-500/15 text-green-500")}>
                {active.ended_at ? "ended" : "live"}
              </span>
            </div>

            <div className="mt-3 aspect-video rounded bg-base border border-border-subtle p-3 flex flex-col justify-between">
              <div className="text-[10px] uppercase text-tertiary">Current slide</div>
              <div className="text-lg font-semibold text-primary break-words">{slide?.title || "Slide"}</div>
              <div className="text-[11px] text-secondary">
                {slide?.render_url ? "Rendered slide available" : active.renderer_configured ? "Waiting for renderer output" : "Renderer not configured"}
              </div>
            </div>

            {!active.ended_at && (
              <div className="grid grid-cols-2 gap-2 mt-3">
                <button
                  onClick={() => update(active, { current_slide: Math.max(0, active.current_slide - 1) })}
                  disabled={active.current_slide === 0}
                  className="py-1.5 text-xs rounded bg-hover border border-border-subtle disabled:opacity-40"
                >
                  Previous
                </button>
                <button
                  onClick={() => update(active, { current_slide: Math.min(active.slides.length - 1, active.current_slide + 1) })}
                  disabled={active.current_slide >= active.slides.length - 1}
                  className="py-1.5 text-xs rounded bg-hover border border-border-subtle disabled:opacity-40"
                >
                  Next
                </button>
                <button
                  onClick={() => update(active, { attendee_navigation_enabled: !active.attendee_navigation_enabled })}
                  className={cn("py-1.5 text-xs rounded border", active.attendee_navigation_enabled ? "bg-accent/15 border-accent/30 text-accent" : "bg-hover border-border-subtle")}
                >
                  Attendee Nav {active.attendee_navigation_enabled ? "On" : "Off"}
                </button>
                <button onClick={() => end(active)} className="py-1.5 text-xs rounded bg-red-500 text-white">
                  End
                </button>
              </div>
            )}
          </div>

          <div className="space-y-1 max-h-48 overflow-y-auto">
            {active.slides.map((item) => (
              <button
                key={item.index}
                disabled={Boolean(active.ended_at)}
                onClick={() => update(active, { current_slide: item.index })}
                className={cn(
                  "w-full text-left px-2 py-1.5 rounded text-xs border",
                  item.index === active.current_slide ? "border-accent/40 bg-accent/10 text-primary" : "border-border-subtle bg-hover text-secondary"
                )}
              >
                {item.index + 1}. {item.title}
              </button>
            ))}
          </div>
        </div>
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

// ── Meeting Assistant Panel ───────────────────────────────────────

interface MeetingAssistantReport {
  conference_id: string;
  title: string;
  generated_at: string;
  transcript_segments: number;
  ai_provider_configured: boolean;
  summary: string;
  key_topics: string[];
  action_items: { owner?: string | null; text: string; source_segment_id: string }[];
  decisions: string[];
  risks: string[];
  open_questions: string[];
  speaker_stats: { speaker_uri: string; speaker_name: string; segments: number; words: number }[];
}

function MeetingAssistantPanel({ conferenceId, baseUrl, token }: { conferenceId: string; baseUrl: string | null; token: string | null }) {
  const [report, setReport] = useState<MeetingAssistantReport | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      const data = await paleServerApi<MeetingAssistantReport>(baseUrl, token, `/v1/conferences/${conferenceId}/assistant`);
      setReport(data);
    } catch {
      err("Failed to generate meeting assistant report");
    } finally {
      setLoading(false);
    }
  }, [baseUrl, token, conferenceId]);

  useEffect(() => { load(); }, [load]);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <span className="text-sm font-medium">Meeting Assistant</span>
          <p className="text-[11px] text-secondary">
            {report?.ai_provider_configured ? "AI provider configured" : "Local extractive mode"}
          </p>
        </div>
        <button onClick={load} disabled={loading} className="text-xs text-accent hover:underline disabled:text-tertiary">
          {loading ? "Generating..." : "Refresh"}
        </button>
      </div>
      {!report ? (
        <p className="text-xs text-secondary text-center py-4">No assistant report available.</p>
      ) : (
        <>
          <div className="rounded-md bg-hover p-2">
            <div className="text-xs font-medium text-secondary mb-1">Summary</div>
            <p className="text-sm text-primary">{report.summary}</p>
            <div className="text-[10px] text-tertiary mt-2">
              {report.transcript_segments} transcript segments · {new Date(report.generated_at).toLocaleString()}
            </div>
          </div>
          <AssistantSection title="Topics" empty="No topics yet">
            <div className="flex flex-wrap gap-1">
              {report.key_topics.map((topic) => (
                <span key={topic} className="px-1.5 py-0.5 rounded bg-accent/10 text-accent text-[11px]">{topic}</span>
              ))}
            </div>
          </AssistantSection>
          <AssistantSection title="Action Items" empty="No action items detected" showEmpty={report.action_items.length === 0}>
            {report.action_items.map((item) => (
              <div key={item.source_segment_id} className="rounded bg-base px-2 py-1 text-xs">
                <div className="text-primary">{item.text}</div>
                {item.owner && <div className="text-[10px] text-tertiary mt-0.5">Owner: {item.owner}</div>}
              </div>
            ))}
          </AssistantSection>
          <AssistantList title="Decisions" items={report.decisions} empty="No decisions detected" />
          <AssistantList title="Risks" items={report.risks} empty="No risks detected" />
          <AssistantList title="Open Questions" items={report.open_questions} empty="No open questions detected" />
          <AssistantSection title="Speaker Stats" empty="No speaker stats" showEmpty={report.speaker_stats.length === 0}>
            {report.speaker_stats.map((stat) => (
              <div key={stat.speaker_uri} className="flex items-center justify-between rounded bg-base px-2 py-1 text-xs">
                <span className="truncate">{stat.speaker_name || stat.speaker_uri.replace(/^sip:/, "")}</span>
                <span className="text-tertiary shrink-0">{stat.words} words</span>
              </div>
            ))}
          </AssistantSection>
        </>
      )}
    </div>
  );
}

function AssistantSection({ title, empty, showEmpty, children }: { title: string; empty: string; showEmpty?: boolean; children: ReactNode }) {
  return (
    <div className="space-y-1">
      <div className="text-xs font-medium text-secondary">{title}</div>
      {showEmpty ? <p className="text-xs text-tertiary">{empty}</p> : children}
    </div>
  );
}

function AssistantList({ title, items, empty }: { title: string; items: string[]; empty: string }) {
  return (
    <AssistantSection title={title} empty={empty} showEmpty={items.length === 0}>
      <div className="space-y-1">
        {items.map((item) => (
          <div key={item} className="rounded bg-base px-2 py-1 text-xs text-primary">{item}</div>
        ))}
      </div>
    </AssistantSection>
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
