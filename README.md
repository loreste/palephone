# Pale

A secure, self-hosted unified communications platform for organizations that need complete control over their voice, video, messaging, and file-sharing infrastructure.

## Why Pale?

Most business communication tools force you to choose: either use a cloud service that routes your data through someone else's servers, or cobble together separate apps for calling, chat, and file sharing that don't talk to each other.

Pale exists because:

- **Your calls shouldn't be someone else's data.** Every voice and video call stays on your infrastructure. SIP signaling goes through your PBX, media flows peer-to-peer or through your media proxy. No third-party cloud in the middle.

- **Chat and files should be encrypted by default.** Not "encrypted in transit" where the server can read everything — actual end-to-end encryption where only sender and receiver can decrypt. Messages, files, and media are encrypted client-side before they ever leave the device.

- **One app, not five.** Calling, video, messaging, and file transfer in a single application. Click a contact in chat to start a call. Share a file in a conversation. No context switching between tools.

- **Works everywhere.** Same app on macOS, Windows, Linux, Android, and iOS. Your team uses whatever devices they have.

- **You own the infrastructure.** Pale connects to your SIP server (Obeyswan, FreePBX, Kamailio) and your Matrix homeserver (Synapse, Conduit, Dendrite). Self-host the entire stack or run it on-prem. Nothing phones home.

## Features

**Calling**
- Voice and video calls via SIP (PJSIP 2.14.1)
- UDP, TCP, and TLS transports
- SRTP media encryption with DTLS key exchange
- Hold, attended/blind transfer, DTMF, multi-line
- Call history with full metadata

**Messaging**
- End-to-end encrypted chat via Matrix protocol
- 1:1 direct messages and group rooms
- Olm (Double Ratchet) for 1:1, Megolm for groups
- Typing indicators and read receipts
- Device verification (emoji comparison)

**File Transfer**
- Encrypted file sharing through Matrix
- AES-256-CTR per-file encryption keys
- Drag-and-drop upload with progress tracking
- Images, documents, audio, video — any file type

**Desktop Experience**
- Modern dark-first UI with glassmorphic design
- System tray with status indicators
- Close-to-tray background operation
- Command palette (Cmd+K) and keyboard shortcuts
- Native OS notifications for calls and messages
- OS keychain for credential storage

**Mobile**
- Android and iOS via Tauri 2.x
- Adaptive UI with safe area support
- Background call handling with push notifications

## Encryption

Every layer is encrypted:

| Layer | What | How |
|-------|------|-----|
| SIP signaling | Call setup, registration | TLS (port 5061) |
| Matrix API | Chat, file metadata | HTTPS |
| Voice/video media | RTP audio and video streams | SRTP with DTLS key exchange |
| 1:1 messages | Direct chat messages | Olm (Double Ratchet protocol) |
| Group messages | Room messages | Megolm (ratcheted group encryption) |
| File attachments | Uploaded files | AES-256-CTR with per-file key |
| Credentials | SIP and Matrix passwords | OS keychain (never written to disk) |

Keys never leave the device. Your Matrix homeserver only sees ciphertext.

## Architecture

```
Frontend    React + TypeScript + Tailwind CSS + Zustand
Desktop     Tauri 2.x (Rust backend, OS-native webview)
Mobile      Tauri Android / iOS
SIP/Media   PJSIP 2.14.1 (compiled from source, linked via FFI)
Chat/Files  matrix-sdk 0.18 (Rust, E2E encryption via vodozemac)
Storage     SQLite (call history), JSON (config), OS keychain (passwords)
```

### Rust Crates

| Crate | What it does |
|-------|-------------|
| `pjsip-sys` | Downloads PJSIP source, compiles per-platform, generates FFI bindings via bindgen |
| `pale-core` | SIP engine with dedicated worker thread, call management, audio device control, call history (SQLite), config persistence, OS keychain integration |
| `pale-matrix` | Matrix client lifecycle, E2E encrypted messaging, file upload/download, room management, sync loop |

## Quick Start

### Prerequisites

- Node.js 22+
- Rust 1.93+
- autoconf, automake (for PJSIP build)
- Platform-specific: OpenSSL, Opus

```bash
# macOS
brew install openssl@3 opus autoconf automake

# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libpulse-dev libssl-dev libopus-dev autoconf automake

# Windows
# Install Visual Studio Build Tools, OpenSSL via vcpkg
```

### Build & Run

```bash
git clone git@github.com:loreste/palephone.git
cd palephone

npm install
npm run tauri dev       # Development with hot reload
npm run tauri build     # Production installer

# Tests
npm test                                                      # Frontend (14 tests)
cargo test --manifest-path src-tauri/Cargo.toml --workspace   # Rust (3 tests)
```

### Android

```bash
# Install JDK 17+, Android SDK 34, NDK 27
rustup target add aarch64-linux-android

npm run tauri android init
npm run tauri android dev     # USB debugging
npm run tauri android build   # Release APK
```

See [ANDROID_SETUP.md](ANDROID_SETUP.md) for detailed environment setup.

## Infrastructure Requirements

Pale connects to two servers that you run:

| Server | Purpose | Options |
|--------|---------|---------|
| **SIP Server** | Voice/video calling | Obeyswan, FreePBX, Kamailio, Asterisk |
| **Matrix Homeserver** | Chat, files, E2E encryption | Synapse, Conduit, Dendrite |

Both can run on the same machine. A minimal deployment is a single Linux server running Obeyswan + Conduit behind a reverse proxy.

## Project Structure

```
pale/
  src/                      React frontend (55 files)
    components/             33 UI components
    store/                  7 Zustand state stores
    hooks/                  7 custom hooks
    lib/                    Tauri IPC wrappers + utilities
    test/                   Vitest test suite
  src-tauri/                Rust backend
    src/                    Tauri app entry (28 IPC commands)
    crates/
      pjsip-sys/            PJSIP FFI bindings
      pale-core/            SIP engine + persistence
      pale-matrix/          Matrix chat + file transfer
  .github/workflows/        CI/CD pipelines
  assets/                   Logo + tray icons
```

## CI/CD

GitHub Actions builds for all platforms automatically:

| Workflow | Trigger | Produces |
|----------|---------|----------|
| `ci.yml` | Push / PR to main | Build + test on macOS, Windows, Linux |
| `release.yml` | Git tag `v*` | .dmg, .msi, .exe, .deb, .AppImage |
| `android.yml` | Push / PR to main | .apk |

Tag a release to build installers:

```bash
git tag v0.1.0
git push --tags
```

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — SIP/media stack, codec negotiation, OS-specific details
- [ARCHITECTURE_V2.md](ARCHITECTURE_V2.md) — Video, Matrix chat, file transfer, E2E encryption design
- [UI_UX_SPEC.md](UI_UX_SPEC.md) — Design system, component wireframes, interaction patterns
- [ANDROID_SETUP.md](ANDROID_SETUP.md) — Android development environment setup

## License

This project is licensed under the GNU General Public License v2.0. See [LICENSE](LICENSE) for details.
