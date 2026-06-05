import { useState, useEffect, useMemo, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Phone, Settings, Volume2, User, Clock, Search } from "lucide-react";
import { cn } from "@/lib/cn";
import { useUiStore } from "@/store/uiStore";

interface CommandItem {
  id: string;
  label: string;
  icon: typeof Phone;
  action: () => void;
  category: string;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const setActiveTab = useUiStore((s) => s.setActiveTab);

  const commands: CommandItem[] = useMemo(
    () => [
      {
        id: "call-alice",
        label: "Call Alice Smith",
        icon: Phone,
        category: "Contacts",
        action: () => {
          console.log("Call Alice");
          onClose();
        },
      },
      {
        id: "call-bob",
        label: "Call Bob Chen",
        icon: Phone,
        category: "Contacts",
        action: () => {
          console.log("Call Bob");
          onClose();
        },
      },
      {
        id: "open-dialpad",
        label: "Open Dialpad",
        icon: Phone,
        category: "Navigation",
        action: () => {
          setActiveTab("dialpad");
          onClose();
        },
      },
      {
        id: "open-settings",
        label: "Open Settings",
        icon: Settings,
        category: "Navigation",
        action: () => {
          setActiveTab("settings");
          onClose();
        },
      },
      {
        id: "open-audio",
        label: "Audio Settings",
        icon: Volume2,
        category: "Navigation",
        action: () => {
          setActiveTab("settings");
          onClose();
        },
      },
      {
        id: "open-files",
        label: "Open Files",
        icon: User,
        category: "Navigation",
        action: () => {
          setActiveTab("files");
          onClose();
        },
      },
      {
        id: "open-recent",
        label: "Recent Calls",
        icon: Clock,
        category: "Navigation",
        action: () => {
          setActiveTab("recent");
          onClose();
        },
      },
    ],
    [onClose, setActiveTab]
  );

  const filtered = useMemo(() => {
    if (!query.trim()) return commands;
    const lower = query.toLowerCase();
    return commands.filter((cmd) =>
      cmd.label.toLowerCase().includes(lower)
    );
  }, [query, commands]);

  // Reset on open
  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // Clamp selection
  useEffect(() => {
    if (selectedIndex >= filtered.length) {
      setSelectedIndex(Math.max(0, filtered.length - 1));
    }
  }, [filtered.length, selectedIndex]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && filtered[selectedIndex]) {
      e.preventDefault();
      filtered[selectedIndex].action();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
            className="fixed inset-0 z-50 bg-base/50 backdrop-blur-sm"
          />

          {/* Panel */}
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: -10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -10 }}
            transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
            className={cn(
              "fixed inset-x-4 top-[80px] z-50",
              "bg-surface border border-border-subtle rounded-xl",
              "shadow-lg overflow-hidden"
            )}
          >
            {/* Search input */}
            <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border-subtle">
              <Search size={16} className="text-tertiary shrink-0" />
              <input
                ref={inputRef}
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="Type a command or contact..."
                className={cn(
                  "flex-1 bg-transparent text-sm text-primary",
                  "placeholder:text-tertiary",
                  "focus:outline-none"
                )}
              />
            </div>

            {/* Results */}
            <div className="max-h-[280px] overflow-y-auto py-1">
              {filtered.length === 0 ? (
                <p className="text-xs text-tertiary text-center py-6">
                  No results found
                </p>
              ) : (
                filtered.map((cmd, i) => (
                  <button
                    key={cmd.id}
                    onClick={cmd.action}
                    onMouseEnter={() => setSelectedIndex(i)}
                    className={cn(
                      "w-full flex items-center gap-2.5 px-3 py-2 text-left",
                      "transition-colors",
                      i === selectedIndex
                        ? "bg-accent-muted text-accent"
                        : "text-secondary hover:bg-elevated"
                    )}
                  >
                    <cmd.icon size={16} className="shrink-0" />
                    <span className="text-sm">{cmd.label}</span>
                    <span className="text-[10px] text-tertiary ml-auto">
                      {cmd.category}
                    </span>
                  </button>
                ))
              )}
            </div>

            {/* Footer hint */}
            <div className="flex items-center gap-3 px-3 py-2 border-t border-border-subtle">
              <span className="text-[10px] text-tertiary">
                <kbd className="px-1 py-0.5 rounded bg-elevated text-secondary font-mono">\u2191\u2193</kbd> navigate
              </span>
              <span className="text-[10px] text-tertiary">
                <kbd className="px-1 py-0.5 rounded bg-elevated text-secondary font-mono">\u23CE</kbd> select
              </span>
              <span className="text-[10px] text-tertiary">
                <kbd className="px-1 py-0.5 rounded bg-elevated text-secondary font-mono">esc</kbd> close
              </span>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
