import { useEffect, useState } from "react";
import { isMobile } from "./usePlatform";

/**
 * Request Android runtime permissions for microphone and camera.
 * On desktop this is a no-op.
 *
 * Android permissions are requested via the Tauri Android bridge.
 * The actual permission request happens natively; this hook tracks the state.
 */
export function useAndroidPermissions() {
  const [micGranted, setMicGranted] = useState(true);
  const [cameraGranted, setCameraGranted] = useState(true);

  useEffect(() => {
    if (!isMobile()) return;

    // On Android, permissions are requested when the feature is first used.
    // Tauri handles the permission dialog automatically for:
    // - RECORD_AUDIO (triggered when PJSIP opens the audio device)
    // - CAMERA (triggered when PJSIP opens the video device)
    //
    // We can also explicitly check/request via navigator.permissions API:
    if (navigator.permissions) {
      navigator.permissions
        .query({ name: "microphone" as PermissionName })
        .then((result) => {
          setMicGranted(result.state === "granted");
          result.onchange = () => setMicGranted(result.state === "granted");
        })
        .catch(() => {});

      navigator.permissions
        .query({ name: "camera" as PermissionName })
        .then((result) => {
          setCameraGranted(result.state === "granted");
          result.onchange = () => setCameraGranted(result.state === "granted");
        })
        .catch(() => {});
    }
  }, []);

  return { micGranted, cameraGranted };
}
