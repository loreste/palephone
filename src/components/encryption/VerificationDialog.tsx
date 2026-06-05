import { useState } from "react";
import { ShieldCheck, ShieldX } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { cn } from "@/lib/cn";
import { Button } from "@/components/ui/Button";

// Emoji set used for SAS verification (matches Matrix spec)
const VERIFICATION_EMOJIS = [
  "🐶", "🐱", "🦁", "🐴", "🦄", "🐷", "🐘",
  "🐰", "🦊", "🐻", "🐼", "🐔", "🐧", "🐢",
  "🐟", "🐙", "🦋", "🌸", "🌲", "🌵", "🍄",
  "🌍", "🌙", "☁️", "🔥", "🌈", "⭐", "🎸",
  "🎺", "🎲", "🔑", "🔔", "🎁", "💎", "🚀",
  "✈️", "🚂", "🚢", "🏠", "⛪", "🗽", "🗼",
  "⚓", "🔧", "🔨", "📱", "💻", "📷", "🎵",
  "❤️", "💚", "💙", "💜", "🧡", "💛", "🤍",
  "♠️", "♣️", "♥️", "♦️", "✅", "❌", "⚡",
];

interface VerificationDialogProps {
  open: boolean;
  onClose: () => void;
  peerName: string;
  emojis?: number[]; // Indices into VERIFICATION_EMOJIS
}

export function VerificationDialog({
  open,
  onClose,
  peerName,
  emojis,
}: VerificationDialogProps) {
  const [result, setResult] = useState<"pending" | "matched" | "mismatched">("pending");

  // Use provided emojis or generate mock ones for demo
  const displayEmojis = emojis ?? [0, 27, 34, 17, 31, 30, 5];

  const handleMatch = () => {
    setResult("matched");
    setTimeout(() => {
      onClose();
      setResult("pending");
    }, 2000);
  };

  const handleMismatch = () => {
    setResult("mismatched");
    setTimeout(() => {
      onClose();
      setResult("pending");
    }, 2000);
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
            className="fixed inset-0 z-50 bg-base/60 backdrop-blur-sm"
          />
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            className={cn(
              "fixed inset-x-4 top-1/4 z-50",
              "bg-surface border border-border-subtle rounded-xl",
              "shadow-lg p-6"
            )}
          >
            {result === "pending" ? (
              <>
                <div className="flex items-center justify-center mb-4">
                  <div className="w-12 h-12 rounded-full bg-accent/10 flex items-center justify-center">
                    <ShieldCheck size={24} className="text-accent" />
                  </div>
                </div>

                <h3 className="text-base font-semibold text-primary text-center mb-1">
                  Verify {peerName}
                </h3>
                <p className="text-xs text-tertiary text-center mb-5">
                  Compare these emoji with {peerName}'s device to verify the encryption
                </p>

                {/* Emoji grid */}
                <div className="flex justify-center gap-3 mb-6">
                  {displayEmojis.map((idx, i) => (
                    <div key={i} className="flex flex-col items-center gap-1">
                      <span className="text-2xl">{VERIFICATION_EMOJIS[idx]}</span>
                    </div>
                  ))}
                </div>

                <div className="flex gap-3">
                  <Button
                    variant="destructive"
                    className="flex-1 gap-1"
                    onClick={handleMismatch}
                  >
                    <ShieldX size={16} />
                    They don't match
                  </Button>
                  <Button
                    variant="success"
                    className="flex-1 gap-1"
                    onClick={handleMatch}
                  >
                    <ShieldCheck size={16} />
                    They match
                  </Button>
                </div>
              </>
            ) : result === "matched" ? (
              <div className="text-center py-4">
                <ShieldCheck size={48} className="text-success mx-auto mb-3" />
                <h3 className="text-base font-semibold text-success">Verified!</h3>
                <p className="text-xs text-tertiary mt-1">
                  {peerName}'s device is now verified
                </p>
              </div>
            ) : (
              <div className="text-center py-4">
                <ShieldX size={48} className="text-destructive mx-auto mb-3" />
                <h3 className="text-base font-semibold text-destructive">
                  Verification Failed
                </h3>
                <p className="text-xs text-tertiary mt-1">
                  The emoji didn't match. This could indicate a security issue.
                </p>
              </div>
            )}
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
