import { useEffect, useRef } from "react";
import { useServerStore } from "@/store/serverStore";
import { usePresenceStore } from "@/store/presenceStore";
import { useAccountStore } from "@/store/accountStore";
import { paleServerSetPresence } from "@/lib/tauri";

const IDLE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes

/**
 * Tracks mouse/keyboard activity and automatically sets presence to "away"
 * after 5 minutes of inactivity. Restores "online" on activity resume.
 * Does not override manual "busy" or "dnd" status.
 */
export function useAutoAway() {
  const { baseUrl, token, connected } = useServerStore();
  const lastActivityRef = useRef(Date.now());
  const isAwayRef = useRef(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    if (!connected || !baseUrl || !token) return;

    const resetActivity = () => {
      lastActivityRef.current = Date.now();

      if (isAwayRef.current) {
        // Check if user manually set busy/dnd — don't override
        const sipUri = useAccountStore.getState().account?.sipUri;
        if (sipUri) {
          const presenceMap = usePresenceStore.getState().presenceMap;
          const current = Object.values(presenceMap).find(
            (p) => p.sip_uri === sipUri || p.sip_uri === `sip:${sipUri}`
          );
          if (current && (current.status === "busy" || current.status === "dnd")) {
            isAwayRef.current = false;
            return;
          }
        }

        isAwayRef.current = false;
        paleServerSetPresence(baseUrl, token, "online").catch(() => {});
      }
    };

    const checkIdle = () => {
      const elapsed = Date.now() - lastActivityRef.current;
      if (elapsed >= IDLE_TIMEOUT_MS && !isAwayRef.current) {
        // Check if user manually set busy/dnd — don't override
        const sipUri = useAccountStore.getState().account?.sipUri;
        if (sipUri) {
          const presenceMap = usePresenceStore.getState().presenceMap;
          const current = Object.values(presenceMap).find(
            (p) => p.sip_uri === sipUri || p.sip_uri === `sip:${sipUri}`
          );
          if (current && (current.status === "busy" || current.status === "dnd")) {
            return;
          }
        }

        isAwayRef.current = true;
        paleServerSetPresence(baseUrl, token, "away", "Auto-away (idle)").catch(() => {});
      }
    };

    window.addEventListener("mousemove", resetActivity);
    window.addEventListener("mousedown", resetActivity);
    window.addEventListener("keydown", resetActivity);
    window.addEventListener("touchstart", resetActivity);

    timerRef.current = window.setInterval(checkIdle, 30_000); // check every 30s

    return () => {
      window.removeEventListener("mousemove", resetActivity);
      window.removeEventListener("mousedown", resetActivity);
      window.removeEventListener("keydown", resetActivity);
      window.removeEventListener("touchstart", resetActivity);
      if (timerRef.current) {
        window.clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [connected, baseUrl, token]);
}
