import { useState, useCallback } from "react";
import { PhoneOff } from "lucide-react";
import { motion } from "framer-motion";
import { cn } from "@/lib/cn";
import { CallerAvatar } from "./CallerAvatar";
import { CallTimer } from "./CallTimer";
import { CallControls } from "./CallControls";
import { DtmfOverlay } from "./DtmfOverlay";
import * as ipc from "@/lib/tauri";
import { TransferPanel } from "@/components/transfer/TransferPanel";
import { useCallStore } from "@/store/callStore";
import { Badge } from "@/components/ui/Badge";
import { CallLineIndicator } from "./CallLineIndicator";

export function ActiveCallView() {
  const { sessions, activeCallId, setMuted, setHeld, setRecording, removeSession, updateSessionState } =
    useCallStore();
  const session = sessions.find((s) => s.id === activeCallId);

  const [showDtmf, setShowDtmf] = useState(false);
  const [showTransfer, setShowTransfer] = useState(false);
  const [consultationTarget, setConsultationTarget] = useState<string | null>(null);

  const handleToggleMute = useCallback(() => {
    if (!session) return;
    const newMuted = !session.isMuted;
    setMuted(session.id, newMuted);
    ipc.setMute(session.id, newMuted).catch(() => {});
  }, [session, setMuted]);

  const handleToggleHold = useCallback(() => {
    if (!session) return;
    const newHeld = !session.isHeld;
    setHeld(session.id, newHeld);
    updateSessionState(session.id, newHeld ? "on_hold" : "connected");
    (newHeld ? ipc.holdCall(session.id) : ipc.unholdCall(session.id)).catch(() => {});
  }, [session, setHeld, updateSessionState]);

  const handleToggleRecord = useCallback(() => {
    if (!session) return;
    if (session.isRecording) {
      ipc.stopRecording(session.id).catch(() => {});
      setRecording(session.id, false);
    } else {
      ipc.startRecording(session.id)
        .then(() => setRecording(session.id, true))
        .catch(() => {});
    }
  }, [session, setRecording]);

  const handleHangup = useCallback(() => {
    if (!session) return;
    // Stop recording if active before hanging up
    if (session.isRecording) {
      ipc.stopRecording(session.id).catch(() => {});
    }
    ipc.hangupCall(session.id).catch(() => {});
    updateSessionState(session.id, "terminated");
    setTimeout(() => removeSession(session.id), 300);
  }, [session, removeSession, updateSessionState]);

  const handleDtmf = useCallback((digit: string) => {
    if (!session) return;
    ipc.sendDtmf(session.id, digit).catch(() => {});
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
    <div className="relative flex flex-col items-center justify-between h-full px-6 py-6">
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
            if (session) ipc.blindTransfer(session.id, target).catch(() => {});
            setShowTransfer(false);
          }}
          onAttendedTransfer={(target) => {
            if (!session) return;
            // Step 1: Hold the current call
            setHeld(session.id, true);
            updateSessionState(session.id, "on_hold");
            ipc.holdCall(session.id).catch(() => {});
            // Step 2: Initiate consultation call to target
            setConsultationTarget(target);
            ipc.makeCall(target).catch(() => {});
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
              ipc.attendedTransfer(originalCall.id, consultCall.id).catch(() => {});
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
  );
}
