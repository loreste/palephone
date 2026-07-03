import { useState, useEffect, useCallback, useMemo } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Plus,
  Clock,
  Users,
  Video,
  Phone,
} from "lucide-react";
import { cn } from "@/lib/cn";
import { useMeetingStore, type ScheduledMeeting } from "@/store/meetingStore";
import { useServerStore } from "@/store/serverStore";
import { paleServerApi } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";

type CalendarMode = "month" | "week" | "day";

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

function sameDay(a: Date, b: Date) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function formatTime(iso: string) {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function daysInMonth(year: number, month: number) {
  return new Date(year, month + 1, 0).getDate();
}

export function CalendarView() {
  const meetings = useMeetingStore((s) => s.meetings);
  const setMeetings = useMeetingStore((s) => s.setMeetings);
  const addMeeting = useMeetingStore((s) => s.addMeeting);
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);

  const [mode, setMode] = useState<CalendarMode>("month");
  const [currentDate, setCurrentDate] = useState(new Date());
  const [showCreate, setShowCreate] = useState(false);
  const [selectedDay, setSelectedDay] = useState<Date | null>(null);

  // Form state
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [startDate, setStartDate] = useState("");
  const [startTime, setStartTime] = useState("09:00");
  const [endTime, setEndTime] = useState("10:00");
  const [participants, setParticipants] = useState("");

  const loadMeetings = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<ScheduledMeeting[]>(baseUrl, token, "/v1/meetings");
      setMeetings(data);
    } catch { /* ignore */ }
  }, [baseUrl, token, setMeetings]);

  useEffect(() => { loadMeetings(); }, [loadMeetings]);

  const handleCreate = async () => {
    if (!baseUrl || !token || !title || !startDate) return;
    const startsAt = new Date(`${startDate}T${startTime}`).toISOString();
    const endsAt = new Date(`${startDate}T${endTime}`).toISOString();
    const parts = participants
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    try {
      const meeting = await paleServerApi<ScheduledMeeting>(baseUrl, token, "/v1/meetings", {
        method: "POST",
        body: {
          title,
          description: description || undefined,
          participants: parts,
          starts_at: startsAt,
          ends_at: endsAt,
        },
      });
      addMeeting(meeting);
      setShowCreate(false);
      setTitle("");
      setDescription("");
      setParticipants("");
      toast({ type: "info", title: "Meeting scheduled" });
    } catch {
      toast({ type: "error", title: "Failed to create meeting" });
    }
  };

  // Navigation
  const prev = () => {
    const d = new Date(currentDate);
    if (mode === "month") d.setMonth(d.getMonth() - 1);
    else if (mode === "week") d.setDate(d.getDate() - 7);
    else d.setDate(d.getDate() - 1);
    setCurrentDate(d);
  };

  const next = () => {
    const d = new Date(currentDate);
    if (mode === "month") d.setMonth(d.getMonth() + 1);
    else if (mode === "week") d.setDate(d.getDate() + 7);
    else d.setDate(d.getDate() + 1);
    setCurrentDate(d);
  };

  const today = new Date();

  // Build month grid
  const year = currentDate.getFullYear();
  const month = currentDate.getMonth();
  const firstDay = new Date(year, month, 1).getDay();
  const totalDays = daysInMonth(year, month);

  const calendarDays = useMemo(() => {
    const days: (Date | null)[] = [];
    for (let i = 0; i < firstDay; i++) days.push(null);
    for (let d = 1; d <= totalDays; d++) days.push(new Date(year, month, d));
    return days;
  }, [year, month, firstDay, totalDays]);

  const meetingsOnDay = (day: Date) =>
    meetings.filter((m) => sameDay(new Date(m.starts_at), day));

  // Week view days
  const weekDays = useMemo(() => {
    const start = new Date(currentDate);
    start.setDate(start.getDate() - start.getDay());
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(start);
      d.setDate(d.getDate() + i);
      return d;
    });
  }, [currentDate]);

  const viewMeetings = mode === "day"
    ? meetingsOnDay(currentDate)
    : mode === "week"
      ? meetings.filter((m) => {
          const d = new Date(m.starts_at);
          return d >= weekDays[0] && d <= weekDays[6];
        })
      : [];

  const dayDetail = selectedDay ? meetingsOnDay(selectedDay) : [];

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-border-subtle">
        <div className="flex items-center gap-2">
          <button onClick={prev} className="p-1 hover:bg-hover rounded">
            <ChevronLeft size={18} />
          </button>
          <h2 className="text-lg font-semibold min-w-[180px] text-center">
            {mode === "day"
              ? currentDate.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })
              : `${MONTH_NAMES[month]} ${year}`}
          </h2>
          <button onClick={next} className="p-1 hover:bg-hover rounded">
            <ChevronRight size={18} />
          </button>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex rounded-md border border-border-subtle overflow-hidden text-xs">
            {(["month", "week", "day"] as CalendarMode[]).map((m) => (
              <button
                key={m}
                onClick={() => setMode(m)}
                className={cn(
                  "px-3 py-1 capitalize",
                  mode === m ? "bg-accent text-white" : "hover:bg-hover"
                )}
              >
                {m}
              </button>
            ))}
          </div>
          <button
            onClick={() => {
              setShowCreate(true);
              setStartDate(
                (selectedDay ?? today).toISOString().slice(0, 10)
              );
            }}
            className="flex items-center gap-1 px-3 py-1.5 bg-accent text-white rounded-md text-sm hover:bg-accent/90"
          >
            <Plus size={14} /> New Meeting
          </button>
        </div>
      </div>

      {/* Calendar content */}
      <div className="flex-1 overflow-y-auto p-3">
        {mode === "month" && (
          <div className="grid grid-cols-7 gap-px bg-border-subtle rounded-md overflow-hidden">
            {DAY_NAMES.map((d) => (
              <div key={d} className="bg-surface p-2 text-xs font-medium text-secondary text-center">
                {d}
              </div>
            ))}
            {calendarDays.map((day, i) => (
              <button
                key={i}
                onClick={() => day && setSelectedDay(day)}
                disabled={!day}
                className={cn(
                  "bg-surface p-1.5 min-h-[80px] text-left align-top hover:bg-hover transition-colors",
                  day && sameDay(day, today) && "ring-1 ring-accent ring-inset",
                  day && selectedDay && sameDay(day, selectedDay) && "bg-accent/10"
                )}
              >
                {day && (
                  <>
                    <span className={cn(
                      "text-xs font-medium",
                      sameDay(day, today) ? "text-accent" : "text-primary"
                    )}>
                      {day.getDate()}
                    </span>
                    <div className="mt-0.5 space-y-0.5">
                      {meetingsOnDay(day).slice(0, 3).map((m) => (
                        <div
                          key={m.id}
                          className="text-[10px] px-1 py-0.5 bg-accent/15 text-accent rounded truncate"
                        >
                          {formatTime(m.starts_at)} {m.title}
                        </div>
                      ))}
                      {meetingsOnDay(day).length > 3 && (
                        <div className="text-[10px] text-secondary px-1">
                          +{meetingsOnDay(day).length - 3} more
                        </div>
                      )}
                    </div>
                  </>
                )}
              </button>
            ))}
          </div>
        )}

        {mode === "week" && (
          <div className="grid grid-cols-7 gap-2">
            {weekDays.map((day) => (
              <div key={day.toISOString()} className="space-y-1">
                <div className={cn(
                  "text-xs font-medium text-center p-1 rounded",
                  sameDay(day, today) && "bg-accent text-white"
                )}>
                  {DAY_NAMES[day.getDay()]} {day.getDate()}
                </div>
                {meetingsOnDay(day).map((m) => (
                  <div key={m.id} className="text-xs p-2 bg-accent/10 rounded border border-accent/20">
                    <div className="font-medium truncate">{m.title}</div>
                    <div className="text-secondary">{formatTime(m.starts_at)} - {formatTime(m.ends_at)}</div>
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}

        {mode === "day" && (
          <div className="space-y-2">
            {viewMeetings.length === 0 && (
              <p className="text-sm text-secondary py-8 text-center">No meetings scheduled for this day</p>
            )}
            {viewMeetings.map((m) => (
              <MeetingCard key={m.id} meeting={m} />
            ))}
          </div>
        )}

        {/* Day detail sidebar when clicking a day in month view */}
        {selectedDay && mode === "month" && (
          <div className="mt-4 p-3 border border-border-subtle rounded-md bg-surface">
            <div className="flex items-center justify-between mb-2">
              <h3 className="font-medium text-sm">
                {selectedDay.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}
              </h3>
              <button onClick={() => setSelectedDay(null)} className="text-xs text-secondary hover:text-primary">
                Close
              </button>
            </div>
            {dayDetail.length === 0 ? (
              <p className="text-sm text-secondary">No meetings</p>
            ) : (
              <div className="space-y-2">
                {dayDetail.map((m) => (
                  <MeetingCard key={m.id} meeting={m} />
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Create meeting modal */}
      {showCreate && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-surface border border-border-subtle rounded-lg p-4 w-[400px] max-w-[90vw] space-y-3">
            <h3 className="font-semibold text-lg">Schedule Meeting</h3>
            <input
              className="w-full rounded-md border border-border-subtle bg-input px-3 py-2 text-sm"
              placeholder="Meeting title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <textarea
              className="w-full rounded-md border border-border-subtle bg-input px-3 py-2 text-sm resize-none"
              placeholder="Description (optional)"
              rows={2}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
            <div className="grid grid-cols-3 gap-2">
              <input
                type="date"
                className="col-span-1 rounded-md border border-border-subtle bg-input px-2 py-2 text-sm"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
              />
              <input
                type="time"
                className="rounded-md border border-border-subtle bg-input px-2 py-2 text-sm"
                value={startTime}
                onChange={(e) => setStartTime(e.target.value)}
              />
              <input
                type="time"
                className="rounded-md border border-border-subtle bg-input px-2 py-2 text-sm"
                value={endTime}
                onChange={(e) => setEndTime(e.target.value)}
              />
            </div>
            <input
              className="w-full rounded-md border border-border-subtle bg-input px-3 py-2 text-sm"
              placeholder="Participants (comma-separated SIP URIs)"
              value={participants}
              onChange={(e) => setParticipants(e.target.value)}
            />
            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => setShowCreate(false)}
                className="px-4 py-2 text-sm rounded-md hover:bg-hover"
              >
                Cancel
              </button>
              <button
                onClick={handleCreate}
                disabled={!title || !startDate}
                className="px-4 py-2 text-sm bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
              >
                Schedule
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function MeetingCard({ meeting }: { meeting: ScheduledMeeting }) {
  return (
    <div className="p-3 rounded-md border border-border-subtle bg-surface hover:bg-hover transition-colors">
      <div className="flex items-start justify-between">
        <div className="flex-1 min-w-0">
          <h4 className="font-medium text-sm truncate">{meeting.title}</h4>
          {meeting.description && (
            <p className="text-xs text-secondary mt-0.5 truncate">{meeting.description}</p>
          )}
        </div>
        <div className="flex items-center gap-1 ml-2 text-secondary">
          {meeting.conference_id ? <Video size={14} /> : <Phone size={14} />}
        </div>
      </div>
      <div className="flex items-center gap-3 mt-2 text-xs text-secondary">
        <span className="flex items-center gap-1">
          <Clock size={12} />
          {formatTime(meeting.starts_at)} - {formatTime(meeting.ends_at)}
        </span>
        {meeting.participants.length > 0 && (
          <span className="flex items-center gap-1">
            <Users size={12} />
            {meeting.participants.length}
          </span>
        )}
      </div>
    </div>
  );
}
