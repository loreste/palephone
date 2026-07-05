/**
 * Web Push subscription management.
 *
 * Fetches the VAPID public key from pale-server, subscribes to push via the
 * service worker PushManager, and registers the subscription with the server.
 */

import { paleServerApi } from "./tauri";

/** Convert a URL-safe base64 VAPID key to a Uint8Array for PushManager. */
function urlBase64ToUint8Array(base64String: string): Uint8Array {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  const output = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) {
    output[i] = raw.charCodeAt(i);
  }
  return output;
}

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export interface PushVapidKeyResponse {
  vapid_public_key: string;
  enabled: boolean;
}

/**
 * Subscribe to Web Push notifications.
 *
 * 1. Fetches the VAPID public key from the server.
 * 2. Requests notification permission if needed.
 * 3. Subscribes via PushManager.
 * 4. Sends the subscription to the server.
 */
export async function subscribeToPush(
  baseUrl: string,
  token: string,
): Promise<boolean> {
  // Only works in secure contexts with service worker support
  if (typeof window === "undefined" || !("serviceWorker" in navigator)) {
    return false;
  }

  // Skip in Tauri desktop (uses native notifications instead)
  if ((window as any).__TAURI_INTERNALS__) {
    return false;
  }

  try {
    // Fetch VAPID key
    const vapidResponse = await paleServerApi<PushVapidKeyResponse>(
      baseUrl,
      token,
      "/v1/push/vapid-key",
    );
    if (!vapidResponse.enabled || !vapidResponse.vapid_public_key) {
      return false;
    }

    // Request notification permission
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      return false;
    }

    // Get the service worker registration
    const registration = await navigator.serviceWorker.ready;

    // Subscribe to push
    const subscription = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToUint8Array(vapidResponse.vapid_public_key),
    });

    // Extract keys
    const p256dhKey = subscription.getKey("p256dh");
    const authKey = subscription.getKey("auth");
    if (!p256dhKey || !authKey) {
      console.warn("Push subscription missing keys");
      return false;
    }

    // Register with server
    await paleServerApi(baseUrl, token, "/v1/push/subscribe", {
      method: "POST",
      body: {
        endpoint: subscription.endpoint,
        p256dh: arrayBufferToBase64(p256dhKey),
        auth: arrayBufferToBase64(authKey),
      },
    });

    return true;
  } catch (err) {
    console.warn("Push subscription failed:", err);
    return false;
  }
}

/**
 * Unsubscribe from Web Push notifications.
 */
export async function unsubscribeFromPush(
  baseUrl: string,
  token: string,
): Promise<boolean> {
  if (typeof window === "undefined" || !("serviceWorker" in navigator)) {
    return false;
  }

  try {
    const registration = await navigator.serviceWorker.ready;
    const subscription = await registration.pushManager.getSubscription();
    if (!subscription) {
      return false;
    }

    // Unsubscribe from browser
    await subscription.unsubscribe();

    // Unsubscribe from server
    await paleServerApi(baseUrl, token, "/v1/push/unsubscribe", {
      method: "DELETE",
      body: { endpoint: subscription.endpoint },
    });

    return true;
  } catch (err) {
    console.warn("Push unsubscribe failed:", err);
    return false;
  }
}
