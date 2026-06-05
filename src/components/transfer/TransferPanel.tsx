import { useState } from "react";
import { X, ArrowRight, Phone } from "lucide-react";
import { cn } from "@/lib/cn";
import { Button } from "@/components/ui/Button";

interface TransferPanelProps {
  onClose: () => void;
  onBlindTransfer: (target: string) => void;
  onAttendedTransfer: (target: string) => void;
}

const quickTargets = [
  { name: "Alice Smith", uri: "sip:alice@example.com" },
  { name: "Support Queue", uri: "sip:300@example.com" },
];

export function TransferPanel({
  onClose,
  onBlindTransfer,
  onAttendedTransfer,
}: TransferPanelProps) {
  const [target, setTarget] = useState("");

  return (
    <div className="w-full px-2">
      <div
        className={cn(
          "bg-surface border border-border-subtle rounded-xl p-4 space-y-3"
        )}
      >
        {/* Header */}
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-primary">Transfer Call</h3>
          <button
            onClick={onClose}
            className="p-1 rounded-md text-tertiary hover:text-secondary hover:bg-elevated"
            aria-label="Cancel transfer"
          >
            <X size={14} />
          </button>
        </div>

        {/* Target input */}
        <input
          type="text"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          placeholder="Enter number or SIP URI..."
          autoFocus
          className={cn(
            "w-full bg-base border border-border-subtle rounded-lg",
            "px-3 py-2 text-sm text-primary",
            "placeholder:text-tertiary",
            "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
          )}
        />

        {/* Quick targets */}
        {!target && (
          <div className="space-y-1">
            <p className="text-[10px] font-semibold text-tertiary uppercase tracking-wider">
              Recent
            </p>
            {quickTargets.map((t) => (
              <button
                key={t.uri}
                onClick={() => setTarget(t.uri)}
                className={cn(
                  "w-full flex items-center gap-2 px-2 py-1.5 rounded-md",
                  "text-left hover:bg-elevated transition-colors"
                )}
              >
                <Phone size={12} className="text-tertiary" />
                <span className="text-xs text-primary">{t.name}</span>
                <span className="text-[10px] text-tertiary ml-auto">{t.uri.split("@")[0]?.split(":")[1]}</span>
              </button>
            ))}
          </div>
        )}

        {/* Transfer buttons */}
        <div className="flex gap-2 pt-1">
          <Button
            variant="secondary"
            size="sm"
            className="flex-1 gap-1"
            disabled={!target.trim()}
            onClick={() => onBlindTransfer(target)}
          >
            <ArrowRight size={14} />
            Blind
          </Button>
          <Button
            variant="primary"
            size="sm"
            className="flex-1 gap-1"
            disabled={!target.trim()}
            onClick={() => onAttendedTransfer(target)}
          >
            <Phone size={14} />
            Attended
          </Button>
        </div>
      </div>
    </div>
  );
}
