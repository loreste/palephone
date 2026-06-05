# Pale v2 — Unified Communications Architecture

> **Version:** 2.0 — June 2026
> **Scope:** Voice + Video calling, Encrypted chat, File transfer, E2E encryption
> **Approach:** Hybrid SIP + Matrix protocol

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Protocol Architecture](#2-protocol-architecture)
3. [Video Calling](#3-video-calling)
4. [Chat & Messaging](#4-chat--messaging)
5. [File Transfer](#5-file-transfer)
6. [E2E Encryption](#6-e2e-encryption)
7. [Rust Crate Architecture](#7-rust-crate-architecture)
8. [Frontend Architecture](#8-frontend-architecture)
9. [Implementation Phases](#9-implementation-phases)

---

## 1. System Overview

Pale v2 becomes a full unified communications client — like WhatsApp for desktop — combining:

- **Voice calls** (existing SIP/RTP/SRTP stack via PJSIP)
- **Video calls** (PJSIP video with VP8/H.264 + native rendering)
- **Encrypted chat** (Matrix protocol via matrix-rust-sdk)
- **File transfer** (Matrix encrypted attachments)
- **E2E encryption** (Vodozemac/Olm/Megolm for Matrix, SRTP-DTLS for media)

```
┌─────────────────────────────────────────────────────────────┐
│                     Pale Desktop App                        │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              React Frontend (Tauri Webview)           │  │
│  │                                                       │  │
│  │  ┌─────────┐ ┌──────────┐ ┌──────┐ ┌──────────────┐  │  │
│  │  │ Dialpad │ │ Chat     │ │Video │ │ File Manager │  │  │
│  │  │ + Calls │ │ Messages │ │ View │ │ + Transfers  │  │  │
│  │  └─────────┘ └──────────┘ └──────┘ └──────────────┘  │  │
│  └────────────────────┬──────────────────────────────────┘  │
│                       │ Tauri IPC                            │
│  ┌────────────────────┴──────────────────────────────────┐  │
│  │                   Rust Backend                        │  │
│  │                                                       │  │
│  │  ┌──────────────┐    ┌─────────────────────────────┐  │  │
│  │  │  pale-core   │    │     pale-matrix             │  │  │
│  │  │  (SIP/PJSIP) │    │  (matrix-rust-sdk)          │  │  │
│  │  │              │    │                             │  │  │
│  │  │ Voice calls  │    │  Chat messages (E2E)       │  │  │
│  │  │ Video calls  │    │  File transfer (E2E)       │  │  │
│  │  │ SRTP/DTLS    │    │  Key verification          │  │  │
│  │  │ PJSIP FFI   │    │  Room management           │  │  │
│  │  └──────┬───────┘    └──────────┬──────────────────┘  │  │
│  │         │                       │                      │  │
│  │         ▼                       ▼                      │  │
│  │    SIP Server              Matrix Homeserver           │  │
│  │    (UDP/TCP/TLS)           (HTTPS + E2E)               │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Why Hybrid SIP + Matrix?

| Concern | SIP Only | Matrix Only | **Hybrid (chosen)** |
|---------|----------|-------------|---------------------|
| Voice quality | Excellent (PJSIP, proven) | Good (WebRTC) | **Best: PJSIP for voice** |
| Video | PJSIP video (mature) | WebRTC in Matrix | **PJSIP video** |
| Chat | SIP MESSAGE (limited) | Excellent (native) | **Matrix for chat** |
| File transfer | Not standard in SIP | Native, encrypted | **Matrix for files** |
| E2E encryption | SRTP only (media) | Olm/Megolm (proven) | **Both: SRTP + Megolm** |
| Offline messages | No | Yes | **Matrix stores offline** |
| Group chat | No | Yes | **Matrix rooms** |
| Interop w/ PBX | Yes | No | **SIP for PBX calling** |
| Self-hostable | Yes (Obeyswan) | Yes (Synapse/Conduit) | **Both** |

---

## 2. Protocol Architecture

### SIP Stack (existing — enhanced with video)

```
Voice/Video Calls:
  User → PJSIP → SIP INVITE (with video SDP) → SIP Server
  Media → RTP/SRTP → Direct peer-to-peer (or via media proxy)
  Encryption → SRTP with DTLS key exchange
```

### Matrix Stack (new)

```
Chat/Files:
  User → matrix-rust-sdk → Matrix Client-Server API (HTTPS)
  Messages → Olm (1:1) / Megolm (group) E2E encryption
  Files → Encrypted upload to Matrix content repository
  Sync → Long-polling /sync endpoint for real-time updates
```

### Authentication Flow

```
1. User opens Pale for the first time
2. Configures SIP account (voice/video calling) — existing flow
3. Logs into Matrix homeserver (chat/files):
   - Username + password, or
   - SSO (OIDC), or
   - QR code cross-sign from mobile
4. Matrix SDK handles E2E key generation + device verification
5. Both connections run simultaneously
```

---

## 3. Video Calling

### 3.1 PJSIP Video Re-enablement

Currently PJSIP is built with `--disable-video`. To enable:

**Build changes (`pjsip-sys/build.rs`):**
- Remove `--disable-video` from configure flags
- Add video codec dependencies:
  - **VP8/VP9**: via `libvpx` (`brew install libvpx` / `apt install libvpx-dev`)
  - **H.264**: via `libopenh264` (`brew install openh264`)
- Add `config_site.h` defines:
  ```c
  #define PJMEDIA_HAS_VIDEO 1
  #define PJMEDIA_VIDEO_DEV_HAS_DARWIN 1   // macOS AVFoundation
  #define PJMEDIA_HAS_OPENH264_CODEC 1
  #define PJMEDIA_HAS_VPX_CODEC 1
  ```

**Video rendering architecture:**

```
┌──────────────────────────────────┐
│  Camera Capture                  │
│  (AVFoundation / V4L2 / MSMF)   │
│  Managed by PJSIP               │
└──────────┬───────────────────────┘
           │ Raw frames
           ▼
┌──────────────────────────────────┐
│  Video Encoder (VP8 or H.264)   │
│  PJSIP pjmedia_vid_codec        │
└──────────┬───────────────────────┘
           │ Encoded RTP
           ▼
┌──────────────────────────────────┐
│  SRTP Encryption                 │
│  (DTLS-SRTP key exchange)        │
└──────────┬───────────────────────┘
           │ Encrypted RTP
           ▼
        Network (UDP)
           │
           ▼
┌──────────────────────────────────┐
│  SRTP Decryption                 │
└──────────┬───────────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  Video Decoder                   │
└──────────┬───────────────────────┘
           │ Raw frames (I420/NV12)
           ▼
┌──────────────────────────────────┐
│  Native Video Renderer           │
│  - macOS: CALayer / Metal        │
│  - Windows: Direct3D 11          │
│  - Linux: OpenGL / Wayland       │
│  Rendered in a separate native   │
│  window overlaid on the webview  │
└──────────────────────────────────┘
```

**Key decision: Video rendering outside the webview.**
Rendering video inside the webview (via Canvas/WebGL) adds latency and CPU overhead from frame copying between native → JS. Instead, render in a native window/layer that overlays the webview. PJSIP's video device API supports this natively.

### 3.2 Video UI Components

```
┌──────────────────────────────────────┐
│  Video Call View                     │
│  ┌────────────────────────────────┐  │
│  │                                │  │
│  │     Remote Video               │  │  Full-size remote video
│  │     (native overlay)           │  │
│  │                                │  │
│  │                    ┌────────┐  │  │
│  │                    │ Self   │  │  │  PiP self-view (draggable)
│  │                    │ View   │  │  │
│  │                    └────────┘  │  │
│  └────────────────────────────────┘  │
│                                      │
│  ┌────────────────────────────────┐  │
│  │  🎤  📹  🖥️  📞  ⏸️           │  │  Controls: Mute, Camera,
│  │ Mute Video Screen Hangup Hold │  │  Screen share, Hangup, Hold
│  └────────────────────────────────┘  │
└──────────────────────────────────────┘
```

---

## 4. Chat & Messaging

### 4.1 Matrix Integration via matrix-rust-sdk

**Rust crate: `pale-matrix`**

Uses `matrix-rust-sdk` — the official Rust SDK for Matrix, maintained by Element. It provides:
- Full Matrix Client-Server API
- Olm/Megolm E2E encryption (via `vodozemac`, a Rust implementation)
- Room management, sync, timeline
- Sliding sync for efficient message loading
- Cross-signing and device verification

```toml
# pale-matrix/Cargo.toml
[dependencies]
matrix-sdk = { version = "0.10", features = ["e2e-encryption", "sqlite"] }
```

### 4.2 Chat Data Flow

```
User types message
    → Frontend: chatStore.sendMessage(roomId, text)
    → Tauri IPC: invoke("send_message", { roomId, body })
    → pale-matrix: client.get_room(roomId).send(text)
    → matrix-rust-sdk encrypts with Megolm session key
    → HTTPS PUT to /_matrix/client/v3/rooms/{roomId}/send
    → Homeserver distributes to room members

Receiving:
    → matrix-rust-sdk /sync long-poll receives event
    → Decrypts with Megolm session key
    → Emits PaleMatrixEvent::Message { room_id, sender, body, timestamp }
    → Tauri event "matrix://message"
    → Frontend: chatStore updates, UI re-renders
```

### 4.3 Chat UI Components

```
Expanded Mode (900px+):
┌────────────────────┬─────────────────────────────────────┐
│  Conversations     │  Chat View                          │
│  ┌──────────────┐  │  ┌─────────────────────────────┐   │
│  │ 🔍 Search    │  │  │ Alice Smith         Online  │   │
│  ├──────────────┤  │  ├─────────────────────────────┤   │
│  │ Alice Smith  │  │  │                             │   │
│  │ "Hey, can...│  │  │  Alice: Hey, can you join   │   │
│  │ 2m ago      │  │  │  the call?                  │   │
│  ├──────────────┤  │  │                    10:24 AM │   │
│  │ Team Chat   │  │  │                             │   │
│  │ Bob: Sure.. │  │  │  You: Sure, joining now     │   │
│  │ 5m ago      │  │  │                    10:25 AM │   │
│  ├──────────────┤  │  │                             │   │
│  │ Support     │  │  │  📎 Alice sent a file       │   │
│  │ File shared │  │  │  report.pdf (2.4 MB)        │   │
│  │ 1h ago      │  │  │  [Download]   10:26 AM      │   │
│  └──────────────┘  │  │                             │   │
│                    │  ├─────────────────────────────┤   │
│                    │  │ 📎 [Type a message...]  ➤  │   │
│                    │  │     Attach  Send            │   │
│                    │  └─────────────────────────────┘   │
├────────────────────┴─────────────────────────────────────┤
│  ☎ Dial  💬 Chat  📁 Files  ⏱ Recent  ⚙ Settings      │
└──────────────────────────────────────────────────────────┘
```

---

## 5. File Transfer

### 5.1 Matrix Encrypted Attachments

Matrix supports encrypted file uploads natively:

```
Upload flow:
1. User selects file via native file picker (Tauri dialog)
2. pale-matrix generates a one-time AES-256-CTR key
3. File is encrypted client-side with this key
4. Encrypted blob is uploaded to Matrix content repository (mxc:// URI)
5. Message event is sent to the room containing:
   - mxc:// URI (encrypted blob location)
   - AES key + IV + SHA-256 hash (inside the E2E encrypted event)
6. Only room members with Megolm session keys can decrypt

Download flow:
1. Recipient receives the message event (Megolm-decrypted)
2. Extracts mxc:// URI + AES key from the event
3. Downloads encrypted blob from homeserver
4. Decrypts with AES-256-CTR key
5. Saves to user's download directory
```

**Supported file types:**
- Documents (PDF, DOCX, etc.)
- Images (PNG, JPG, GIF — with thumbnails)
- Audio messages (voice notes)
- Video clips
- Any arbitrary file

**Size limits:** Configurable per homeserver (default 50MB on Synapse). For larger files, chunked upload with progress tracking.

### 5.2 File Transfer UI

```
┌──────────────────────────────────────┐
│  📁 Files                            │
│  ┌──────────────────────────────────┐│
│  │ 🔍 Search files...               ││
│  ├──────────────────────────────────┤│
│  │ 📄 report.pdf          2.4 MB   ││
│  │    From Alice · Today 10:26 AM   ││
│  │    [Open] [Save As]              ││
│  ├──────────────────────────────────┤│
│  │ 🖼️ screenshot.png      450 KB   ││
│  │    From Bob · Yesterday 3:15 PM  ││
│  │    [Preview] [Save As]           ││
│  ├──────────────────────────────────┤│
│  │ 📦 project.zip         12.1 MB  ││
│  │    From You · Jun 2, 9:00 AM     ││
│  │    Sent to Team Chat             ││
│  └──────────────────────────────────┘│
└──────────────────────────────────────┘
```

---

## 6. E2E Encryption

### 6.1 Encryption Layers

```
Layer 1: Transport Encryption
  ├── SIP signaling: TLS (port 5061)
  ├── Matrix API: HTTPS
  └── Always on, non-optional

Layer 2: Media Encryption
  ├── Voice/Video RTP: SRTP with DTLS key exchange
  └── Encrypts media stream, keys negotiated per-call

Layer 3: Message & File Encryption (E2E)
  ├── 1:1 chats: Olm (Double Ratchet, like Signal Protocol)
  ├── Group chats: Megolm (ratcheted group encryption)
  ├── File attachments: AES-256-CTR per-file key
  └── Keys NEVER leave the device — homeserver sees only ciphertext
```

### 6.2 Key Management

```
Device setup:
1. First login → generate Ed25519 signing key + Curve25519 identity key
2. Upload public keys to homeserver
3. Generate 100 one-time prekeys (Curve25519)
4. Cross-sign with other devices (QR code or emoji verification)

Per-session (1:1):
  Olm session established via X3DH-like key agreement
  → Double Ratchet provides forward secrecy
  → Each message encrypted with a unique key

Per-room (group):
  Megolm session created by sender
  → Session key shared via Olm to each room member
  → Ratchets forward only (no backward secrecy per message,
     but new sessions are created periodically)

Key backup:
  → Optional: encrypted key backup to homeserver
  → Protected by a recovery passphrase / key
  → Allows message history on new devices
```

### 6.3 Verification UI

```
┌──────────────────────────────────────┐
│  🔐 Verify Device                    │
│                                      │
│  Compare these emoji with            │
│  Alice's device:                     │
│                                      │
│  🐶  🎸  🚀  🌺  🎲  🔑  🌙       │
│  Dog Guitar Rocket Flower Dice Key Moon│
│                                      │
│  ┌──────────┐   ┌──────────────┐    │
│  │ They     │   │ They don't   │    │
│  │ match ✓  │   │ match ✗      │    │
│  └──────────┘   └──────────────┘    │
└──────────────────────────────────────┘
```

---

## 7. Rust Crate Architecture

```
src-tauri/
├── Cargo.toml              (workspace root)
├── src/
│   ├── lib.rs              (Tauri app — commands + event bridges)
│   └── main.rs
└── crates/
    ├── pjsip-sys/          (existing — PJSIP FFI bindings)
    ├── pale-core/          (existing — SIP engine, call mgmt)
    └── pale-matrix/        (NEW — Matrix SDK wrapper)
        ├── Cargo.toml
        └── src/
            ├── lib.rs
            ├── client.rs       — Matrix client lifecycle
            ├── chat.rs         — Room/message management
            ├── file_transfer.rs— Encrypted upload/download
            ├── encryption.rs   — E2E key mgmt, verification
            ├── events.rs       — PaleMatrixEvent enum
            └── types.rs        — Room, Message, FileInfo types
```

### pale-matrix Cargo.toml

```toml
[package]
name = "pale-matrix"
version = "0.1.0"
edition = "2021"

[dependencies]
matrix-sdk = { version = "0.10", features = [
    "e2e-encryption",
    "sqlite",        # Local state/crypto store
    "image-proc",    # Thumbnail generation
] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
log = "0.4"
thiserror = "2"
mime = "0.3"
mime_guess = "2"
```

---

## 8. Frontend Architecture

### 8.1 New Navigation (5 tabs)

```
┌──────────┬──────────┬──────────┬──────────┬──────────┐
│   ☎️      │   💬     │   📁     │   ⏱      │   ⚙️      │
│ Calls    │  Chat   │  Files  │ Recent  │ Settings │
└──────────┴──────────┴──────────┴──────────┴──────────┘
```

### 8.2 New Zustand Stores

```
src/store/
├── uiStore.ts          (existing — extended with new tabs)
├── accountStore.ts     (existing)
├── callStore.ts        (existing)
├── audioStore.ts       (existing)
├── chatStore.ts        (NEW — rooms, messages, typing indicators)
├── matrixStore.ts      (NEW — Matrix auth state, sync status)
└── fileStore.ts        (NEW — transfer progress, file index)
```

### 8.3 New Components

```
src/components/
├── chat/
│   ├── ChatView.tsx            — Main chat layout (conversation list + messages)
│   ├── ConversationList.tsx    — Sidebar list of rooms/DMs
│   ├── ConversationItem.tsx    — Single conversation row
│   ├── MessageList.tsx         — Scrollable message timeline
│   ├── MessageBubble.tsx       — Single message (text, file, image)
│   ├── MessageInput.tsx        — Compose bar with attach button
│   ├── TypingIndicator.tsx     — "Alice is typing..." dots
│   └── FilePreview.tsx         — Inline image/document preview
├── files/
│   ├── FilesView.tsx           — File browser across all rooms
│   ├── FileItem.tsx            — Single file row with actions
│   ├── TransferProgress.tsx    — Upload/download progress bar
│   └── FileDropZone.tsx        — Drag-and-drop file upload
├── video/
│   ├── VideoCallView.tsx       — Video call layout
│   ├── VideoRenderer.tsx       — Native video surface wrapper
│   ├── SelfView.tsx            — PiP self-view (draggable)
│   └── VideoControls.tsx       — Camera toggle, screen share
├── encryption/
│   ├── VerificationDialog.tsx  — Emoji/QR verification flow
│   ├── EncryptionBadge.tsx     — 🔐 lock icon on encrypted rooms
│   └── KeyBackupPrompt.tsx     — Prompt to set up key backup
└── auth/
    ├── MatrixLoginView.tsx     — Matrix homeserver login form
    └── MatrixSetupWizard.tsx   — First-run setup (SIP + Matrix)
```

---

## 9. Implementation Phases

### Phase 9: Video Calling (5-7 days)

1. Re-enable PJSIP video build (`--enable-video`, add libvpx/openh264 deps)
2. Add video codec negotiation (VP8 preferred, H.264 fallback)
3. Create native video renderer window (platform-specific)
4. Add camera selection and video device enumeration
5. Build `VideoCallView`, `VideoControls`, `SelfView` components
6. Add video toggle button to existing `CallControls`

### Phase 10: Matrix SDK Integration (7-10 days)

1. Create `pale-matrix` crate with matrix-rust-sdk
2. Implement Matrix login flow (username/password + SSO)
3. Set up E2E encryption (generate device keys, enable Megolm)
4. Implement /sync loop and event handling
5. Build `MatrixLoginView` and `MatrixSetupWizard`
6. Wire Tauri IPC commands for Matrix operations
7. Create `matrixStore` for auth and sync state

### Phase 11: Chat & Messaging (7-10 days)

1. Implement room listing and direct message creation
2. Build message sending/receiving with E2E encryption
3. Implement typing indicators and read receipts
4. Build all chat UI components (ConversationList, MessageList, MessageBubble, etc.)
5. Create `chatStore` with real-time sync
6. Add new "Chat" tab to navigation
7. Support message types: text, emote, reply, edit, delete

### Phase 12: File Transfer (5-7 days)

1. Implement encrypted file upload via Matrix content repository
2. Implement download + decrypt flow
3. Add drag-and-drop file upload zone
4. Build transfer progress tracking
5. Create file browser view across all rooms
6. Support image thumbnails and inline previews
7. Add file sharing to chat compose bar

### Phase 13: Encryption Polish (3-5 days)

1. Device verification flow (emoji comparison + QR code)
2. Cross-signing support
3. Key backup setup (recovery key/passphrase)
4. Encryption status badges on rooms and messages
5. Unverified device warnings

### Phase 14: Integration & Polish (5-7 days)

1. Unified contact list (SIP + Matrix contacts merged)
2. Click-to-call from chat (SIP call initiated from Matrix DM)
3. Call history includes both SIP and Matrix calls
4. Notification unification (calls + messages)
5. Setup wizard for first-run (configure both SIP + Matrix)
6. Performance optimization (lazy load messages, virtual scrolling)

---

## Appendix: Matrix Homeserver Options

| Server | Language | Best For |
|--------|----------|----------|
| **Synapse** | Python | Reference implementation, most features |
| **Conduit** | Rust | Lightweight, easy to self-host |
| **Dendrite** | Go | Better scaling than Synapse |
| **matrix.org** | — | Public homeserver for testing |

For development, use `matrix.org` or run Conduit locally via Docker:
```bash
docker run -d -p 6167:6167 -v conduit_data:/var/lib/conduit matrixconduit/matrix-conduit
```
