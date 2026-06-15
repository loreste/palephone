let audioCtx: AudioContext | null = null;

function getAudioContext(): AudioContext {
  if (!audioCtx) {
    audioCtx = new AudioContext();
  }
  return audioCtx;
}

/**
 * Play a short notification beep using the Web Audio API.
 * Two quick tones: 880 Hz for 80ms, then 1047 Hz for 80ms.
 */
export function playNotificationBeep(): void {
  try {
    const ctx = getAudioContext();
    const now = ctx.currentTime;

    const tones: [number, number, number][] = [
      [880, now, 0.08],
      [1047, now + 0.1, 0.08],
    ];

    for (const [freq, start, dur] of tones) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.value = freq;
      gain.gain.setValueAtTime(0.15, start);
      gain.gain.exponentialRampToValueAtTime(0.001, start + dur);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(start);
      osc.stop(start + dur + 0.01);
    }
  } catch {
    // Ignore audio errors (e.g. user hasn't interacted yet)
  }
}

/**
 * Play a repeating ringtone pattern for incoming calls.
 * Returns a stop function to silence it.
 * Pattern: two-tone ring (440+480 Hz) for 2s, silence 4s, repeating.
 */
export function playRingtone(): () => void {
  let stopped = false;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  let activeOscillators: OscillatorNode[] = [];

  function ring() {
    if (stopped) return;
    try {
      const ctx = getAudioContext();
      const now = ctx.currentTime;
      const duration = 2;

      const osc1 = ctx.createOscillator();
      const osc2 = ctx.createOscillator();
      const gain = ctx.createGain();

      osc1.type = "sine";
      osc1.frequency.value = 440;
      osc2.type = "sine";
      osc2.frequency.value = 480;

      gain.gain.setValueAtTime(0.12, now);
      gain.gain.setValueAtTime(0, now + duration);

      osc1.connect(gain);
      osc2.connect(gain);
      gain.connect(ctx.destination);

      osc1.start(now);
      osc1.stop(now + duration);
      osc2.start(now);
      osc2.stop(now + duration);

      activeOscillators = [osc1, osc2];

      // Ring again after 4 seconds of silence
      timeoutId = setTimeout(() => {
        if (!stopped) ring();
      }, (duration + 4) * 1000);
    } catch {
      // Ignore audio errors
    }
  }

  ring();

  return () => {
    stopped = true;
    if (timeoutId != null) clearTimeout(timeoutId);
    for (const osc of activeOscillators) {
      try { osc.stop(); } catch { /* already stopped */ }
    }
    activeOscillators = [];
  };
}
