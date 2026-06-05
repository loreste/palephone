import { Key, X } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { cn } from "@/lib/cn";
import { Button } from "@/components/ui/Button";

interface KeyBackupPromptProps {
  open: boolean;
  onDismiss: () => void;
  onSetup: () => void;
}

export function KeyBackupPrompt({ open, onDismiss, onSetup }: KeyBackupPromptProps) {
  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 20 }}
          className={cn(
            "mx-4 mb-3 p-3 rounded-lg",
            "bg-warning-muted border border-warning/20"
          )}
        >
          <div className="flex items-start gap-3">
            <Key size={18} className="text-warning shrink-0 mt-0.5" />
            <div className="flex-1">
              <p className="text-sm font-medium text-primary">Set up key backup</p>
              <p className="text-xs text-tertiary mt-0.5">
                Back up your encryption keys so you can access your messages on new devices.
              </p>
              <div className="flex gap-2 mt-2">
                <Button size="sm" variant="primary" onClick={onSetup}>
                  Set Up
                </Button>
                <Button size="sm" variant="ghost" onClick={onDismiss}>
                  Later
                </Button>
              </div>
            </div>
            <button
              onClick={onDismiss}
              className="text-tertiary hover:text-secondary shrink-0"
            >
              <X size={14} />
            </button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
