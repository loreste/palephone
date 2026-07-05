import { useState, useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Search, X, MessageSquare, Hash, FileText, Users, CalendarDays, Video, AppWindow, User, Sparkles } from "lucide-react";
import { cn } from "@/lib/cn";
import { useServerStore } from "@/store/serverStore";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import { paleServerCopilotQuery, paleServerUnifiedSearch, type CopilotAnswer, type UnifiedSearchResult } from "@/lib/tauri";
import type { Tab } from "@/types";

interface SearchOverlayProps {
  open: boolean;
  onClose: () => void;
}

export function SearchOverlay({ open, onClose }: SearchOverlayProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<UnifiedSearchResult[]>([]);
  const [copilot, setCopilot] = useState<CopilotAnswer | null>(null);
  const [loading, setLoading] = useState(false);
  const [copilotLoading, setCopilotLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const { baseUrl, token, connected } = useServerStore();
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const setActiveRoomId = useChatStore((s) => s.setActiveRoomId);
  const debounceRef = useRef<number | null>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setResults([]);
      setCopilot(null);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const handleSearch = (value: string) => {
    setQuery(value);
    if (debounceRef.current) window.clearTimeout(debounceRef.current);

    if (!value.trim() || !connected || !baseUrl || !token) {
      setResults([]);
      setCopilot(null);
      return;
    }

    debounceRef.current = window.setTimeout(async () => {
      setLoading(true);
      try {
        const data = await paleServerUnifiedSearch(baseUrl, token, value.trim(), 30);
        setResults(data);
      } catch {
        setResults([]);
      }
      setLoading(false);
    }, 300);
  };

  const askCopilot = async () => {
    if (!query.trim() || !connected || !baseUrl || !token) return;
    setCopilotLoading(true);
    try {
      const answer = await paleServerCopilotQuery(baseUrl, token, query.trim(), query.trim(), 8);
      setCopilot(answer);
    } catch {
      setCopilot(null);
    }
    setCopilotLoading(false);
  };

  const handleSelect = (result: UnifiedSearchResult) => {
    if (result.room_id) {
      setActiveRoomId(result.room_id);
    }
    setActiveTab(tabForResult(result));
    onClose();
  };

  const iconForResult = (kind: string) => {
    if (kind === "file") return <FileText size={14} className="text-accent shrink-0 mt-0.5" />;
    if (kind === "team") return <Users size={14} className="text-accent shrink-0 mt-0.5" />;
    if (kind === "user") return <User size={14} className="text-accent shrink-0 mt-0.5" />;
    if (kind === "meeting") return <CalendarDays size={14} className="text-accent shrink-0 mt-0.5" />;
    if (kind === "recording") return <Video size={14} className="text-accent shrink-0 mt-0.5" />;
    if (kind === "app") return <AppWindow size={14} className="text-accent shrink-0 mt-0.5" />;
    if (kind === "channel" || kind === "room" || kind === "direct") return <Hash size={14} className="text-accent shrink-0 mt-0.5" />;
    return <MessageSquare size={14} className="text-accent shrink-0 mt-0.5" />;
  };

  const tabForResult = (result: UnifiedSearchResult): Tab => {
    if (result.kind === "file") return "files";
    if (result.kind === "user") return "people";
    if (result.kind === "meeting" || result.kind === "recording") return "calendar";
    if (result.kind === "app") return "settings";
    return "chat";
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
            role="dialog"
            aria-modal="true"
            aria-label="Enterprise search"
            initial={{ opacity: 0, scale: 0.95, y: -10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -10 }}
            transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
            className={cn(
              "fixed inset-x-4 top-[60px] z-50",
              "bg-surface border border-border-subtle rounded-xl",
              "shadow-lg overflow-hidden max-h-[560px] flex flex-col"
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
                placeholder="Search messages, files, meetings, people..."
                className="flex-1 bg-transparent text-sm text-primary placeholder:text-tertiary focus:outline-none"
              />
              {query && (
                <button onClick={() => handleSearch("")} className="text-tertiary hover:text-secondary" aria-label="Clear search">
                  <X size={14} />
                </button>
              )}
              <button
                onClick={askCopilot}
                disabled={!query.trim() || copilotLoading || !connected}
                className="h-7 w-7 inline-flex items-center justify-center rounded-md text-tertiary hover:text-primary hover:bg-elevated disabled:opacity-40 disabled:hover:bg-transparent"
                aria-label="Ask Copilot"
                title="Ask Copilot"
              >
                <Sparkles size={15} />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto py-1">
              {!connected && (
                <p className="text-xs text-tertiary text-center py-6">Connect to a server to search</p>
              )}
              {connected && !query.trim() && (
                <p className="text-xs text-tertiary text-center py-6">Type to search enterprise content</p>
              )}
              {loading && (
                <p className="text-xs text-tertiary text-center py-4">Searching...</p>
              )}
              {copilotLoading && (
                <p className="text-xs text-tertiary text-center py-4">Thinking...</p>
              )}
              {copilot && (
                <div className="mx-3 my-2 border border-border-subtle rounded-lg overflow-hidden">
                  <div className="flex items-center justify-between gap-2 px-3 py-2 border-b border-border-subtle bg-elevated/60">
                    <div className="flex items-center gap-2 min-w-0">
                      <Sparkles size={14} className="text-accent shrink-0" />
                      <p className="text-xs text-secondary truncate">{copilot.question}</p>
                    </div>
                    <span className="text-[10px] uppercase tracking-normal text-tertiary shrink-0">
                      {copilot.provider_configured ? "provider" : "local"}
                    </span>
                  </div>
                  <div className="px-3 py-2 space-y-2">
                    <p className="text-sm text-primary whitespace-pre-line">{copilot.answer}</p>
                    {copilot.citations.length > 0 && (
                      <div className="flex flex-wrap gap-1.5">
                        {copilot.citations.slice(0, 5).map((citation) => (
                          <button
                            key={`${citation.index}-${citation.result.kind}-${citation.result.id}`}
                            onClick={() => handleSelect(citation.result)}
                            className="text-[10px] px-2 py-1 rounded-md bg-elevated text-secondary hover:text-primary"
                          >
                            [{citation.index}] {citation.result.kind}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )}
              {!loading && query.trim() && results.length === 0 && (
                <p className="text-xs text-tertiary text-center py-6">No results found</p>
              )}
              {results.map((result) => (
                <button
                  key={`${result.kind}-${result.id}`}
                  onClick={() => handleSelect(result)}
                  className="w-full flex items-start gap-2.5 px-3 py-2 text-left hover:bg-elevated transition-colors"
                >
                  {iconForResult(result.kind)}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 min-w-0">
                      <p className="text-xs text-secondary truncate">{result.title}</p>
                      <span className="text-[10px] uppercase tracking-normal text-tertiary shrink-0">{result.kind}</span>
                    </div>
                    <p className="text-sm text-primary line-clamp-2">{result.snippet}</p>
                    <p className="text-[10px] text-tertiary mt-0.5">
                      {result.source} · {new Date(result.updated_at).toLocaleString([], {
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
