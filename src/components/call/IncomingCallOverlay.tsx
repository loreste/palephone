import { useEffect, useRef } from "react";
import { Phone, PhoneOff } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { cn } from "@/lib/cn";
import { CallerAvatar } from "./CallerAvatar";
import { useCallStore } from "@/store/callStore";
import { useCallActions } from "@/hooks/useCallActions";
import { playRingtone } from "@/lib/notificationSound";

export function IncomingCallOverlay() {
  const incomingCall = useCallStore((s) => s.incomingCall);
  const { answerIncomingCall, rejectIncomingCall } = useCallActions();
  const stopRingtoneRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (incomingCall) {
      stopRingtoneRef.current = playRingtone();
    } else if (stopRingtoneRef.current) {
      stopRingtoneRef.current();
      stopRingtoneRef.current = null;
    }
    return () => {
      if (stopRingtoneRef.current) {
        stopRingtoneRef.current();
        stopRingtoneRef.current = null;
      }
    };
  }, [incomingCall]);

  const handleAccept = () => answerIncomingCall();
  const handleReject = () => rejectIncomingCall();

  return (
    <AnimatePresence>
      {incomingCall && (
        <>
          {/* Backdrop */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 z-40 bg-base/60 backdrop-blur-sm"
          />

          {/* Panel */}
          <motion.div
            initial={{ opacity: 0, y: "100%" }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: "100%" }}
            transition={{ duration: 0.35, ease: [0.16, 1, 0.3, 1] }}
            className={cn(
              "fixed inset-x-0 bottom-0 z-50",
              "flex flex-col items-center",
              "bg-surface/95 backdrop-blur-2xl",
              "border-t border-white/[0.06]",
              "rounded-t-3xl px-6 pt-8 pb-10",
              "shadow-lg"
            )}
          >
            {/* Ring pulse animation */}
            <div className="relative mb-4">
              <PulseRings />
              <CallerAvatar
                name={incomingCall.remoteName || incomingCall.remoteUri}
                size="lg"
              />
            </div>

            {/* Caller info */}
            <p className="text-xs font-semibold text-accent uppercase tracking-widest mb-2">
              Incoming Call
            </p>
            <h2 className="text-xl font-semibold text-primary">
              {incomingCall.remoteName || "Unknown"}
            </h2>
            <p className="text-sm text-tertiary font-mono mt-1 mb-8">
              {incomingCall.remoteUri}
            </p>

            {/* Accept / Reject buttons */}
            <div className="flex items-center gap-6">
              <motion.button
                whileTap={{ scale: 0.9 }}
                onClick={handleReject}
                className={cn(
                  "flex items-center justify-center gap-2",
                  "h-14 px-8 rounded-full",
                  "bg-destructive text-white font-semibold",
                  "hover:bg-destructive-hover transition-colors"
                )}
                aria-label="Reject call"
              >
                <PhoneOff size={20} />
                <span>Reject</span>
              </motion.button>

              <motion.button
                whileTap={{ scale: 0.9 }}
                onClick={handleAccept}
                className={cn(
                  "flex items-center justify-center gap-2",
                  "h-14 px-8 rounded-full",
                  "bg-success text-white font-semibold",
                  "hover:brightness-110 transition-all"
                )}
                style={{ boxShadow: "0 0 20px rgba(34, 197, 94, 0.35)" }}
                aria-label="Accept call"
              >
                <Phone size={20} fill="currentColor" />
                <span>Accept</span>
              </motion.button>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

/** Three expanding, fading concentric rings around the avatar */
function PulseRings() {
  return (
    <div className="absolute inset-0 flex items-center justify-center" aria-hidden>
      {[0, 0.5, 1].map((delay) => (
        <span
          key={delay}
          className="absolute w-24 h-24 rounded-full border-2 border-accent/40"
          style={{
            animation: `ring-pulse 1.5s ease-out ${delay}s infinite`,
          }}
        />
      ))}
    </div>
  );
}
