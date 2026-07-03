import { makeCall, paleServerStartMeeting } from "@/lib/tauri";
import { useMeetingStore, type ScheduledMeeting } from "@/store/meetingStore";
import { useUiStore } from "@/store/uiStore";

export function meetingCanJoin(meeting: ScheduledMeeting, now = new Date()) {
  if (meeting.status === "cancelled") return false;
  const startsAt = new Date(meeting.starts_at).getTime();
  const endsAt = new Date(meeting.ends_at).getTime();
  const earlyJoinMs = 15 * 60 * 1000;
  const lateJoinMs = 30 * 60 * 1000;
  const nowMs = now.getTime();
  return nowMs >= startsAt - earlyJoinMs && nowMs <= endsAt + lateJoinMs;
}

export async function joinScheduledMeeting(
  baseUrl: string,
  token: string,
  meeting: Pick<ScheduledMeeting, "id">,
) {
  const target = await paleServerStartMeeting(baseUrl, token, meeting.id);
  useMeetingStore.getState().setActiveConferenceId(target.conference_id);
  useUiStore.getState().setActiveTab("dialpad");
  await makeCall(target.call_uri);
  return target;
}
