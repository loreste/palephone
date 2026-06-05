import { useState, useCallback } from "react";
import {
  PhoneOff, Mic, MicOff, Video, VideoOff,
  Monitor, Pause, Play,
} from "lucide-react";
import { motion } from "framer-motion";
import { cn } from "@/lib/cn";
import { CallerAvatar } from "@/components/call/CallerAvatar";
import { CallTimer } from "@/components/call/CallTimer";
import { Badge } from "@/components/ui/Badge";
import { useCallStore } from "@/store/callStore";
import * as ipc from "@/lib/tauri";
import { invoke } from "@tauri-apps/api/core";

/**
 * Video call view — displays remote and self video with call controls.
 * Since native video rendering requires platform-specific code,
 * this view shows the call state and controls; actual video frames
 * will be rendered in a native overlay window by PJSIP.
 */
export function VideoCallView() {
  const { sessions, activeCallId, setMuted, setHeld, updateSessionState, removeSession } =
    useCallStore();
  const session = sessions.find((s) => s.id === activeCallId);

  const [videoEnabled, setVideoEnabled] = useState(true);
  const [screenSharing, setScreenSharing] = useState(false);

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

  const handleToggleVideo = useCallback(() => {
    if (!session) return;
    const newEnabled = !videoEnabled;
    setVideoEnabled(newEnabled);
    invoke("toggle_video", { callId: session.id, enabled: newEnabled }).catch(() => {});
  }, [session, videoEnabled]);

  const handleHangup = useCallback(() => {
    if (!session) return;
    ipc.hangupCall(session.id).catch(() => {});
    updateSessionState(session.id, "terminated");
    setTimeout(() => removeSession(session.id), 300);
  }, [session, removeSession, updateSessionState]);

  if (!session) return null;

  const stateLabel = session.state === "connected"
    ? "Video Call"
    : session.state === "dialing"
      ? "Calling..."
      : session.state === "on_hold"
        ? "On Hold"
        : "Ringing...";

  return (
    <div className="flex flex-col h-full bg-base">
      {/* Video area — placeholder for native rendering */}
      <div className="flex-1 relative flex items-center justify-center bg-base">
        {/* Remote video placeholder */}
        <div className="flex flex-col items-center gap-4">
          <CallerAvatar name={session.remoteName || session.remoteUri} size="lg" />
          <h2 className="text-xl font-semibold text-primary">
            {session.remoteName || "Unknown"}
          </h2>
          <CallTimer connectTime={session.connectTime} className="text-lg" />
          <Badge variant={session.state === "connected" ? "success" : "accent"}>
            {stateLabel}
          </Badge>
          {videoEnabled && session.state === "connected" && (
            <p className="text-xs text-tertiary">
              Video is being rendered in a native overlay window
            </p>
          )}
        </div>

        {/* Self view (PiP) — placeholder */}
        {videoEnabled && (
          <div
            className={cn(
              "absolute bottom-4 right-4 w-32 h-24 rounded-lg",
              "bg-surface border border-border-subtle",
              "flex items-center justify-center"
            )}
          >
            <Video size={20} className="text-tertiary" />
          </div>
        )}
      </div>

      {/* Video call controls */}
      <div className="flex items-center justify-center gap-3 py-4 px-6 bg-surface border-t border-border-subtle">
        <ControlButton
          icon={session.isMuted ? Mic : MicOff}
          label={session.isMuted ? "Unmute" : "Mute"}
          active={session.isMuted}
          onClick={handleToggleMute}
        />
        <ControlButton
          icon={videoEnabled ? VideoOff : Video}
          label={videoEnabled ? "Stop Video" : "Start Video"}
          active={!videoEnabled}
          onClick={handleToggleVideo}
        />
        <ControlButton
          icon={Monitor}
          label="Share Screen"
          active={screenSharing}
          onClick={() => setScreenSharing(!screenSharing)}
        />
        <ControlButton
          icon={session.isHeld ? Play : Pause}
          label={session.isHeld ? "Resume" : "Hold"}
          active={session.isHeld}
          activeColor="warning"
          onClick={handleToggleHold}
        />

        {/* Hangup */}
        <motion.button
          whileTap={{ scale: 0.9 }}
          onClick={handleHangup}
          className={cn(
            "flex items-center justify-center",
            "w-12 h-12 rounded-full",
            "bg-destructive text-white",
            "hover:bg-destructive-hover transition-colors"
          )}
          aria-label="End call"
        >
          <PhoneOff size={20} />
        </motion.button>
      </div>
    </div>
  );
}

function ControlButton({
  icon: Icon,
  label,
  active = false,
  activeColor = "accent",
  onClick,
}: {
  icon: typeof Mic;
  label: string;
  active?: boolean;
  activeColor?: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      className={cn(
        "flex items-center justify-center w-12 h-12 rounded-full transition-colors",
        active
          ? activeColor === "warning"
            ? "bg-warning text-inverse"
            : "bg-accent text-white"
          : "bg-elevated text-secondary hover:bg-overlay"
      )}
    >
      <Icon size={20} />
    </button>
  );
}
