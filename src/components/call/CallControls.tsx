import { MicOff, Mic, Pause, Play, Grid3X3, PhoneForwarded, Circle, ParkingSquare } from "lucide-react";
import { cn } from "@/lib/cn";
import { CallControlButton } from "./CallControlButton";

interface CallControlsProps {
  isMuted: boolean;
  isHeld: boolean;
  isRecording: boolean;
  onToggleMute: () => void;
  onToggleHold: () => void;
  onOpenKeypad: () => void;
  onTransfer: () => void;
  onToggleRecord: () => void;
  onParkCall: () => void;
}

export function CallControls({
  isMuted,
  isHeld,
  isRecording,
  onToggleMute,
  onToggleHold,
  onOpenKeypad,
  onTransfer,
  onToggleRecord,
  onParkCall,
}: CallControlsProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-center gap-2 px-4 py-3 mx-4 rounded-2xl",
        "border border-white/[0.06]",
        isHeld ? "bg-warning-muted" : "bg-surface/70 backdrop-blur-xl"
      )}
      style={
        !isHeld
          ? { backdropFilter: "blur(16px)", WebkitBackdropFilter: "blur(16px)" }
          : undefined
      }
    >
      <CallControlButton
        icon={isMuted ? Mic : MicOff}
        label={isMuted ? "Unmute" : "Mute"}
        active={isMuted}
        activeColor="accent"
        onClick={onToggleMute}
      />
      <CallControlButton
        icon={isHeld ? Play : Pause}
        label={isHeld ? "Resume" : "Hold"}
        active={isHeld}
        activeColor="warning"
        onClick={onToggleHold}
      />
      <CallControlButton
        icon={Circle}
        label={isRecording ? "Stop Rec" : "Record"}
        active={isRecording}
        activeColor="destructive"
        onClick={onToggleRecord}
      />
      <CallControlButton
        icon={Grid3X3}
        label="Keypad"
        onClick={onOpenKeypad}
      />
      <CallControlButton
        icon={ParkingSquare}
        label="Park"
        onClick={onParkCall}
      />
      <CallControlButton
        icon={PhoneForwarded}
        label="Transfer"
        disabled={isHeld}
        onClick={onTransfer}
      />
    </div>
  );
}
