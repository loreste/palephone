import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock getConfig
vi.mock("@/lib/tauri", () => ({
  getConfig: vi.fn(),
}));

describe("notifications", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("shouldNotify returns true when notifications enabled and no DND", async () => {
    const { getConfig } = await import("@/lib/tauri");
    (getConfig as any).mockResolvedValue({
      notifications: {
        enabled: true,
        sound_enabled: true,
        dnd_enabled: false,
        dnd_start: "22:00",
        dnd_end: "07:00",
        muted_rooms: [],
      },
    });

    const { shouldNotify } = await import("@/lib/notifications");
    expect(await shouldNotify()).toBe(true);
  });

  it("shouldNotify returns false when notifications disabled", async () => {
    const { getConfig } = await import("@/lib/tauri");
    (getConfig as any).mockResolvedValue({
      notifications: {
        enabled: false,
        sound_enabled: true,
        dnd_enabled: false,
        dnd_start: "22:00",
        dnd_end: "07:00",
        muted_rooms: [],
      },
    });

    const { shouldNotify } = await import("@/lib/notifications");
    expect(await shouldNotify()).toBe(false);
  });

  it("shouldNotify returns false for muted room", async () => {
    const { getConfig } = await import("@/lib/tauri");
    (getConfig as any).mockResolvedValue({
      notifications: {
        enabled: true,
        sound_enabled: true,
        dnd_enabled: false,
        dnd_start: "22:00",
        dnd_end: "07:00",
        muted_rooms: ["!room1:example.com"],
      },
    });

    const { shouldNotify } = await import("@/lib/notifications");
    expect(await shouldNotify("!room1:example.com")).toBe(false);
    expect(await shouldNotify("!room2:example.com")).toBe(true);
  });

  it("shouldPlaySound returns false when sound disabled", async () => {
    const { getConfig } = await import("@/lib/tauri");
    (getConfig as any).mockResolvedValue({
      notifications: {
        enabled: true,
        sound_enabled: false,
        dnd_enabled: false,
        dnd_start: "22:00",
        dnd_end: "07:00",
        muted_rooms: [],
      },
    });

    const { shouldPlaySound } = await import("@/lib/notifications");
    expect(await shouldPlaySound()).toBe(false);
  });
});
