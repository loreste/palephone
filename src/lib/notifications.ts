import { getConfig } from "@/lib/tauri";

let cachedConfig: {
  enabled: boolean;
  sound_enabled: boolean;
  dnd_enabled: boolean;
  dnd_start: string;
  dnd_end: string;
  muted_rooms: string[];
} | null = null;

let lastLoad = 0;

async function loadConfig() {
  // Cache config for 30 seconds
  if (cachedConfig && Date.now() - lastLoad < 30_000) return cachedConfig;
  try {
    const config = await getConfig();
    cachedConfig = config.notifications;
    lastLoad = Date.now();
  } catch {
    cachedConfig = {
      enabled: true,
      sound_enabled: true,
      dnd_enabled: false,
      dnd_start: "22:00",
      dnd_end: "07:00",
      muted_rooms: [],
    };
  }
  return cachedConfig;
}

function isInDndWindow(start: string, end: string): boolean {
  const now = new Date();
  const currentMinutes = now.getHours() * 60 + now.getMinutes();

  const [startH, startM] = start.split(":").map(Number);
  const [endH, endM] = end.split(":").map(Number);
  const startMinutes = (startH ?? 0) * 60 + (startM ?? 0);
  const endMinutes = (endH ?? 0) * 60 + (endM ?? 0);

  if (startMinutes <= endMinutes) {
    // Same day: e.g., 09:00 - 17:00
    return currentMinutes >= startMinutes && currentMinutes < endMinutes;
  }
  // Overnight: e.g., 22:00 - 07:00
  return currentMinutes >= startMinutes || currentMinutes < endMinutes;
}

/**
 * Check if notifications should be shown right now.
 * Returns false if notifications are disabled, DND is active, or the room is muted.
 */
export async function shouldNotify(roomId?: string): Promise<boolean> {
  const config = await loadConfig();
  if (!config.enabled) return false;
  if (config.dnd_enabled && isInDndWindow(config.dnd_start, config.dnd_end)) return false;
  if (roomId && config.muted_rooms.includes(roomId)) return false;
  return true;
}

/**
 * Check if notification sounds should play.
 */
export async function shouldPlaySound(): Promise<boolean> {
  const config = await loadConfig();
  return config.enabled && config.sound_enabled;
}
