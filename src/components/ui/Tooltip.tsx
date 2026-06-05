import { useState, useRef, type ReactNode } from "react";
import { cn } from "@/lib/cn";

interface TooltipProps {
  content: string;
  children: ReactNode;
  side?: "top" | "bottom";
}

export function Tooltip({ content, children, side = "top" }: TooltipProps) {
  const [visible, setVisible] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const show = () => {
    timeoutRef.current = setTimeout(() => setVisible(true), 500);
  };
  const hide = () => {
    clearTimeout(timeoutRef.current);
    setVisible(false);
  };

  return (
    <div className="relative inline-flex" onMouseEnter={show} onMouseLeave={hide}>
      {children}
      {visible && (
        <div
          role="tooltip"
          className={cn(
            "absolute left-1/2 -translate-x-1/2 z-50",
            "px-2 py-1 rounded-md text-xs font-medium",
            "bg-overlay text-primary shadow-md",
            "whitespace-nowrap pointer-events-none",
            "animate-in fade-in-0 zoom-in-95",
            side === "top" ? "bottom-full mb-1.5" : "top-full mt-1.5"
          )}
        >
          {content}
        </div>
      )}
    </div>
  );
}
