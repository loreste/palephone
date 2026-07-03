import type { ScheduledMeeting } from "@/store/meetingStore";

export type CalendarMeetingOccurrence = ScheduledMeeting & {
  occurrence_key: string;
  series_id: string;
  is_recurring_occurrence: boolean;
};

function startOfDay(value: Date) {
  const day = new Date(value);
  day.setHours(0, 0, 0, 0);
  return day;
}

function endOfDay(value: Date) {
  const day = new Date(value);
  day.setHours(23, 59, 59, 999);
  return day;
}

function addMonthsClamped(value: Date, count: number, originalDay: number) {
  const next = new Date(value);
  next.setUTCDate(1);
  next.setUTCMonth(next.getUTCMonth() + count);
  const lastDay = new Date(Date.UTC(next.getUTCFullYear(), next.getUTCMonth() + 1, 0)).getUTCDate();
  next.setUTCDate(Math.min(originalDay, lastDay));
  return next;
}

function addOccurrenceInterval(value: Date, meeting: ScheduledMeeting) {
  const interval = Math.max(1, meeting.recurrence?.interval ?? 1);
  const next = new Date(value);
  switch (meeting.recurrence?.frequency) {
    case "daily":
      next.setUTCDate(next.getUTCDate() + interval);
      return next;
    case "weekly":
      next.setUTCDate(next.getUTCDate() + interval * 7);
      return next;
    case "monthly":
      return addMonthsClamped(next, interval, new Date(meeting.starts_at).getUTCDate());
    default:
      return next;
  }
}

function overlaps(start: Date, end: Date, rangeStart: Date, rangeEnd: Date) {
  return start <= rangeEnd && end >= rangeStart;
}

export function occurrencesForDay(meetings: ScheduledMeeting[], day: Date) {
  return expandMeetingOccurrences(meetings, startOfDay(day), endOfDay(day));
}

export function expandMeetingOccurrences(
  meetings: ScheduledMeeting[],
  rangeStart: Date,
  rangeEnd: Date,
): CalendarMeetingOccurrence[] {
  const occurrences: CalendarMeetingOccurrence[] = [];

  for (const meeting of meetings) {
    const masterStart = new Date(meeting.starts_at);
    const masterEnd = new Date(meeting.ends_at);
    const durationMs = masterEnd.getTime() - masterStart.getTime();
    if (durationMs <= 0) continue;

    if (!meeting.recurrence) {
      if (overlaps(masterStart, masterEnd, rangeStart, rangeEnd)) {
        occurrences.push({
          ...meeting,
          occurrence_key: meeting.id,
          series_id: meeting.id,
          is_recurring_occurrence: false,
        });
      }
      continue;
    }

    const recurrenceEnd = meeting.recurrence.until ? new Date(meeting.recurrence.until) : rangeEnd;
    let occurrenceStart = new Date(masterStart);
    let guard = 0;

    while (occurrenceStart <= rangeEnd && occurrenceStart <= recurrenceEnd && guard < 730) {
      const occurrenceEnd = new Date(occurrenceStart.getTime() + durationMs);
      if (overlaps(occurrenceStart, occurrenceEnd, rangeStart, rangeEnd)) {
        occurrences.push({
          ...meeting,
          starts_at: occurrenceStart.toISOString(),
          ends_at: occurrenceEnd.toISOString(),
          occurrence_key: `${meeting.id}:${occurrenceStart.toISOString()}`,
          series_id: meeting.id,
          is_recurring_occurrence: occurrenceStart.getTime() !== masterStart.getTime(),
        });
      }
      const next = addOccurrenceInterval(occurrenceStart, meeting);
      if (next.getTime() <= occurrenceStart.getTime()) break;
      occurrenceStart = next;
      guard += 1;
    }
  }

  return occurrences.sort((a, b) => new Date(a.starts_at).getTime() - new Date(b.starts_at).getTime());
}
