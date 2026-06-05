import { useEffect, useRef } from "react";
import { useCallStore } from "@/store/callStore";

/**
 * Plays a ringtone using Web Audio API when there's an incoming call.
 * Two-tone pattern similar to a classic phone ring.
 */
export function useRingtone() {
  const incomingCall = useCallStore((s) => s.incomingCall);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval>>(undefined);

  useEffect(() => {
    if (incomingCall) {
      // Start ringing
      const ctx = new AudioContext();
      audioCtxRef.current = ctx;

      const playRingBurst = () => {
        if (ctx.state === "closed") return;

        // Ring tone: two frequencies (440Hz + 480Hz) for 1s, silence for 2s
        const now = ctx.currentTime;
        const duration = 0.8;

        const osc1 = ctx.createOscillator();
        const osc2 = ctx.createOscillator();
        const gain = ctx.createGain();

        osc1.frequency.value = 440;
        osc2.frequency.value = 480;
        osc1.type = "sine";
        osc2.type = "sine";

        gain.gain.setValueAtTime(0, now);
        gain.gain.linearRampToValueAtTime(0.15, now + 0.02);
        gain.gain.setValueAtTime(0.15, now + duration - 0.02);
        gain.gain.linearRampToValueAtTime(0, now + duration);

        osc1.connect(gain);
        osc2.connect(gain);
        gain.connect(ctx.destination);

        osc1.start(now);
        osc2.start(now);
        osc1.stop(now + duration);
        osc2.stop(now + duration);
      };

      // Play immediately and repeat every 3 seconds
      playRingBurst();
      intervalRef.current = setInterval(playRingBurst, 3000);

      return () => {
        clearInterval(intervalRef.current);
        ctx.close().catch(() => {});
        audioCtxRef.current = null;
      };
    } else {
      // Stop ringing
      clearInterval(intervalRef.current);
      if (audioCtxRef.current) {
        audioCtxRef.current.close().catch(() => {});
        audioCtxRef.current = null;
      }
    }
  }, [incomingCall]);
}
