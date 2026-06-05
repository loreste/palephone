import { useEffect } from "react";
import { useUiStore } from "@/store/uiStore";
import { useCallStore } from "@/store/callStore";

interface ShortcutHandlers {
  onOpenCommandPalette: () => void;
}

export function useKeyboardShortcuts({ onOpenCommandPalette }: ShortcutHandlers) {
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const { activeCallId, sessions, incomingCall, setMuted, setHeld, updateSessionState, removeSession, setIncomingCall, addSession, setActiveCallId } =
    useCallStore();

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

      // M — Toggle mute (during call)
      if (e.key === "m" || e.key === "M") {
        if (activeSession && activeSession.state === "connected") {
          e.preventDefault();
          setMuted(activeSession.id, !activeSession.isMuted);
        }
        return;
      }

      // H — Toggle hold (during call)
      if (e.key === "h" || e.key === "H") {
        if (activeSession && (activeSession.state === "connected" || activeSession.state === "on_hold")) {
          e.preventDefault();
          const newHeld = !activeSession.isHeld;
          setHeld(activeSession.id, newHeld);
          updateSessionState(activeSession.id, newHeld ? "on_hold" : "connected");
        }
        return;
      }

      // Enter — Answer incoming call
      if (e.key === "Enter") {
        if (incomingCall) {
          e.preventDefault();
          addSession({ ...incomingCall, state: "connected", connectTime: Date.now() });
          setActiveCallId(incomingCall.id);
          setIncomingCall(null);
        }
        return;
      }

      // Escape — Hangup / reject / back
      if (e.key === "Escape") {
        if (incomingCall) {
          e.preventDefault();
          setIncomingCall(null);
          return;
        }
        if (activeSession) {
          e.preventDefault();
          updateSessionState(activeSession.id, "terminated");
          setTimeout(() => removeSession(activeSession.id), 300);
        }
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    activeCallId, sessions, incomingCall,
    setActiveTab, setMuted, setHeld, updateSessionState,
    removeSession, setIncomingCall, addSession, setActiveCallId,
    onOpenCommandPalette,
  ]);
}
