import { describe, expect, it } from "vitest";
import { meetingCanJoin } from "@/lib/meetingJoin";
import type { ScheduledMeeting } from "@/store/meetingStore";

function meeting(overrides: Partial<ScheduledMeeting> = {}): ScheduledMeeting {
  return {
    id: "meeting-1",
    title: "Planning",
    description: "",
    organizer_uri: "sip:alice@example.com",
    room_id: null,
    conference_id: "conference-1",
    participants: ["sip:alice@example.com"],
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

describe("meeting join window", () => {
  it("allows joining fifteen minutes before start", () => {
    expect(meetingCanJoin(meeting(), new Date("2026-07-06T13:45:00.000Z"))).toBe(true);
  });

  it("blocks joining too early", () => {
    expect(meetingCanJoin(meeting(), new Date("2026-07-06T13:44:59.000Z"))).toBe(false);
  });

  it("keeps recently ended meetings joinable", () => {
    expect(meetingCanJoin(meeting(), new Date("2026-07-06T15:30:00.000Z"))).toBe(true);
  });

  it("blocks cancelled meetings", () => {
    expect(meetingCanJoin(meeting({ status: "cancelled" }), new Date("2026-07-06T14:00:00.000Z"))).toBe(false);
  });
});
