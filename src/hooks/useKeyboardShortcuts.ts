import { useEffect } from "react";
import { useUiStore } from "@/store/uiStore";
import { useCallStore } from "@/store/callStore";
import {
  answerIncomingCall,
  rejectIncomingCall,
  hangupCall,
  toggleMute,
  toggleHold,
} from "@/hooks/useCallActions";

interface ShortcutHandlers {
  onOpenCommandPalette: () => void;
  onOpenSearch?: () => void;
}

export function useKeyboardShortcuts({ onOpenCommandPalette, onOpenSearch }: ShortcutHandlers) {
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const { activeCallId, sessions, incomingCall } = useCallStore();

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const target = e.target as HTMLElement;
      const isInput =
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable;

      // Cmd+K — Command palette (always active)
      if (meta && e.key === "k") {
        e.preventDefault();
        onOpenCommandPalette();
        return;
      }

      // Cmd+F — Search
      if (meta && e.key === "f") {
        e.preventDefault();
        onOpenSearch?.();
        return;
      }

      // Cmd+, — Settings
      if (meta && e.key === ",") {
        e.preventDefault();
        setActiveTab("settings");
        return;
      }

      // Cmd+D — Focus dialpad
      if (meta && e.key === "d") {
        e.preventDefault();
        setActiveTab("dialpad");
        return;
      }

      // Don't capture in input fields for the rest
      if (isInput) return;

      const activeSession = sessions.find((s) => s.id === activeCallId);

      // M — Toggle mute (during call) — drives the SIP engine, not just UI state
      if (e.key === "m" || e.key === "M") {
        if (activeSession && activeSession.state === "connected") {
          e.preventDefault();
          toggleMute(activeSession.id);
        }
        return;
      }

      // H — Toggle hold (during call)
      if (e.key === "h" || e.key === "H") {
        if (activeSession && (activeSession.state === "connected" || activeSession.state === "on_hold")) {
          e.preventDefault();
          toggleHold(activeSession.id);
        }
        return;
      }

      // Enter — Answer incoming call (via ipc.answerCall, same path as the Accept button)
      if (e.key === "Enter") {
        if (incomingCall) {
          e.preventDefault();
          answerIncomingCall();
        }
        return;
      }

      // Escape — Reject incoming / hang up active call (on the wire)
      if (e.key === "Escape") {
        if (incomingCall) {
          e.preventDefault();
          rejectIncomingCall();
          return;
        }
        if (activeSession) {
          e.preventDefault();
          hangupCall(activeSession.id);
        }
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    activeCallId, sessions, incomingCall,
    setActiveTab, onOpenCommandPalette, onOpenSearch,
  ]);
}
