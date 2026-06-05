import { cn } from "@/lib/cn";

const gradientPairs = [
  ["#6366F1", "#8B5CF6"], // indigo → violet
  ["#EC4899", "#F43F5E"], // pink → rose
  ["#14B8A6", "#06B6D4"], // teal → cyan
  ["#F59E0B", "#EF4444"], // amber → red
  ["#22C55E", "#14B8A6"], // green → teal
];

function hashName(name: string): number {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash << 5) - hash + name.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash);
}

function getInitials(name: string): string {
  const parts = name.trim().split(/\s+/);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0][0]?.toUpperCase() ?? "?";
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

interface CallerAvatarProps {
  name: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = {
  sm: "w-10 h-10 text-sm",
  md: "w-16 h-16 text-xl",
  lg: "w-24 h-24 text-3xl",
};

export function CallerAvatar({ name, size = "lg", className }: CallerAvatarProps) {
  const hash = hashName(name);
  const pair = gradientPairs[hash % gradientPairs.length];
  const initials = getInitials(name);

  return (
    <div
      className={cn(
        "rounded-full flex items-center justify-center font-semibold text-white shrink-0",
        sizeMap[size],
        className
      )}
      style={{
        background: `linear-gradient(135deg, ${pair[0]}, ${pair[1]})`,
      }}
      aria-hidden
    >
      {initials}
    </div>
  );
}
