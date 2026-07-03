import { describe, expect, it } from "vitest";
import { expandMeetingOccurrences, occurrencesForDay } from "@/lib/calendarOccurrences";
import type { ScheduledMeeting } from "@/store/meetingStore";

function meeting(overrides: Partial<ScheduledMeeting> = {}): ScheduledMeeting {
  return {
    id: "meeting-1",
    title: "Planning",
    description: "",
    organizer_uri: "sip:alice@example.com",
    room_id: null,
    conference_id: "conference-1",
    participants: ["sip:alice@example.com", "sip:bob@example.com"],
    starts_at: "2026-07-06T14:00:00.000Z",
    ends_at: "2026-07-06T15:00:00.000Z",
    recurrence: null,
    status: "scheduled",
    cancelled_at: null,
    updated_at: null,
    created_at: "2026-07-01T12:00:00.000Z",
    ...overrides,
  };
}

describe("calendar occurrences", () => {
  it("returns one-off meetings on their scheduled day", () => {
    const occurrences = occurrencesForDay([meeting()], new Date("2026-07-06T12:00:00.000Z"));

    expect(occurrences).toHaveLength(1);
    expect(occurrences[0].occurrence_key).toBe("meeting-1");
    expect(occurrences[0].is_recurring_occurrence).toBe(false);
  });

  it("expands weekly recurring meetings within the visible range", () => {
    const occurrences = expandMeetingOccurrences(
      [
        meeting({
          recurrence: {
            frequency: "weekly",
            interval: 1,
            until: "2026-07-27T23:59:59.000Z",
          },
        }),
      ],
      new Date("2026-07-01T00:00:00.000Z"),
      new Date("2026-07-31T23:59:59.999Z"),
    );

    expect(occurrences.map((item) => item.starts_at)).toEqual([
      "2026-07-06T14:00:00.000Z",
      "2026-07-13T14:00:00.000Z",
      "2026-07-20T14:00:00.000Z",
      "2026-07-27T14:00:00.000Z",
    ]);
  });

  it("clamps monthly recurring meetings to valid month days", () => {
    const occurrences = expandMeetingOccurrences(
      [
        meeting({
          starts_at: "2026-01-31T14:00:00.000Z",
          ends_at: "2026-01-31T15:00:00.000Z",
          recurrence: {
            frequency: "monthly",
            interval: 1,
            until: "2026-03-31T23:59:59.000Z",
          },
        }),
      ],
      new Date("2026-01-01T00:00:00.000Z"),
      new Date("2026-03-31T23:59:59.999Z"),
    );

    expect(occurrences.map((item) => item.starts_at)).toEqual([
      "2026-01-31T14:00:00.000Z",
      "2026-02-28T14:00:00.000Z",
      "2026-03-31T14:00:00.000Z",
    ]);
  });
});
