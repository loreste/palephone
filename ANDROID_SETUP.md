# Android setup

Pale uses Tauri Mobile for the Android client. The Android build packages the same React interface as the desktop app and compiles the Rust backend for Android.

The current Android CI builds an arm64 APK. That proves the project can package for Android, but it does not replace testing on a real phone with microphone, camera, notification, network, and background-mode permissions.

## Requirements

- Node.js 22
- Rust stable
- JDK 17
- Android SDK command-line tools
- Android NDK `27.2.12479018`
- Android platform API 35 or newer
- Android build tools 35 or newer

Set these environment variables before building:

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk/27.2.12479018"
export NDK_HOME="$ANDROID_NDK_ROOT"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
```

Install the Rust targets used by the Android build:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
```

Install JavaScript dependencies:

```bash
npm ci
```

## Initialize the Android project

Tauri generates the Android Gradle project under `src-tauri/gen/android`.

```bash
npm run tauri android init
```

Run this once per checkout, or again after changing Tauri mobile configuration.

## Build an APK

For the supported arm64 build:

```bash
npm run tauri android build -- --target aarch64 --apk
```

The APK is written under:

```text
src-tauri/gen/android/app/build/outputs/apk/
```

## Run on a phone

Enable USB debugging on the phone, connect it, then run:

```bash
adb devices
npm run tauri android dev
```

If the device does not appear in `adb devices`, fix the USB debugging, cable, or driver issue before debugging Pale.

## Permissions to verify on device

Before considering an Android build usable, test these flows on real hardware:

- Sign in to a Pale server over HTTPS.
- Keep the app installed, rebuild/update it, and confirm the saved server/account settings remain.
- Open Calls, Chat, People, Files, Calendar, Settings, and Admin for an admin user.
- Grant microphone permission and place a test call.
- Grant camera permission and start a video-capable flow.
- Enable notifications and confirm incoming call/message notification behavior.
- Lock the phone and confirm the expected background behavior for your deployment.

## Troubleshooting

If the build cannot find the NDK, check `ANDROID_HOME`, `ANDROID_NDK_ROOT`, and `NDK_HOME`.

If Rust reports that `core` cannot be found for an Android target, install the missing target with `rustup target add`.

If Gradle fails after a Tauri upgrade, regenerate the Android project with `npm run tauri android init`.

If the APK builds but the app cannot connect to the server, confirm the server URL uses HTTPS with a certificate trusted by Android. Self-signed or private certificates must be installed on the device or replaced with a public CA certificate.
