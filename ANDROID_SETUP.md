# Pale Android — Setup Guide

## Prerequisites

### 1. Install JDK 17+

```bash
# macOS (requires sudo)
brew install --cask temurin

# Or download from: https://adoptium.net/temurin/releases/
```

### 2. Install Android SDK + NDK

```bash
# Install Android Studio (includes SDK) — recommended
brew install --cask android-studio

# Or install SDK command-line tools only:
brew install --cask android-commandlinetools

# After install, set ANDROID_HOME:
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"

# Install required SDK packages:
sdkmanager "platforms;android-34" "build-tools;34.0.0" "ndk;27.0.12077973"
```

### 3. Install Rust Android targets

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
```

### 4. Verify the local Android toolchain

```bash
java -version
test -d "$ANDROID_HOME"
test -d "$NDK_HOME"
```

All three checks must pass before Tauri can initialize or build the Android project.

### 5. Initialize Tauri Android

```bash
cd /Users/loreste/palephone
npm run android:init
```

This generates the `src-tauri/gen/android/` directory with the Gradle project.

### 6. Build and run

```bash
# Development (USB debugging)
npm run android:dev

# Production APK
npm run android:build
```

## Android-Specific Configuration

### Permissions (AndroidManifest.xml)
The following permissions are configured automatically:

- `INTERNET` — SIP signaling + Matrix API
- `RECORD_AUDIO` — Microphone for calls
- `CAMERA` — Video calls
- `VIBRATE` — Incoming call notification
- `FOREGROUND_SERVICE` — Keep calls alive in background
- `WAKE_LOCK` — Prevent sleep during calls
- `ACCESS_NETWORK_STATE` — Network status detection
- `POST_NOTIFICATIONS` — Android 13+ notification permission

### Architecture Targets

| Target | ABI | Notes |
|--------|-----|-------|
| `aarch64-linux-android` | arm64-v8a | Most modern phones |
| `armv7-linux-androideabi` | armeabi-v7a | Older 32-bit phones |
| `x86_64-linux-android` | x86_64 | Emulator |

### PJSIP Cross-Compilation

The `pjsip-sys` build.rs automatically detects Android targets and:
1. Uses the NDK toolchain (`$NDK_HOME/toolchains/llvm/prebuilt/`)
2. Sets `--host=aarch64-linux-android` for configure
3. Uses OpenSLES audio backend (Android native audio)
4. Links against `libOpenSLES`, `liblog`, `libandroid`
