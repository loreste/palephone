import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { getCurrentWindow } from "@tauri-apps/api/window";

// ─── Native OS notifications (Teams-style) ───
//
// Chat/meeting notifications previously used the web Notification API, which is
// unreliable inside a Tauri webview. Use the native notification plugin so OS
// banners appear whether or not the window is focused, with a web fallback for
// a plain browser build.

let permissionChecked = false;
let permissionGranted = false;

async function ensurePermission(): Promise<boolean> {
  if (permissionChecked) return permissionGranted;
  permissionChecked = true;
  try {
    permissionGranted = await isPermissionGranted();
    if (!permissionGranted) {
      permissionGranted = (await requestPermission()) === "granted";
    }
  } catch {
    permissionGranted = false;
  }
  return permissionGranted;
}

/** Show a native OS notification, falling back to the web API when needed. */
export async function notify(title: string, body?: string): Promise<void> {
  try {
    if (await ensurePermission()) {
      sendNotification(body ? { title, body } : { title });
      return;
    }
  } catch {
    /* fall through to web fallback */
  }
  try {
    if (typeof Notification === "undefined") return;
    if (Notification.permission === "granted") {
      new Notification(title, body ? { body } : undefined);
    } else if (Notification.permission !== "denied") {
      const perm = await Notification.requestPermission();
      if (perm === "granted") new Notification(title, body ? { body } : undefined);
    }
  } catch {
    /* ignore */
  }
}

let cachedWindow: ReturnType<typeof getCurrentWindow> | null = null;
function appWindow(): ReturnType<typeof getCurrentWindow> | null {
  try {
    if (!cachedWindow) cachedWindow = getCurrentWindow();
    return cachedWindow;
  } catch {
    return null;
  }
}

/** Set the app icon badge (dock/taskbar) to the unread count; clears at 0. */
export async function setAppBadge(count: number): Promise<void> {
  try {
    await appWindow()?.setBadgeCount(count > 0 ? count : undefined);
  } catch {
    /* unsupported platform / not tauri */
  }
}

/** Whether the app window currently has focus (used to suppress redundant alerts). */
export async function isWindowFocused(): Promise<boolean> {
  try {
    const win = appWindow();
    if (win) return await win.isFocused();
  } catch {
    /* fall through */
  }
  return typeof document !== "undefined" ? document.hasFocus() : true;
}
