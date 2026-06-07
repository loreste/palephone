import { useRef, useState, type ReactNode } from "react";
import { Loader2 } from "lucide-react";
import { cn } from "@/lib/cn";

interface PullToRefreshProps {
  children: ReactNode;
  onRefresh: () => Promise<void>;
  className?: string;
}

/**
 * Wraps a scrollable container with pull-to-refresh gesture.
 */
export function PullToRefresh({ children, onRefresh, className }: PullToRefreshProps) {
  const [pulling, setPulling] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [pullDistance, setPullDistance] = useState(0);
  const startY = useRef(0);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleTouchStart = (e: React.TouchEvent) => {
    if (containerRef.current && containerRef.current.scrollTop === 0) {
      startY.current = e.touches[0].clientY;
      setPulling(true);
    }
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (!pulling || refreshing) return;
    const diff = e.touches[0].clientY - startY.current;
    if (diff > 0) {
      setPullDistance(Math.min(diff * 0.5, 60));
    }
  };

  const handleTouchEnd = async () => {
    setPulling(false);
    if (pullDistance > 40 && !refreshing) {
      setRefreshing(true);
      setPullDistance(40);
      await onRefresh();
      setRefreshing(false);
    }
    setPullDistance(0);
  };

  return (
    <div
      ref={containerRef}
      className={cn("overflow-y-auto", className)}
      onTouchStart={handleTouchStart}
      onTouchMove={handleTouchMove}
      onTouchEnd={handleTouchEnd}
    >
      {(pullDistance > 0 || refreshing) && (
        <div
          className="flex items-center justify-center transition-all"
          style={{ height: pullDistance }}
        >
          <Loader2
            size={16}
            className={cn("text-accent", refreshing && "animate-spin")}
          />
        </div>
      )}
      {children}
    </div>
  );
}
