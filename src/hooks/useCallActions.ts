import * as ipc from "@/lib/tauri";
import { useCallStore } from "@/store/callStore";
import { toast } from "@/components/ui/Toast";

/**
 * Shared call actions that drive the real SIP engine via Tauri IPC and keep
 * the call store in sync. Used by both the call UI (ActiveCallView,
 * IncomingCallOverlay) and keyboard shortcuts so every path actually
 * answers/hangs up/mutes on the wire — never just the UI state.
 *
 * Every optimistic store update is rolled back (with an error toast) if the
 * engine rejects the command.
 */

function getSession(sessionId: number) {
  return useCallStore.getState().sessions.find((s) => s.id === sessionId);
}

/** Toggle microphone mute for a call. Rolls back on engine failure. */
export function toggleMute(sessionId: number) {
  const session = getSession(sessionId);
  if (!session) return;
  const { setMuted } = useCallStore.getState();
  const newMuted = !session.isMuted;
  setMuted(sessionId, newMuted);
  ipc.setMute(sessionId, newMuted).catch((err) => {
    setMuted(sessionId, !newMuted);
    toast({
      type: "error",
      title: newMuted ? "Failed to mute" : "Failed to unmute",
      description: String(err),
    });
  });
}

/** Toggle hold for a call. Rolls back on engine failure. */
export function toggleHold(sessionId: number) {
  const session = getSession(sessionId);
  if (!session) return;
  const { setHeld, updateSessionState } = useCallStore.getState();
  const newHeld = !session.isHeld;
  const prevState = session.state;
  setHeld(sessionId, newHeld);
  updateSessionState(sessionId, newHeld ? "on_hold" : "connected");
  (newHeld ? ipc.holdCall(sessionId) : ipc.unholdCall(sessionId)).catch((err) => {
    setHeld(sessionId, !newHeld);
    updateSessionState(sessionId, prevState);
    toast({
      type: "error",
      title: newHeld ? "Failed to hold call" : "Failed to resume call",
      description: String(err),
    });
  });
}

/**
 * Hang up a call. Stops an active recording first, and only removes the
 * session from the UI once the engine accepts the hangup — if it fails, the
 * call UI stays visible so the user knows the call (and mic) is still live.
 */
export function hangupCall(sessionId: number) {
  const session = getSession(sessionId);
  if (!session) return;
  const { setRecording } = useCallStore.getState();
  // Stop recording if active before hanging up
  if (session.isRecording) {
    ipc.stopRecording(sessionId).catch(() => {});
    setRecording(sessionId, false);
  }
  ipc.hangupCall(sessionId)
    .then(() => {
      useCallStore.getState().updateSessionState(sessionId, "terminated");
      setTimeout(() => useCallStore.getState().removeSession(sessionId), 300);
    })
    .catch((err) => {
      toast({ type: "error", title: "Failed to end call", description: String(err) });
    });
}

/** Answer the current incoming call. Rolls back the session on engine failure. */
export function answerIncomingCall() {
  const { incomingCall, addSession, setActiveCallId, setIncomingCall } =
    useCallStore.getState();
  if (!incomingCall) return;
  const call = incomingCall;
  ipc.answerCall(call.id).catch((err) => {
    toast({ type: "error", title: "Failed to answer call", description: String(err) });
    useCallStore.getState().removeSession(call.id);
    useCallStore.getState().setActiveCallId(null);
  });
  addSession({ ...call, state: "connected", connectTime: Date.now() });
  setActiveCallId(call.id);
  setIncomingCall(null);
}

/** Reject (hang up) the current incoming call on the wire and dismiss the overlay. */
export function rejectIncomingCall() {
  const { incomingCall, setIncomingCall } = useCallStore.getState();
  if (!incomingCall) return;
  ipc.hangupCall(incomingCall.id).catch((err) => {
    toast({ type: "error", title: "Failed to reject call", description: String(err) });
  });
  setIncomingCall(null);
}

/** Hook-style accessor for components. The actions read store state lazily, so they are stable. */
export function useCallActions() {
  return {
    toggleMute,
    toggleHold,
    hangupCall,
    answerIncomingCall,
    rejectIncomingCall,
  } as const;
}
