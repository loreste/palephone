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
  Users,
} from "lucide-react";
import { cn } from "@/lib/cn";
import {
  useMeetingStore,
  type ConferenceLobby,
  type HandRaise,
  type MeetingPoll,
  type QaQuestion,
  type BreakoutSession,
} from "@/store/meetingStore";
import { useServerStore } from "@/store/serverStore";
import { paleServerApi } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";

function err(title: string) { toast({ type: "error", title }); }

type MeetingTab = "lobby" | "hands" | "polls" | "qa" | "breakout" | "captions";

export function MeetingPanel({ conferenceId }: { conferenceId: string }) {
  const [tab, setTab] = useState<MeetingTab>("lobby");
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);

  const tabs: { id: MeetingTab; icon: typeof Hand; label: string }[] = [
    { id: "lobby", icon: DoorOpen, label: "Lobby" },
    { id: "hands", icon: Hand, label: "Hands" },
    { id: "polls", icon: BarChart3, label: "Polls" },
    { id: "qa", icon: MessageCircleQuestion, label: "Q&A" },
    { id: "breakout", icon: LayoutGrid, label: "Rooms" },
    { id: "captions", icon: Captions, label: "Captions" },
  ];

  return (
    <div className="flex flex-col h-full border-l border-border-subtle w-[320px]">
      <div className="flex border-b border-border-subtle">
        {tabs.map(({ id, icon: Icon, label }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={cn(
              "flex-1 flex flex-col items-center gap-0.5 py-2 text-[10px]",
              tab === id ? "text-accent border-b-2 border-accent" : "text-secondary hover:text-primary"
            )}
          >
            <Icon size={16} />
            {label}
          </button>
        ))}
      </div>
      <div className="flex-1 overflow-y-auto p-3">
        {tab === "lobby" && <LobbyPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "hands" && <HandsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "polls" && <PollsPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "qa" && <QaPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "breakout" && <BreakoutPanel conferenceId={conferenceId} baseUrl={baseUrl} token={token} />}
        {tab === "captions" && <CaptionsPanel conferenceId={conferenceId} />}
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
