import { type LucideIcon } from "lucide-react";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/Tooltip";

interface CallControlButtonProps {
  icon: LucideIcon;
  label: string;
  active?: boolean;
  activeColor?: "accent" | "warning" | "destructive";
  disabled?: boolean;
  onClick: () => void;
}

const activeColorMap = {
  accent: "bg-accent text-white",
  warning: "bg-warning text-inverse",
  destructive: "bg-destructive text-white",
};

export function CallControlButton({
  icon: Icon,
  label,
  active = false,
  activeColor = "accent",
  disabled = false,
  onClick,
}: CallControlButtonProps) {
  return (
    <Tooltip content={label}>
      <button
        onClick={onClick}
        disabled={disabled}
        aria-label={label}
        aria-pressed={active}
        className={cn(
          "flex flex-col items-center justify-center gap-1",
          "w-14 h-14 rounded-xl transition-all",
          "disabled:opacity-40 disabled:cursor-not-allowed",
          active
            ? activeColorMap[activeColor]
            : "text-secondary hover:bg-white/10 hover:text-primary"
        )}
      >
        <Icon size={22} strokeWidth={active ? 2.5 : 1.5} />
        <span className="text-[9px] font-medium">{label}</span>
      </button>
    </Tooltip>
  );
}
