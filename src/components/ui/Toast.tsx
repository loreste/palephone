import { useEffect, useState, useCallback } from "react";
import { CheckCircle, AlertCircle, AlertTriangle, Info, X } from "lucide-react";
import { cn } from "@/lib/cn";
import { AnimatePresence, motion } from "framer-motion";

type ToastType = "success" | "error" | "warning" | "info";

interface Toast {
  id: string;
  type: ToastType;
  title: string;
  description?: string;
  duration?: number;
}

const toastConfig: Record<
  ToastType,
  { icon: typeof CheckCircle; borderColor: string; defaultDuration: number }
> = {
  success: { icon: CheckCircle, borderColor: "border-l-success", defaultDuration: 3000 },
  error: { icon: AlertCircle, borderColor: "border-l-destructive", defaultDuration: 8000 },
  warning: { icon: AlertTriangle, borderColor: "border-l-warning", defaultDuration: 5000 },
  info: { icon: Info, borderColor: "border-l-info", defaultDuration: 4000 },
};

// Simple global toast state
let addToastFn: ((toast: Omit<Toast, "id">) => void) | null = null;

export function toast(opts: Omit<Toast, "id">) {
  addToastFn?.(opts);
}

export function ToastContainer() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback((opts: Omit<Toast, "id">) => {
    const id = crypto.randomUUID();
    setToasts((prev) => [...prev, { ...opts, id }]);
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  useEffect(() => {
    addToastFn = addToast;
    return () => {
      addToastFn = null;
    };
  }, [addToast]);

  return (
    <div className="fixed top-[68px] right-3 z-50 flex flex-col gap-2 w-[280px] pointer-events-none">
      <AnimatePresence>
        {toasts.map((t) => (
          <ToastItem key={t.id} toast={t} onDismiss={removeToast} />
        ))}
      </AnimatePresence>
    </div>
  );
}

function ToastItem({
  toast: t,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: (id: string) => void;
}) {
  const config = toastConfig[t.type];
  const Icon = config.icon;
  const duration = t.duration ?? config.defaultDuration;

  useEffect(() => {
    const timer = setTimeout(() => onDismiss(t.id), duration);
    return () => clearTimeout(timer);
  }, [t.id, duration, onDismiss]);

  return (
    <motion.div
      initial={{ opacity: 0, x: 80 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
      className={cn(
        "pointer-events-auto",
        "flex items-start gap-2.5 p-3 rounded-lg",
        "bg-surface border border-border-subtle",
        "border-l-[3px] shadow-md",
        config.borderColor
      )}
    >
      <Icon size={16} className="shrink-0 mt-0.5 text-secondary" />
      <div className="flex-1 min-w-0">
        <p className="text-xs font-semibold text-primary">{t.title}</p>
        {t.description && (
          <p className="text-[11px] text-tertiary mt-0.5">{t.description}</p>
        )}
      </div>
      <button
        onClick={() => onDismiss(t.id)}
        className="shrink-0 text-tertiary hover:text-secondary"
      >
        <X size={12} />
      </button>
    </motion.div>
  );
}
