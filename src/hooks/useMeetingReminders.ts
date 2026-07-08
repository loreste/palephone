import { useCallback, useEffect, useRef } from "react";
import { useMeetingStore, type ScheduledMeeting } from "@/store/meetingStore";
import { useServerStore } from "@/store/serverStore";
import { useUiStore } from "@/store/uiStore";
import { expandMeetingOccurrences, type CalendarMeetingOccurrence } from "@/lib/calendarOccurrences";
import { paleServerApi } from "@/lib/tauri";
import { shouldNotify, shouldPlaySound } from "@/lib/notifications";
import { playNotificationBeep } from "@/lib/notificationSound";
import { notify as desktopNotify } from "@/lib/nativeNotify";
import { toast } from "@/components/ui/Toast";

const REMINDER_LOOKAHEAD_MS = 15 * 60 * 1000;
const STARTING_WINDOW_MS = 60 * 1000;
const CHECK_INTERVAL_MS = 30 * 1000;
const REFRESH_INTERVAL_MS = 5 * 60 * 1000;

export function shouldSendMeetingReminder(
  occurrence: CalendarMeetingOccurrence,
  now: Date,
  sentKeys: ReadonlySet<string>,
) {
  if (occurrence.status === "cancelled") return null;
  const startsAt = new Date(occurrence.starts_at).getTime();
  const diffMs = startsAt - now.getTime();
  if (diffMs > 0 && diffMs <= REMINDER_LOOKAHEAD_MS) {
    const key = `${occurrence.occurrence_key}:15m`;
    return sentKeys.has(key) ? null : { key, label: "starts in 15 minutes" };
  }
  if (diffMs <= 0 && diffMs >= -STARTING_WINDOW_MS) {
    const key = `${occurrence.occurrence_key}:now`;
    return sentKeys.has(key) ? null : { key, label: "is starting now" };
  }
  return null;
}

function formatReminderTime(iso: string) {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function reminderBody(occurrence: ScheduledMeeting, label: string) {
  const time = formatReminderTime(occurrence.starts_at);
  const attendees = occurrence.participants.length > 0
    ? ` · ${occurrence.participants.length} attendees`
    : "";
  return `${label} at ${time}${attendees}`;
}

export function useMeetingReminders() {
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);
  const setMeetings = useMeetingStore((s) => s.setMeetings);
  const meetings = useMeetingStore((s) => s.meetings);
  const sentRef = useRef<Set<string>>(new Set());
  const refreshRef = useRef<number | null>(null);
  const checkRef = useRef<number | null>(null);

  const refreshMeetings = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<ScheduledMeeting[]>(baseUrl, token, "/v1/meetings");
      setMeetings(data);
    } catch {
      // Calendar tab can still load meetings manually; reminders retry on the next interval.
    }
  }, [baseUrl, token, setMeetings]);

  useEffect(() => {
    refreshMeetings();
    const onMeetingChanged = () => refreshMeetings();
    window.addEventListener("pale:meeting-scheduled", onMeetingChanged);
    window.addEventListener("pale:meeting-updated", onMeetingChanged);
    window.addEventListener("pale:meeting-cancelled", onMeetingChanged);
    refreshRef.current = window.setInterval(refreshMeetings, REFRESH_INTERVAL_MS);
    return () => {
      window.removeEventListener("pale:meeting-scheduled", onMeetingChanged);
      window.removeEventListener("pale:meeting-updated", onMeetingChanged);
      window.removeEventListener("pale:meeting-cancelled", onMeetingChanged);
      if (refreshRef.current) {
        window.clearInterval(refreshRef.current);
        refreshRef.current = null;
      }
    };
  }, [refreshMeetings]);

  useEffect(() => {
    const checkReminders = () => {
      const now = new Date();
      const occurrences = expandMeetingOccurrences(
        meetings,
        new Date(now.getTime() - STARTING_WINDOW_MS),
        new Date(now.getTime() + REMINDER_LOOKAHEAD_MS),
      );

      for (const occurrence of occurrences) {
        const reminder = shouldSendMeetingReminder(occurrence, now, sentRef.current);
        if (!reminder) continue;
        sentRef.current.add(reminder.key);
        const body = reminderBody(occurrence, reminder.label);
        shouldNotify(occurrence.room_id ?? undefined).then((ok) => {
          if (!ok) return;
          toast({ type: "info", title: occurrence.title, description: body, duration: 8000 });
          useUiStore.getState().setActiveTab("calendar");
          desktopNotify(occurrence.title, body);
        });
        shouldPlaySound().then((ok) => {
          if (ok) playNotificationBeep();
        });
      }
    };

    checkReminders();
    checkRef.current = window.setInterval(checkReminders, CHECK_INTERVAL_MS);
    return () => {
      if (checkRef.current) {
        window.clearInterval(checkRef.current);
        checkRef.current = null;
      }
    };
  }, [meetings]);
}
