# Android packaging

## PJSIP video (camera + OpenGL)

| Path | Role |
|------|------|
| `packaging/android/java/org/pjsip/*.java` | PJSIP camera JNI classes (`PjCamera2`, etc.) |
| `src-tauri/android/PaleJni.kt` | Early `JNI_OnLoad` companion + `prepare()` |
| `src-tauri/android/PaleVideoOverlay.kt` | SurfaceView overlays for remote/local video |
| `src-tauri/crates/pale-core/src/android_jni.rs` | Rust `JNI_OnLoad`, ClassLoader cache, `pale_android_find_class` |

CI (`.github/workflows/android.yml`) copies Java/Kotlin into `src-tauri/gen/android` after `tauri android init` and hooks `PaleJni.prepare(this)` into `MainActivity.onCreate`.

Native side re-enables `PJMEDIA_VIDEO_DEV_HAS_ANDROID` + OpenGL renderer, disables PJSIP's own `JNI_OnLoad` (Pale owns it), and patches `android_dev.c` so `FindClass` uses the app ClassLoader from the PJSIP worker thread.

### Live video call path

1. `PaleJni.prepare` registers `CameraManager` and attaches overlays.
2. Video call / answer uses `vid_cnt = 1`.
3. On media active, `android_video::bind_call_video`:
   - shows remote + local SurfaceViews
   - `pjsua_vid_win_set_win` binds remote stream to remote Surface (`ANativeWindow`)
   - `pjsua_vid_preview_start` draws local capture onto the PIP Surface

## Sideload signing certificate

`pale-sideload.jks` is a **public sideload** signing key for CI and website
APKs that users install outside Google Play.

| Property | Value |
|----------|--------|
| File | `packaging/android/pale-sideload.jks` |
| Alias | `pale` |
| Store / key password | `palesideload` |
| Purpose | Sideload / lab only |

**Do not** use this keystore for Google Play or MDM production fleets. For
Play Store or enterprise MDM, generate a private keystore, store it in CI
secrets (`PALE_ANDROID_KEYSTORE` base64 + passwords), and never commit it.

## Sign an APK

```bash
./scripts/sign-android-apk.sh path/to/app-release-unsigned.apk dist/Pale_android-signed.apk
```

CI runs the same script after `tauri android build` so uploaded artifacts
are installable on modern Android (signature schemes v2/v3).

## Install notes (Android 14+)

1. Download the **signed** APK (not `*-unsigned.apk`).
2. Enable **Install unknown apps** for your browser or Files app.
3. If an older Pale build used a different certificate, uninstall the old app first.
4. On some OEM devices (Honor, Xiaomi, etc.) also disable extra “pure mode”
   / “external sources” restrictions for the installer.
