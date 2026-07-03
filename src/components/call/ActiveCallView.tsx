import { useState, useCallback } from "react";
import { PhoneOff, Hand, PanelRightOpen, Captions } from "lucide-react";
import { motion } from "framer-motion";
import { cn } from "@/lib/cn";
import { CallerAvatar } from "./CallerAvatar";
import { CallTimer } from "./CallTimer";
import { CallControls } from "./CallControls";
import { DtmfOverlay } from "./DtmfOverlay";
import * as ipc from "@/lib/tauri";
import { TransferPanel } from "@/components/transfer/TransferPanel";
import { useCallStore } from "@/store/callStore";
import { useCallActions } from "@/hooks/useCallActions";
import { Badge } from "@/components/ui/Badge";
import { toast } from "@/components/ui/Toast";
import { CallLineIndicator } from "./CallLineIndicator";
import { MeetingPanel } from "@/components/meeting/MeetingPanel";
import { useMeetingStore } from "@/store/meetingStore";
import { useServerStore } from "@/store/serverStore";
import { paleServerApi } from "@/lib/tauri";

export function ActiveCallView() {
  const { sessions, activeCallId, setHeld, setRecording, removeSession, updateSessionState } =
    useCallStore();
  const session = sessions.find((s) => s.id === activeCallId);
  const { toggleMute, toggleHold, hangupCall } = useCallActions();

  const [showDtmf, setShowDtmf] = useState(false);
  const [showTransfer, setShowTransfer] = useState(false);
  const [showMeetingPanel, setShowMeetingPanel] = useState(false);
  const [consultationTarget, setConsultationTarget] = useState<string | null>(null);
  const activeConferenceId = useMeetingStore((s) => s.activeConferenceId);
  const raisedHands = useMeetingStore((s) => s.raisedHands);
  const captionsEnabled = useMeetingStore((s) => s.captionsEnabled);
  const setCaptionsEnabled = useMeetingStore((s) => s.setCaptionsEnabled);
  const captions = useMeetingStore((s) => s.captions);
  const baseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);

  const handleToggleMute = useCallback(() => {
    if (!session) return;
    toggleMute(session.id);
  }, [session, toggleMute]);

  const handleToggleHold = useCallback(() => {
    if (!session) return;
    toggleHold(session.id);
  }, [session, toggleHold]);

  const handleToggleRecord = useCallback(() => {
    if (!session) return;
    if (session.isRecording) {
      ipc.stopRecording(session.id)
        .then(() => setRecording(session.id, false))
        .catch((err) => toast({ type: "error", title: "Failed to stop recording", description: String(err) }));
    } else {
      ipc.startRecording(session.id)
        .then(() => setRecording(session.id, true))
        .catch((err) => toast({ type: "error", title: "Failed to start recording", description: String(err) }));
    }
  }, [session, setRecording]);

  const handleHangup = useCallback(() => {
    if (!session) return;
    hangupCall(session.id);
  }, [session, hangupCall]);

  const handleParkCall = useCallback(async () => {
    if (!session) return;
    // Park the call using blind transfer to an auto-assigned park slot
    const slot = `sip:park-${701 + (session.id % 99)}@pale.local`;
    try {
      await ipc.blindTransfer(session.id, slot);
      toast({ type: "success", title: `Call parked in slot ${701 + (session.id % 99)}` });
    } catch {
      toast({ type: "error", title: "Failed to park call" });
      return;
    }
    updateSessionState(session.id, "terminated");
    setTimeout(() => removeSession(session.id), 300);
  }, [session, removeSession, updateSessionState]);

  const handleDtmf = useCallback((digit: string) => {
    if (!session) return;
    ipc.sendDtmf(session.id, digit).catch((err) =>
      toast({ type: "error", title: "Failed to send digit", description: String(err) })
    );
  }, [session]);

  if (!session) return null;

  const stateLabel =
    session.state === "dialing"
      ? "Dialing..."
      : session.state === "ringing"
        ? "Ringing..."
        : session.state === "on_hold"
          ? consultationTarget ? "On Hold — Consulting..." : "On Hold"
          : session.state === "transferring"
            ? "Transferring..."
            : "Connected";

  const stateBadgeVariant =
    session.state === "connected"
      ? "success"
      : session.state === "on_hold"
        ? "warning"
        : "accent";

  return (
    <div className="flex h-full">
    <div className="relative flex flex-col items-center justify-between flex-1 px-6 py-6">
      {/* Multi-line indicator */}
      <CallLineIndicator />

      {/* Caller info */}
      <div className="flex flex-col items-center gap-3 pt-6">
        <motion.div
          initial={{ scale: 0.9, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          transition={{ type: "spring", stiffness: 300, damping: 25 }}
        >
          <CallerAvatar name={session.remoteName || session.remoteUri} size="lg" />
        </motion.div>

        <div className="text-center">
          <h2 className="text-xl font-semibold text-primary">
            {session.remoteName || "Unknown"}
          </h2>
          <p className="text-sm text-tertiary font-mono mt-0.5">
            {session.remoteUri}
          </p>
        </div>

        <CallTimer connectTime={session.connectTime} />

        <Badge variant={stateBadgeVariant as any}>{stateLabel}</Badge>
        {session.isRecording && (
          <Badge variant="destructive">
            <span className="inline-block w-2 h-2 rounded-full bg-white animate-pulse mr-1.5" />
            Recording
          </Badge>
        )}
      </div>

      {/* Transfer panel or controls */}
      {showTransfer ? (
        <TransferPanel
          onClose={() => setShowTransfer(false)}
          onBlindTransfer={(target) => {
            if (session) {
              ipc.blindTransfer(session.id, target).catch((err) =>
                toast({ type: "error", title: "Transfer failed", description: String(err) })
              );
            }
            setShowTransfer(false);
          }}
          onAttendedTransfer={(target) => {
            if (!session) return;
            const callId = session.id;
            // Step 1: Hold the current call
            setHeld(callId, true);
            updateSessionState(callId, "on_hold");
            ipc.holdCall(callId).catch((err) => {
              setHeld(callId, false);
              updateSessionState(callId, "connected");
              toast({ type: "error", title: "Failed to hold call", description: String(err) });
            });
            // Step 2: Initiate consultation call to target
            setConsultationTarget(target);
            ipc.makeCall(target).catch((err) => {
              setConsultationTarget(null);
              toast({ type: "error", title: "Consultation call failed", description: String(err) });
            });
            setShowTransfer(false);
          }}
        />
      ) : (
        <CallControls
          isMuted={session.isMuted}
          isHeld={session.isHeld}
          isRecording={session.isRecording}
          onToggleMute={handleToggleMute}
          onToggleHold={handleToggleHold}
          onToggleRecord={handleToggleRecord}
          onOpenKeypad={() => setShowDtmf(!showDtmf)}
          onParkCall={handleParkCall}
          onTransfer={() => setShowTransfer(true)}
        />
      )}

      {/* DTMF overlay */}
      <DtmfOverlay
        open={showDtmf}
        onClose={() => setShowDtmf(false)}
        onDigit={handleDtmf}
      />

      {/* Attended transfer: Complete Transfer button when consultation call is active */}
      {consultationTarget && sessions.length >= 2 && (
        <button
          onClick={() => {
            // Find the consultation call (the one that's not the original held call)
            const originalCall = sessions.find((s) => s.isHeld);
            const consultCall = sessions.find((s) => s.id !== originalCall?.id && s.state === "connected");
            if (originalCall && consultCall) {
              ipc.attendedTransfer(originalCall.id, consultCall.id).catch((err) =>
                toast({ type: "error", title: "Transfer failed", description: String(err) })
              );
              setConsultationTarget(null);
              // Both calls will be terminated by PJSIP after successful transfer
            }
          }}
          className={cn(
            "w-full max-w-[200px] h-[40px] rounded-full",
            "bg-success text-white font-semibold text-sm",
            "hover:bg-success/90 transition-colors mb-2"
          )}
        >
          Complete Transfer
        </button>
      )}

      {/* Meeting controls: raise hand, captions, meeting panel */}
      {activeConferenceId && (
        <div className="flex items-center gap-2 mb-2">
          <button
            onClick={async () => {
              if (!baseUrl || !serverToken) return;
              try {
                await paleServerApi(baseUrl, serverToken, `/v1/conferences/${activeConferenceId}/hands`, {
                  method: "POST",
                  body: { user_id: "00000000-0000-0000-0000-000000000000", sip_uri: session?.remoteUri ?? "", raised: true },
                });
              } catch { /* ignore */ }
            }}
            className={cn(
              "flex items-center gap-1 px-3 py-2 rounded-full text-sm",
              raisedHands.length > 0 ? "bg-yellow-500/20 text-yellow-500" : "bg-hover text-secondary hover:text-primary"
            )}
          >
            <Hand size={16} /> Raise Hand
          </button>
          <button
            onClick={() => setCaptionsEnabled(!captionsEnabled)}
            className={cn(
              "flex items-center gap-1 px-3 py-2 rounded-full text-sm",
              captionsEnabled ? "bg-accent/20 text-accent" : "bg-hover text-secondary hover:text-primary"
            )}
          >
            <Captions size={16} /> Captions
          </button>
          <button
            onClick={() => setShowMeetingPanel(!showMeetingPanel)}
            className="flex items-center gap-1 px-3 py-2 rounded-full text-sm bg-hover text-secondary hover:text-primary"
          >
            <PanelRightOpen size={16} /> Meeting
          </button>
        </div>
      )}

      {/* Live captions overlay */}
      {captionsEnabled && captions.length > 0 && (
        <div className="w-full max-w-md mb-2 p-2 bg-black/80 rounded-lg text-white text-sm max-h-[100px] overflow-y-auto">
          {captions.slice(-3).map((c) => (
            <div key={c.id}>
              <span className="font-medium text-accent">{c.speaker_name || c.speaker_uri.replace(/^sip:/, "")}:</span>{" "}
              {c.text}
            </div>
          ))}
        </div>
      )}

      {/* Hangup button */}
      <motion.button
        whileTap={{ scale: 0.9 }}
        transition={{ type: "spring", stiffness: 400, damping: 20 }}
        onClick={handleHangup}
        className={cn(
          "flex items-center justify-center gap-2",
          "w-full max-w-[200px] h-[48px] rounded-full",
          "bg-destructive text-white font-semibold",
          "hover:bg-destructive-hover transition-colors",
          "shadow-md"
        )}
        style={{ boxShadow: "0 0 16px rgba(239, 68, 68, 0.3)" }}
        aria-label="End call"
      >
        <PhoneOff size={20} />
        <span>End Call</span>
      </motion.button>
    </div>
    {showMeetingPanel && activeConferenceId && (
      <MeetingPanel conferenceId={activeConferenceId} />
    )}
    </div>
  );
}
