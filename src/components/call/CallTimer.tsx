import { useState, useEffect } from "react";
import { cn } from "@/lib/cn";

interface CallTimerProps {
  connectTime: number | null;
  className?: string;
}

export function CallTimer({ connectTime, className }: CallTimerProps) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (connectTime === null) {
      setElapsed(0);
      return;
    }

    const tick = () => {
      setElapsed(Math.floor((Date.now() - connectTime) / 1000));
    };
    tick();
    const interval = setInterval(tick, 1000);
    return () => clearInterval(interval);
  }, [connectTime]);

  const hours = Math.floor(elapsed / 3600);
  const minutes = Math.floor((elapsed % 3600) / 60);
  const seconds = elapsed % 60;

  const formatted =
    hours > 0
      ? `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
      : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;

  return (
    <span
      className={cn(
        "text-xl font-semibold text-secondary tabular-nums tracking-wider",
        className
      )}
    >
      {formatted}
    </span>
  );
}
