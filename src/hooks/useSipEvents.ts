import { useEffect } from "react";
import {
  onRegState,
  onIncomingCall,
  onCallState,
  onPaleError,
  addCallRecord,
  paleServerSyncCallHistory,
} from "@/lib/tauri";
import { useAccountStore } from "@/store/accountStore";
import { useCallStore } from "@/store/callStore";
import { useServerStore } from "@/store/serverStore";
import { shouldNotify } from "@/lib/notifications";
import { toast } from "@/components/ui/Toast";
import type { RegState, CallState } from "@/types";

/**
 * Hook that listens to all SIP events from the Rust backend
 * and updates the Zustand stores accordingly.
 * Should be mounted once at the app root.
 */
export function useSipEvents() {
  const setRegState = useAccountStore((s) => s.setRegState);
  const {
    addSession,
    updateSessionState,
    setIncomingCall,
    setActiveCallId,
    setConnectTime,
    removeSession,
  } = useCallStore();

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    // Registration state changes
    unlisteners.push(
      onRegState((event) => {
        const state = mapRegState(event.state);
        setRegState(state, event.reason || null);

        if (state === "registered") {
          toast({ type: "success", title: "Registered", description: event.reason });
        } else if (state === "unregistered" && event.reason) {
          toast({ type: "error", title: "Registration failed", description: event.reason });
        }
      })
    );

    // Incoming calls
    unlisteners.push(
      onIncomingCall((event) => {
        setIncomingCall({
          id: event.call_id,
          direction: "inbound",
          state: "ringing",
          remoteUri: event.caller_uri,
          remoteName: event.caller_name,
          startTime: Date.now(),
          connectTime: null,
          isMuted: false,
          isHeld: false,
        });
        shouldNotify().then((ok) => {
          if (ok) toast({ type: "info", title: "Incoming call", description: event.caller_name || event.caller_uri });
        });
      })
    );

    // Call state changes
    unlisteners.push(
      onCallState((event) => {
        const state = mapCallState(event.state);
        const existing = useCallStore.getState().sessions.find((s) => s.id === event.call_id);

        if (!existing && state !== "terminated") {
          // New outbound call tracked from backend
          addSession({
            id: event.call_id,
            direction: event.direction === "inbound" ? "inbound" : "outbound",
            state,
            remoteUri: event.remote_uri,
            remoteName: event.remote_name,
            startTime: Date.now(),
            connectTime: state === "connected" ? Date.now() : null,
            isMuted: false,
            isHeld: false,
          });
          setActiveCallId(event.call_id);
        } else if (existing) {
          updateSessionState(event.call_id, state);

          if (state === "connected" && !existing.connectTime) {
            setConnectTime(event.call_id, Date.now());
            toast({ type: "success", title: "Call connected" });
          }

          if (state === "terminated") {
            toast({ type: "info", title: "Call ended" });
            // Save to call history
            const durationSecs = existing.connectTime
              ? Math.floor((Date.now() - existing.connectTime) / 1000)
              : 0;
            const record = {
              direction: existing.direction,
              remote_uri: existing.remoteUri,
              remote_name: existing.remoteName,
              start_time: new Date(existing.startTime ?? Date.now()).toISOString(),
              duration_secs: durationSecs,
              answered: existing.connectTime !== null,
            };
            addCallRecord({ id: 0, ...record }).catch(() => {});
            // Sync to pale-server if connected
            const { baseUrl, token } = useServerStore.getState();
            if (baseUrl && token) {
              paleServerSyncCallHistory(baseUrl, token, [record]).catch(() => {});
            }
            setTimeout(() => removeSession(event.call_id), 500);
          }
        }
      })
    );

    // Errors
    unlisteners.push(
      onPaleError((event) => {
        toast({ type: "error", title: "SIP Error", description: event.message });
      })
    );

    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [
    setRegState, addSession, updateSessionState, setIncomingCall,
    setActiveCallId, setConnectTime, removeSession,
  ]);
}

// Map backend snake_case enum values to frontend types
function mapRegState(state: string | RegState): RegState {
  const map: Record<string, RegState> = {
    registered: "registered",
    registering: "registering",
    unregistered: "unregistered",
    none: "none",
  };
  return map[state as string] ?? "none";
}

function mapCallState(state: string | CallState): CallState {
  const map: Record<string, CallState> = {
    idle: "idle",
    dialing: "dialing",
    ringing: "ringing",
    early_media: "early_media",
    connected: "connected",
    on_hold: "on_hold",
    transferring: "transferring",
    terminated: "terminated",
  };
  return map[state as string] ?? "idle";
}
