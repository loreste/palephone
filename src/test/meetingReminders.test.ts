import { describe, expect, it } from "vitest";
import { shouldSendMeetingReminder } from "@/hooks/useMeetingReminders";
import type { CalendarMeetingOccurrence } from "@/lib/calendarOccurrences";

function occurrence(overrides: Partial<CalendarMeetingOccurrence> = {}): CalendarMeetingOccurrence {
  return {
    id: "meeting-1",
    title: "Planning",
    description: "",
    organizer_uri: "sip:alice@example.com",
    room_id: null,
    conference_id: "conference-1",
    participants: ["sip:alice@example.com"],
    starts_at: "2026-07-06T14:10:00.000Z",
    ends_at: "2026-07-06T15:10:00.000Z",
    recurrence: null,
    status: "scheduled",
    cancelled_at: null,
    updated_at: null,
    created_at: "2026-07-01T12:00:00.000Z",
    occurrence_key: "meeting-1:2026-07-06T14:10:00.000Z",
    series_id: "meeting-1",
    is_recurring_occurrence: false,
    ...overrides,
  };
}

describe("meeting reminders", () => {
  it("sends one reminder for upcoming meetings in the lookahead window", () => {
    const sent = new Set<string>();
    const result = shouldSendMeetingReminder(
      occurrence(),
      new Date("2026-07-06T14:00:00.000Z"),
      sent,
    );

    expect(result?.label).toBe("starts in 15 minutes");
    expect(result?.key).toContain(":15m");
  });

  it("suppresses duplicate reminders for the same occurrence", () => {
    const sent = new Set(["meeting-1:2026-07-06T14:10:00.000Z:15m"]);
    const result = shouldSendMeetingReminder(
      occurrence(),
      new Date("2026-07-06T14:00:00.000Z"),
      sent,
    );

    expect(result).toBeNull();
  });

  it("does not remind for cancelled meetings", () => {
    const result = shouldSendMeetingReminder(
      occurrence({ status: "cancelled" }),
      new Date("2026-07-06T14:00:00.000Z"),
      new Set(),
    );

    expect(result).toBeNull();
  });
});
