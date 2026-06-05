import { useState, useEffect } from "react";

export type Platform = "desktop" | "android" | "ios";

/**
 * Detect whether we're running on mobile (Android/iOS) or desktop.
 * Uses Tauri's user agent and window size heuristics.
 */
export function usePlatform(): Platform {
  const [platform, setPlatform] = useState<Platform>("desktop");

  useEffect(() => {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes("android") || (window as any).__TAURI_INTERNALS__?.metadata?.currentDevice === "android") {
      setPlatform("android");
    } else if (ua.includes("iphone") || ua.includes("ipad")) {
      setPlatform("ios");
    } else {
      setPlatform("desktop");
    }
  }, []);

  return platform;
}

export function isMobile(): boolean {
  const ua = navigator.userAgent.toLowerCase();
  return ua.includes("android") || ua.includes("iphone") || ua.includes("ipad");
}
