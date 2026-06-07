import { useState, useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Search, X, MessageSquare, Hash } from "lucide-react";
import { cn } from "@/lib/cn";
import { useServerStore } from "@/store/serverStore";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import { paleServerSearchMessages, type SearchResult } from "@/lib/tauri";

interface SearchOverlayProps {
  open: boolean;
  onClose: () => void;
}

export function SearchOverlay({ open, onClose }: SearchOverlayProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const { baseUrl, token, connected } = useServerStore();
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const setActiveRoomId = useChatStore((s) => s.setActiveRoomId);
  const debounceRef = useRef<number | null>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setResults([]);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const handleSearch = (value: string) => {
    setQuery(value);
    if (debounceRef.current) window.clearTimeout(debounceRef.current);

    if (!value.trim() || !connected || !baseUrl || !token) {
      setResults([]);
      return;
    }

    debounceRef.current = window.setTimeout(async () => {
      setLoading(true);
      try {
        const data = await paleServerSearchMessages(baseUrl, token, value.trim(), 30);
        setResults(data);
      } catch {
        setResults([]);
      }
      setLoading(false);
    }, 300);
  };

  const handleSelect = (result: SearchResult) => {
    if (result.room_id) {
      setActiveRoomId(result.room_id);
    }
    setActiveTab("chat");
    onClose();
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
            className="fixed inset-0 z-50 bg-base/50 backdrop-blur-sm"
          />
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: -10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -10 }}
            transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
            className={cn(
              "fixed inset-x-4 top-[60px] z-50",
              "bg-surface border border-border-subtle rounded-xl",
              "shadow-lg overflow-hidden max-h-[400px] flex flex-col"
            )}
          >
            <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border-subtle">
              <Search size={16} className="text-tertiary shrink-0" />
              <input
                ref={inputRef}
                type="text"
                value={query}
                onChange={(e) => handleSearch(e.target.value)}
                onKeyDown={(e) => e.key === "Escape" && onClose()}
                placeholder="Search messages..."
                className="flex-1 bg-transparent text-sm text-primary placeholder:text-tertiary focus:outline-none"
              />
              {query && (
                <button onClick={() => handleSearch("")} className="text-tertiary hover:text-secondary">
                  <X size={14} />
                </button>
              )}
            </div>

            <div className="flex-1 overflow-y-auto py-1">
              {!connected && (
                <p className="text-xs text-tertiary text-center py-6">Connect to a server to search</p>
              )}
              {connected && !query.trim() && (
                <p className="text-xs text-tertiary text-center py-6">Type to search across all messages</p>
              )}
              {loading && (
                <p className="text-xs text-tertiary text-center py-4">Searching...</p>
              )}
              {!loading && query.trim() && results.length === 0 && (
                <p className="text-xs text-tertiary text-center py-6">No results found</p>
              )}
              {results.map((result) => (
                <button
                  key={result.id}
                  onClick={() => handleSelect(result)}
                  className="w-full flex items-start gap-2.5 px-3 py-2 text-left hover:bg-elevated transition-colors"
                >
                  {result.source === "room" ? (
                    <Hash size={14} className="text-accent shrink-0 mt-0.5" />
                  ) : (
                    <MessageSquare size={14} className="text-tertiary shrink-0 mt-0.5" />
                  )}
                  <div className="flex-1 min-w-0">
                    <p className="text-xs text-secondary truncate">{result.from_uri}</p>
                    <p className="text-sm text-primary line-clamp-2">{result.body}</p>
                    <p className="text-[10px] text-tertiary mt-0.5">
                      {new Date(result.timestamp).toLocaleString([], {
                        month: "short",
                        day: "numeric",
                        hour: "numeric",
                        minute: "2-digit",
                      })}
                    </p>
                  </div>
                </button>
              ))}
            </div>

            <div className="flex items-center gap-3 px-3 py-1.5 border-t border-border-subtle text-[10px] text-tertiary">
              <span><kbd className="px-1 py-0.5 rounded bg-elevated text-secondary font-mono">esc</kbd> close</span>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
