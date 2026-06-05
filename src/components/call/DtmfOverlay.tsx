import { motion, AnimatePresence } from "framer-motion";
import { X } from "lucide-react";
import { cn } from "@/lib/cn";

const dtmfKeys = [
  "1", "2", "3",
  "4", "5", "6",
  "7", "8", "9",
  "*", "0", "#",
];

interface DtmfOverlayProps {
  open: boolean;
  onClose: () => void;
  onDigit: (digit: string) => void;
}

export function DtmfOverlay({ open, onClose, onDigit }: DtmfOverlayProps) {
  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 20 }}
          transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
          className={cn(
            "absolute inset-x-4 bottom-24 z-20",
            "bg-surface/95 backdrop-blur-lg border border-border-subtle",
            "rounded-xl p-4 shadow-lg"
          )}
        >
          <div className="flex items-center justify-between mb-3">
            <span className="text-xs font-semibold text-secondary">DTMF Keypad</span>
            <button
              onClick={onClose}
              className="p-1 rounded-md text-tertiary hover:text-secondary hover:bg-elevated"
              aria-label="Close keypad"
            >
              <X size={14} />
            </button>
          </div>

          <div className="grid grid-cols-3 gap-2">
            {dtmfKeys.map((key) => (
              <button
                key={key}
                onClick={() => onDigit(key)}
                className={cn(
                  "h-11 rounded-lg text-lg font-medium",
                  "bg-elevated hover:bg-overlay active:scale-95",
                  "text-primary transition-all"
                )}
              >
                {key}
              </button>
            ))}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
