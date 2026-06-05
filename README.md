# Pale

Secure unified communications for desktop and mobile. Voice, video, chat, and file sharing with end-to-end encryption.

## Features

- **Voice & Video Calls** SIP-based calling with SRTP media encryption. UDP, TCP, and TLS transports. Hold, transfer, DTMF, multi-line.
- **E2E Encrypted Chat** Matrix protocol with Olm/Megolm encryption. 1:1 and group messaging.
- **Encrypted File Transfer** Files encrypted client-side before upload. AES-256-CTR per-file keys.
- **Cross-Platform** macOS, Windows, Linux, Android. Single codebase.
- **Self-Hosted** Connects to your own SIP server and Matrix homeserver. No third-party dependencies.

## Architecture

```
Frontend    React + TypeScript + Tailwind CSS + Zustand
Desktop     Tauri 2.x (Rust backend, OS-native webview)
Mobile      Tauri Android/iOS
SIP/Media   PJSIP 2.14.1 (compiled from source via FFI)
Chat/Files  matrix-sdk 0.18 (Rust, E2E encryption built-in)
```

Three Rust crates:

| Crate | Purpose |
|-------|---------|
| `pjsip-sys` | Auto-downloads, compiles PJSIP, generates FFI bindings via bindgen |
| `pale-core` | SIP engine, call management, call history (SQLite), config persistence, OS keychain |
| `pale-matrix` | Matrix client, E2E encrypted chat, file transfer, room management |

## Quick Start

```bash
# Prerequisites: Node.js 22+, Rust 1.93+, autoconf, automake
# macOS: brew install openssl@3 opus autoconf automake

# Install dependencies
npm install

# Run in development
npm run tauri dev

# Build for production
npm run tauri build

# Run tests
npm test                          # Frontend (Vitest)
cargo test --manifest-path src-tauri/Cargo.toml --workspace  # Rust
```

## Android

```bash
# Prerequisites: JDK 17+, Android SDK 34, NDK 27
rustup target add aarch64-linux-android

npm run tauri android init
npm run tauri android dev    # USB debugging
npm run tauri android build  # Release APK
```

See [ANDROID_SETUP.md](ANDROID_SETUP.md) for detailed setup.

## Encryption

| Layer | Scope | Method |
|-------|-------|--------|
| Transport | SIP signaling | TLS (port 5061) |
| Transport | Matrix API | HTTPS |
| Media | Voice/video streams | SRTP with DTLS key exchange |
| Messages | 1:1 chat | Olm (Double Ratchet) |
| Messages | Group chat | Megolm |
| Files | Attachments | AES-256-CTR per-file key |

Passwords are stored in the OS keychain (macOS Keychain, Windows Credential Manager, Linux libsecret). Never written to disk.

## Project Structure

```
pale/
  src/                    React frontend
    components/           UI components (33 total)
    store/                Zustand stores (7)
    hooks/                Custom hooks (7)
    lib/                  Utilities + Tauri IPC wrappers
  src-tauri/              Rust backend
    src/                  Tauri app entry + IPC commands
    crates/
      pjsip-sys/          PJSIP FFI bindings
      pale-core/          SIP engine + persistence
      pale-matrix/        Matrix chat + files
  .github/workflows/      CI/CD (desktop + Android)
  assets/                 Logo + tray icons (SVG)
```

## CI/CD

GitHub Actions workflows build for all platforms on push:

| Workflow | Trigger | Output |
|----------|---------|--------|
| `ci.yml` | Push/PR to main | Build + test (macOS, Windows, Linux) |
| `release.yml` | Tag `v*` | .dmg, .msi, .exe, .deb, .AppImage |
| `android.yml` | Push/PR to main | .apk |

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) Core technical architecture
- [ARCHITECTURE_V2.md](ARCHITECTURE_V2.md) Video, chat, file transfer, E2E encryption
- [UI_UX_SPEC.md](UI_UX_SPEC.md) Design system, component specs, interaction patterns
- [ANDROID_SETUP.md](ANDROID_SETUP.md) Android development environment setup

## License

Private. All rights reserved.
