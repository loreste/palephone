import { useRef, useState, type ReactNode } from "react";
import { cn } from "@/lib/cn";

interface SwipeActionProps {
  children: ReactNode;
  onSwipeLeft?: () => void;
  actionLabel?: string;
  actionColor?: string;
}

/**
 * Wraps a list item with swipe-left-to-reveal-action gesture.
 * Used for delete actions on call history, voicemails, etc.
 */
export function SwipeAction({
  children,
  onSwipeLeft,
  actionLabel = "Delete",
  actionColor = "bg-destructive",
}: SwipeActionProps) {
  const [offset, setOffset] = useState(0);
  const startX = useRef(0);
  const swiping = useRef(false);

  const handleTouchStart = (e: React.TouchEvent) => {
    startX.current = e.touches[0].clientX;
    swiping.current = true;
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (!swiping.current) return;
    const diff = startX.current - e.touches[0].clientX;
    if (diff > 0) {
      setOffset(Math.min(diff, 80));
    } else {
      setOffset(0);
    }
  };

  const handleTouchEnd = () => {
    swiping.current = false;
    if (offset > 60 && onSwipeLeft) {
      onSwipeLeft();
    }
    setOffset(0);
  };

  return (
    <div className="relative overflow-hidden">
      {onSwipeLeft && (
        <div
          className={cn(
            "absolute inset-y-0 right-0 flex items-center justify-center px-4 text-white text-xs font-medium",
            actionColor
          )}
          style={{ width: Math.max(offset, 0) }}
        >
          {offset > 40 && actionLabel}
        </div>
      )}
      <div
        onTouchStart={handleTouchStart}
        onTouchMove={handleTouchMove}
        onTouchEnd={handleTouchEnd}
        style={{ transform: `translateX(-${offset}px)`, transition: swiping.current ? "none" : "transform 0.2s" }}
      >
        {children}
      </div>
    </div>
  );
}
