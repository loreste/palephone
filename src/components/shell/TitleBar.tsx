import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/cn";

const appWindow = getCurrentWindow();

export function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex items-center justify-between h-[28px] px-3",
        "bg-base border-b border-border-subtle",
        "shrink-0"
      )}
    >
      {/* macOS traffic lights get space on the left */}
      <div className="flex items-center gap-2 pl-[68px]">
        <span className="text-xs font-semibold text-secondary tracking-wide">
          PALE
        </span>
      </div>

      {/* Window controls (Windows/Linux only — macOS uses native) */}
      <div className="flex items-center gap-0.5 -mr-1">
        <button
          onClick={() => appWindow.minimize()}
          className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Minimize"
        >
          <Minus size={14} />
        </button>
        <button
          onClick={() => appWindow.toggleMaximize()}
          className="p-1 rounded-sm hover:bg-elevated text-tertiary hover:text-secondary transition-colors"
          aria-label="Maximize"
        >
          <Square size={12} />
        </button>
        <button
          onClick={() => appWindow.close()}
          className="p-1 rounded-sm hover:bg-destructive/20 text-tertiary hover:text-destructive transition-colors"
          aria-label="Close"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
