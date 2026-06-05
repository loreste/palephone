import { Lock, LockOpen, ShieldAlert } from "lucide-react";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/Tooltip";

type EncryptionLevel = "encrypted" | "unencrypted" | "warning";

interface EncryptionBadgeProps {
  level: EncryptionLevel;
  size?: "sm" | "md";
}

const config: Record<EncryptionLevel, { icon: typeof Lock; color: string; tooltip: string }> = {
  encrypted: {
    icon: Lock,
    color: "text-success",
    tooltip: "End-to-end encrypted",
  },
  unencrypted: {
    icon: LockOpen,
    color: "text-tertiary",
    tooltip: "Not encrypted",
  },
  warning: {
    icon: ShieldAlert,
    color: "text-warning",
    tooltip: "Encryption warning — unverified devices in this room",
  },
};

export function EncryptionBadge({ level, size = "sm" }: EncryptionBadgeProps) {
  const { icon: Icon, color, tooltip } = config[level];
  const iconSize = size === "sm" ? 12 : 16;

  return (
    <Tooltip content={tooltip}>
      <span className={cn(color, "shrink-0 inline-flex")}>
        <Icon size={iconSize} />
      </span>
    </Tooltip>
  );
}
