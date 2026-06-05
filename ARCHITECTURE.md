# Pale — Cross-Platform SIP Softphone: Technical Architecture

> **Version:** 1.0 — June 2026
> **Audience:** Engineering team, technical stakeholders

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Recommended Tech Stack](#2-recommended-tech-stack)
3. [SIP & Media Stack](#3-sip--media-stack)
4. [Core Feature Architecture](#4-core-feature-architecture)
5. [OS-Specific Considerations](#5-os-specific-considerations)
6. [High-Level System Diagram](#6-high-level-system-diagram)
7. [Directory Structure](#7-directory-structure)
8. [Security Considerations](#8-security-considerations)
9. [Testing Strategy](#9-testing-strategy)

---

## 1. Executive Summary

**Pale** is a cross-platform desktop SIP softphone targeting Windows, macOS, and Linux. It provides enterprise-grade voice calling via SIP/RTP, with a modern UI, low-latency audio, and robust call management (hold, transfer, mute, multi-line). The architecture prioritizes real-time audio performance, cross-platform consistency, and maintainability.

---

## 2. Recommended Tech Stack

### 2.1 Framework Comparison

| Criterion                    | Electron + React        | Tauri + React/Svelte     | Go + Wails              |
|------------------------------|-------------------------|--------------------------|-------------------------|
| **Binary size**              | ~150–200 MB             | ~5–15 MB                 | ~10–20 MB               |
| **RAM usage (idle)**         | ~100–200 MB             | ~30–60 MB                | ~40–80 MB               |
| **Native API access**        | Via Node.js addons      | Via Rust backend          | Via Go backend           |
| **Audio/RTP performance**    | Requires native addons  | Excellent (Rust)          | Good (Go + CGo)          |
| **Ecosystem maturity**       | Mature, huge ecosystem  | Rapidly maturing          | Smaller community        |
| **Cross-platform parity**    | Excellent               | Very good                 | Good                     |
| **C/C++ FFI (for PJSIP)**   | node-gyp / NAPI         | Native Rust FFI           | CGo                      |
| **Real-time suitability**    | GC pauses in JS layer   | No GC in Rust backend     | GC pauses in Go runtime  |

### 2.2 Recommendation: Tauri 2.x + React (TypeScript) + Rust Backend

**Primary rationale:**

1. **Rust backend for real-time audio.** SIP signaling and RTP media handling run in a Rust process with no garbage collector — critical for jitter-sensitive audio pipelines. Rust's `unsafe` FFI to C libraries (PJSIP) is idiomatic and well-supported.

2. **Small footprint.** Tauri apps use the OS-native webview (WebKit on macOS, WebView2 on Windows, WebKitGTK on Linux), producing binaries 10–20x smaller than Electron with significantly lower RAM usage.

3. **Security model.** Tauri's IPC bridge uses a capability-based allowlist — the webview cannot access the filesystem, network, or OS APIs unless explicitly granted. This reduces the attack surface versus Electron's full Node.js access.

4. **React frontend.** React provides the widest hiring pool, richest component ecosystem, and strong TypeScript support. The entire UI layer is pure web tech — no platform-specific UI code.

**Trade-offs to accept:**
- Tauri's Linux support depends on WebKitGTK, which can lag behind Chromium in rendering features. Test UI thoroughly on Linux.
- Debugging Rust ↔ webview IPC is less ergonomic than Electron's Node.js ↔ renderer bridge. Invest in structured logging early.
- WebView2 must be bootstrapped on Windows (auto-installer provided by Tauri).

### 2.3 Alternative: Electron + React (Fallback)

If the team lacks Rust expertise, Electron remains viable. Use a native Node.js addon (N-API) wrapping PJSIP for the SIP/media layer, keeping the audio pipeline off the JS main thread. Accept the larger binary and RAM footprint. The architecture described below remains applicable — replace "Rust backend" with "native N-API addon."

---

## 3. SIP & Media Stack

### 3.1 Library Comparison

| Library        | Language   | SIP | RTP/Media | Codecs          | Maturity    | License   |
|----------------|------------|-----|-----------|-----------------|-------------|-----------|
| **PJSIP**      | C          | Yes | Yes       | G.711, Opus, G.722, GSM | Battle-tested | GPL-2.0   |
| **oSIP/eXosip**| C          | Yes | No        | N/A             | Stable      | LGPL-2.1  |
| **opal (opalvoip)** | C++   | Yes | Yes       | G.711, G.722    | Aging       | MPL       |
| **baresip**    | C          | Yes | Yes       | G.711, Opus     | Active      | BSD       |
| **Opal/oH323** | C++       | Yes | Yes       | Various         | Aging       | MPL       |
| **JsSIP/SIP.js** | JS      | Yes | WebRTC    | Opus (browser)  | Mature      | MIT       |
| **rsip (Rust)**| Rust       | Partial | No    | N/A             | Early       | MIT       |

### 3.2 Recommendation: PJSIP via Rust FFI

**PJSIP** (pjproject) is the industry standard for desktop/embedded SIP+media. It provides:

- **Complete SIP stack** (RFC 3261, RFC 3515 REFER, RFC 3891 Replaces, RFC 4028 session timers).
- **Full media engine** — RTP/RTCP, jitter buffer, echo cancellation (WebRTC AEC or Speex AEC), AGC, noise suppression, PLC.
- **Codec library** — G.711 (a-law/μ-law), G.722, GSM, Speex, iLBC, and **Opus** (via contrib module).
- **Transport flexibility** — UDP, TCP, TLS (OpenSSL/LibreSSL), and WebSocket transports.
- **NAT traversal** — ICE, STUN, TURN built-in.

**Integration approach:**

```
┌─────────────────────────────────────────────────┐
│  Tauri Frontend (React + TypeScript)            │
│  ┌───────────────────────────────────────────┐  │
│  │  UI Components: Dialpad, Call Controls,   │  │
│  │  Settings, Contact List                   │  │
│  └──────────────────┬────────────────────────┘  │
│                     │  Tauri IPC (invoke/events) │
├─────────────────────┼───────────────────────────┤
│  Tauri Rust Backend │                           │
│  ┌──────────────────┴────────────────────────┐  │
│  │  pale-core (Rust crate)                   │  │
│  │  ├── sip_manager.rs   — Account/reg       │  │
│  │  ├── call_manager.rs  — Session lifecycle  │  │
│  │  ├── audio_manager.rs — Device routing     │  │
│  │  ├── config.rs        — Persistent config  │  │
│  │  └── event_bus.rs     — Async event fan-out│  │
│  └──────────────────┬────────────────────────┘  │
│                     │  Rust FFI (unsafe C calls) │
│  ┌──────────────────┴────────────────────────┐  │
│  │  pjsip-sys (Rust -sys crate)             │  │
│  │  Auto-generated bindings via bindgen      │  │
│  │  Links to: libpjsua2, libpjsip,          │  │
│  │  libpjmedia, libpjnath, libpjlib-util    │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

**Why not pure JS (SIP.js/JsSIP)?**
- They rely on the browser's WebRTC stack for media. Desktop softphones need direct control over audio devices, codecs, and the RTP pipeline — things WebRTC abstracts away.
- No access to hardware echo cancellation, custom jitter buffer tuning, or non-WebRTC codecs like G.711 μ-law (the telephony standard).
- Cannot negotiate SIP over UDP/TCP directly; they require a WebSocket-to-SIP gateway (e.g., Obeysip, Obeyswan), adding infrastructure and latency.

### 3.3 PJSIP Build Strategy

Compile PJSIP as a static library per target triple:

| Target                  | Toolchain              | Notes                              |
|-------------------------|------------------------|------------------------------------|
| `x86_64-apple-darwin`   | Xcode clang            | Use CoreAudio backend              |
| `aarch64-apple-darwin`  | Xcode clang            | Apple Silicon, CoreAudio           |
| `x86_64-pc-windows-msvc`| MSVC 2022             | Use WASAPI/WMME audio backend      |
| `x86_64-unknown-linux-gnu` | gcc/clang           | Use ALSA or PulseAudio backend     |

Automate with a `build.rs` script in the `pjsip-sys` crate that:
1. Downloads/verifies a pinned PJSIP release tarball.
2. Runs `./configure` with target-appropriate flags.
3. Builds with `make dep && make`.
4. Emits `cargo:rustc-link-lib=static=pjsua2` (and friends) + include paths.

---

## 4. Core Feature Architecture

### 4.1 SIP Registration & Authentication

```
                          ┌──────────────┐
                          │  SIP Proxy / │
                          │  Registrar   │
                          └──────┬───────┘
                                 │
              SIP REGISTER (with Contact, Expires)
                                 │
              401 Unauthorized (with WWW-Authenticate nonce)
                                 │
              REGISTER (with Authorization: Digest ...)
                                 │
              200 OK (registered, Expires: 3600)
                                 │
                          ┌──────┴───────┐
                          │ pale-core    │
                          │ sip_manager  │
                          └──────────────┘
```

**Implementation details:**

```rust
// sip_manager.rs — Account configuration (pseudocode)
pub struct SipAccount {
    pub display_name: String,
    pub sip_uri: String,           // sip:user@domain.com
    pub registrar_uri: String,     // sip:domain.com
    pub auth_realm: String,        // "*" or specific realm
    pub auth_username: String,
    pub auth_password: String,     // stored encrypted via OS keychain
    pub transport: Transport,      // Udp | Tcp | Tls
    pub reg_expiry: u32,           // seconds, default 3600
    pub reg_retry_interval: u32,   // seconds on failure, default 30
}

pub enum Transport {
    Udp,
    Tcp,
    Tls { ca_cert: Option<PathBuf>, verify_server: bool },
}
```

**Key behaviors:**
- **Digest authentication** (RFC 2617/7616): PJSIP handles the 401/407 challenge-response natively. Credentials are supplied via `pjsua_acc_config`.
- **Transport selection**: Create one `pjsip_transport` per type. TLS transport requires configuring `pjsip_tls_setting` with CA bundle path and optional client cert.
- **Registration refresh**: PJSIP auto-refreshes before expiry. On network change (detected via OS network event), trigger immediate re-REGISTER.
- **Credential storage**: Use the OS keychain (macOS Keychain, Windows Credential Manager, libsecret on Linux) — never store plaintext passwords on disk.
- **Multi-account**: Support N simultaneous registrations for users with multiple SIP accounts (work + personal, etc.).

### 4.2 Session Management (Call Handling)

```rust
// call_manager.rs — Call state machine
pub enum CallState {
    Idle,
    Dialing,          // INVITE sent, waiting 1xx/2xx
    Ringing,          // 180 Ringing received (outbound) or INVITE received (inbound)
    EarlyMedia,       // 183 Session Progress with SDP (ringback tone from remote)
    Connected,        // 200 OK + ACK exchanged, media flowing
    OnHold(HoldType), // re-INVITE with sendonly/inactive SDP
    Transferring,     // REFER in progress
    Terminated,       // BYE sent/received or error
}

pub enum HoldType {
    Local,   // we put them on hold (sendonly)
    Remote,  // they put us on hold (recvonly)
    Both,    // both sides on hold (inactive)
}
```

**Inbound call flow:**
1. PJSIP fires `on_incoming_call` callback → Rust `call_manager` creates `CallSession`.
2. Emit `CallEvent::Incoming { caller_id, call_id }` to frontend via Tauri event.
3. Frontend shows incoming call UI with Accept/Reject.
4. User accepts → `call_manager.answer(call_id, 200)` → PJSIP sends 200 OK with SDP.
5. Media starts flowing. State → `Connected`.

**Outbound call flow:**
1. Frontend invokes `tauri::invoke("make_call", { uri: "sip:1234@domain.com" })`.
2. `call_manager.make_call(uri)` → PJSIP sends INVITE.
3. On 180 → state `Ringing`, emit event. On 200 → `Connected`.

**Hold/Resume:**
```
Hold:   re-INVITE with a=sendonly → remote sees a=recvonly → music on hold (optional)
Resume: re-INVITE with a=sendrecv → full duplex restored
```

**Attended Transfer (RFC 3515 + RFC 3891):**
```
1. Agent is on call with Party A.
2. Agent makes consultation call to Party B (new INVITE).
3. Agent triggers transfer → sends REFER to Party A with Refer-To: Party B
   (including Replaces header for the consultation dialog).
4. Party A sends INVITE to Party B with Replaces header.
5. Agent's calls are terminated. Party A and B are now connected.
```

**Blind Transfer:**
```
1. Agent sends REFER to remote party with Refer-To: <target-uri>.
2. Remote party sends INVITE to target.
3. Original call terminated via NOTIFY/BYE.
```

**Mute:**
- Local operation only — stop sending RTP packets (or send comfort noise/silence frames).
- No SIP re-signaling required.
- Implementation: `pjsua_call_set_flag(call_id, PJSUA_CALL_FLAG_MUTE_TX)` or disconnect the mic port from the conference bridge.

### 4.3 Audio Pipeline

```
┌───────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌─────────┐
│ Microphone│───▶│  AEC     │───▶│  NS/AGC  │───▶│ Encoder  │───▶│  RTP    │
│ (capture) │    │(echo     │    │(noise    │    │(G.711/   │    │  Send   │
│           │    │ cancel)  │    │ suppress)│    │ Opus)    │    │         │
└───────────┘    └──────────┘    └──────────┘    └──────────┘    └─────────┘

┌─────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌───────────┐
│  RTP    │───▶│ Jitter   │───▶│ Decoder  │───▶│  PLC     │───▶│ Speaker   │
│  Recv   │    │ Buffer   │    │(G.711/   │    │(packet   │    │ (playback)│
│         │    │          │    │ Opus)    │    │ loss     │    │           │
│         │    │          │    │          │    │ conceal) │    │           │
└─────────┘    └──────────┘    └──────────┘    └──────────┘    └───────────┘
```

#### 4.3.1 Audio Device Selection

```rust
// audio_manager.rs
pub struct AudioDevice {
    pub id: String,          // OS-specific device ID
    pub name: String,        // Human-readable name
    pub direction: Direction, // Input | Output | Both
    pub sample_rates: Vec<u32>,
    pub is_default: bool,
}

pub struct AudioConfig {
    pub input_device: Option<String>,   // None = system default
    pub output_device: Option<String>,  // None = system default
    pub sample_rate: u32,               // 16000 for wideband, 48000 for Opus
    pub frame_size_ms: u32,             // 20ms typical
    pub echo_cancel: EchoCancelConfig,
    pub noise_suppress: bool,
    pub auto_gain_control: bool,
}
```

- Enumerate devices via PJSIP's `pjmedia_aud_dev_enum()`.
- Allow hot-switching mid-call by reconnecting ports on the PJSIP conference bridge.
- Monitor device plug/unplug events (OS-specific: CoreAudio notifications on macOS, `IMMNotificationClient` on Windows, PulseAudio events on Linux) and fall back to default device if the active device is removed.

#### 4.3.2 Echo Cancellation (AEC)

PJSIP provides two AEC implementations:
- **WebRTC AEC3** (recommended) — `--with-webrtc-aec3` at build time. Handles non-linear echo, double-talk, and adapts to changing acoustic conditions. Best quality.
- **Speex AEC** — Fallback. Lighter weight but less effective with non-linear echo.

Configure tail length based on expected acoustic environment:
- 128 ms (desktop headset) — low latency, sufficient for close-coupled mic/speaker.
- 256 ms (laptop speakers) — accounts for reflections from desk surface.
- 512 ms (speakerphone mode) — maximum for room reverb.

#### 4.3.3 Jitter Buffer

PJSIP's adaptive jitter buffer dynamically adjusts buffer depth based on network conditions:

```
Configured via pjmedia_jb_setting:
  - min_prefetch:     20 ms   (low-latency start)
  - max_prefetch:     200 ms  (upper bound for bad networks)
  - max_burst:        100     (max consecutive empty frames before reset)
  - discard_algo:     PJMEDIA_JB_DISCARD_PROGRESSIVE
```

#### 4.3.4 Codec Negotiation

Priority order in SDP offer (configurable per-account):

| Priority | Codec    | Payload | Rate   | Bitrate     | Use Case                |
|----------|----------|---------|--------|-------------|-------------------------|
| 1        | **Opus** | 111     | 48 kHz | 16–64 kbps  | Best quality, adaptive  |
| 2        | G.722    | 9       | 16 kHz | 64 kbps     | Wideband PSTN compat    |
| 3        | PCMU     | 0       | 8 kHz  | 64 kbps     | Universal PSTN fallback |
| 4        | PCMA     | 8       | 8 kHz  | 64 kbps     | EU PSTN standard        |
| 5        | telephone-event | 101 | 8 kHz | —         | DTMF (RFC 4733)         |

**DTMF handling:**
- **RFC 4733** (in-band RTP events) — preferred, negotiate `telephone-event/8000` in SDP.
- **SIP INFO** — fallback for legacy systems that don't support RFC 4733.
- **In-band audio tones** — last resort, unreliable with compressed codecs.

---

## 5. OS-Specific Considerations

### 5.1 macOS

| Concern | Details |
|---------|---------|
| **Microphone permission** | macOS requires `NSMicrophoneUsageDescription` in `Info.plist`. Tauri sets this via `tauri.conf.json` → `bundle.macOS.info_plist`. First access triggers a system permission dialog. If denied, detect via `AVCaptureDevice.authorizationStatus` and show in-app guidance. |
| **Audio backend** | PJSIP uses **CoreAudio** (`PJMEDIA_AUDIO_DEV_HAS_COREAUDIO`). Supports low-latency audio units. |
| **App Nap** | macOS may throttle background apps. Disable App Nap when on an active call using `NSProcessInfo.beginActivity(options: .userInitiated)`. Without this, audio glitches occur when the app is not focused. |
| **Code signing** | Required for distribution and notarization. Sign with a Developer ID certificate. Tauri's bundler supports `codesign` and `notarytool` integration. Unsigned apps trigger Gatekeeper warnings. |
| **Notarization** | Apple requires notarization for all distributed apps. Tauri's `beforeBundleCommand` can invoke `xcrun notarytool submit`. Hardened Runtime must be enabled with the `com.apple.security.device.audio-input` entitlement. |
| **Universal Binary** | Build for both `x86_64-apple-darwin` and `aarch64-apple-darwin`, then `lipo` merge into a universal binary for Intel + Apple Silicon support. |

### 5.2 Windows

| Concern | Details |
|---------|---------|
| **Microphone permission** | Windows 10/11 has a privacy setting: Settings → Privacy → Microphone. Apps must be allowed. No code-level permission request — the OS handles it. Check access status via `Windows.Media.Capture.MediaCapture`. |
| **Audio backend** | PJSIP supports **WASAPI** (preferred, low latency) and **WMME** (legacy fallback). Configure with `--with-wasapi` at build time. WASAPI exclusive mode provides the lowest latency but locks the device. Use shared mode for softphone use. |
| **WebView2 Runtime** | Tauri on Windows requires the WebView2 runtime (Chromium-based). Tauri's NSIS/WiX installer bundles an auto-bootstrapper. On enterprise machines, IT may pre-deploy WebView2 via SCCM/GPO. |
| **Code signing** | Use an EV code signing certificate (hardware token) to avoid SmartScreen warnings. Standard OV certs work but require reputation building. Tauri's bundler supports `signtool.exe` integration. |
| **Firewall** | Windows Firewall may prompt on first SIP/RTP traffic. Include a firewall rule in the installer (NSIS `nsFirewall` plugin) for UDP/TCP on the SIP port (5060/5061) and RTP port range (10000–20000). |
| **WASAPI audio routing** | Handle default device changes via `IMMNotificationClient::OnDefaultDeviceChanged`. Reconnect PJSIP audio ports when users switch Bluetooth headsets, etc. |

### 5.3 Linux

| Concern | Details |
|---------|---------|
| **Microphone permission** | No universal permission system. PipeWire/PulseAudio grant access by default. Flatpak/Snap packages require portal permissions (`org.freedesktop.portal.Device`). |
| **Audio backend** | Prefer **PipeWire** (modern distros: Ubuntu 22.04+, Fedora 34+) → **PulseAudio** → **ALSA** (direct, lowest latency but no mixing). PJSIP's ALSA backend works with PipeWire/PulseAudio via their ALSA compatibility layers, but native PulseAudio integration (`--with-pa`) is more reliable for device hot-plug. |
| **WebKitGTK** | Tauri requires `libwebkit2gtk-4.1` and `libgtk-3`. Package as a `.deb` (Debian/Ubuntu) and `.rpm` (Fedora/RHEL) with these as dependencies. AppImage bundles can also work but are larger. |
| **Sandboxing** | If distributing via Flatpak, declare permissions in the manifest: `--socket=pulseaudio`, `--share=network`, `--device=all` (for audio devices). |
| **Tray icon** | Linux tray icon support varies. `libappindicator3` (Ubuntu/GNOME) vs. `StatusNotifierItem` (KDE). Tauri provides a `SystemTray` API that abstracts this. |
| **Distribution fragmentation** | Test on at least: Ubuntu 22.04 LTS, Fedora 40, and Arch Linux (rolling). CI should build and test across these. |

### 5.4 Cross-Platform Summary Matrix

| Feature              | macOS              | Windows            | Linux               |
|----------------------|--------------------|--------------------|----------------------|
| Audio API            | CoreAudio          | WASAPI (shared)    | PipeWire/PulseAudio  |
| Mic permission       | Info.plist + dialog| System Settings    | Flatpak portal / none|
| Code signing         | Developer ID + notarize | EV cert + signtool | GPG (optional)  |
| Installer format     | .dmg / .app        | .msi / .exe (NSIS) | .deb / .rpm / AppImage |
| Background audio     | Disable App Nap    | No special handling| No special handling  |
| Webview              | WebKit (system)    | WebView2           | WebKitGTK            |

---

## 6. High-Level System Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Pale Desktop App                             │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                    UI Layer (Webview)                          │ │
│  │                                                                │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────────┐  │ │
│  │  │ Dialpad  │ │ Call     │ │ Contacts │ │ Settings /      │  │ │
│  │  │          │ │ Controls │ │ / History│ │ Account Config  │  │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └─────────────────┘  │ │
│  │                                                                │ │
│  │  ┌────────────────────────────────────────────────────────┐   │ │
│  │  │  State Management (Zustand / Redux Toolkit)            │   │ │
│  │  │  callSlice · accountSlice · audioSlice · uiSlice       │   │ │
│  │  └──────────────────────┬─────────────────────────────────┘   │ │
│  └─────────────────────────┼─────────────────────────────────────┘ │
│                            │ Tauri IPC                              │
│                            │ (invoke commands + listen events)      │
│  ┌─────────────────────────┼─────────────────────────────────────┐ │
│  │                  Rust Backend (pale-core)                     │ │
│  │                         │                                     │ │
│  │  ┌─────────────┐ ┌─────┴───────┐ ┌──────────────┐           │ │
│  │  │ SipManager  │ │ CallManager │ │ AudioManager │           │ │
│  │  │ - register  │ │ - make_call │ │ - devices    │           │ │
│  │  │ - unregister│ │ - answer    │ │ - set_input  │           │ │
│  │  │ - on_reg_   │ │ - hangup    │ │ - set_output │           │ │
│  │  │   state     │ │ - hold      │ │ - volume     │           │ │
│  │  │             │ │ - transfer  │ │ - mute       │           │ │
│  │  └──────┬──────┘ └──────┬──────┘ └──────┬───────┘           │ │
│  │         │               │               │                    │ │
│  │  ┌──────┴───────────────┴───────────────┴──────────┐        │ │
│  │  │              EventBus (tokio broadcast)          │        │ │
│  │  │  Async event fan-out to frontend + internal      │        │ │
│  │  └──────────────────────┬──────────────────────────┘        │ │
│  │                         │                                    │ │
│  │  ┌──────────────────────┴──────────────────────────┐        │ │
│  │  │         pjsip-sys (bindgen FFI crate)           │        │ │
│  │  │  Static link to: libpjsua2 · libpjsip ·        │        │ │
│  │  │  libpjmedia · libpjnath · libpjlib-util         │        │ │
│  │  └─────────────────────────────────────────────────┘        │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
└────────────────────────────┼──────────────────────────────────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
         SIP/TLS         RTP/SRTP      STUN/TURN
         (5061)        (10000-20000)    (3478)
              │              │              │
              ▼              ▼              ▼
       ┌─────────────────────────────────────────┐
       │        SIP Server / PBX                 │
       │   (Obeyswan, FreePBX, Kamailio, etc.)   │
       └─────────────────────────────────────────┘
```

---

## 7. Directory Structure

```
pale/
├── src-tauri/                      # Rust backend (Tauri app)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs                    # PJSIP compilation orchestration
│   ├── src/
│   │   ├── main.rs                 # Tauri app entry point
│   │   ├── commands/               # Tauri IPC command handlers
│   │   │   ├── mod.rs
│   │   │   ├── sip.rs              # register, unregister, get_reg_status
│   │   │   ├── call.rs             # make_call, answer, hangup, hold, transfer
│   │   │   ├── audio.rs            # list_devices, set_input, set_output, mute
│   │   │   └── config.rs           # get/set settings
│   │   ├── core/                   # Business logic
│   │   │   ├── mod.rs
│   │   │   ├── sip_manager.rs
│   │   │   ├── call_manager.rs
│   │   │   ├── audio_manager.rs
│   │   │   ├── event_bus.rs
│   │   │   └── credential_store.rs # OS keychain integration
│   │   └── pjsip/                  # PJSIP FFI wrapper (safe Rust API)
│   │       ├── mod.rs
│   │       ├── account.rs
│   │       ├── call.rs
│   │       ├── media.rs
│   │       └── transport.rs
│   └── pjsip-sys/                  # -sys crate: raw bindgen bindings
│       ├── Cargo.toml
│       ├── build.rs                # Download, configure, compile PJSIP
│       ├── wrapper.h               # #include headers for bindgen
│       └── src/
│           └── lib.rs              # bindgen-generated bindings
├── src/                            # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── Dialpad.tsx
│   │   ├── CallControls.tsx
│   │   ├── IncomingCallModal.tsx
│   │   ├── ActiveCall.tsx
│   │   ├── ContactList.tsx
│   │   ├── AudioSettings.tsx
│   │   └── AccountSettings.tsx
│   ├── store/                      # Zustand state management
│   │   ├── callStore.ts
│   │   ├── accountStore.ts
│   │   ├── audioStore.ts
│   │   └── uiStore.ts
│   ├── hooks/
│   │   ├── useSipEvents.ts         # Tauri event listeners
│   │   ├── useCall.ts
│   │   └── useAudioDevices.ts
│   ├── lib/
│   │   └── tauri.ts                # Typed invoke/event wrappers
│   └── types/
│       └── index.ts                # Shared TypeScript types
├── package.json
├── tsconfig.json
├── vite.config.ts
├── ARCHITECTURE.md                 # This document
└── .github/
    └── workflows/
        ├── ci.yml                  # Lint, test, build all platforms
        └── release.yml             # Build + sign + notarize + publish
```

---

## 8. Security Considerations

| Area | Approach |
|------|----------|
| **SIP credentials** | Store in OS keychain (macOS Keychain Services, Windows DPAPI/Credential Manager, Linux libsecret). Never write to plaintext config files. |
| **Obeyed signaling** | Default to TLS (port 5061). Validate server certificates. Support mutual TLS for enterprise deployments. |
| **Media encryption** | Enable SRTP (`PJMEDIA_HAS_SRTP=1`). Negotiate via SDP `a=crypto` (SDES) or DTLS-SRTP. Reject unencrypted RTP when policy requires it. |
| **IPC hardening** | Tauri's capability system: only expose the minimum set of IPC commands. No filesystem or shell access from the webview. |
| **Dependency supply chain** | Pin PJSIP to a specific release tag. Verify tarball checksums. Use `cargo-audit` and `npm audit` in CI. |
| **Memory safety** | Rust prevents buffer overflows in application code. PJSIP (C) is the main unsafe boundary — wrap all FFI calls with Rust safety invariants and validate all pointers. |

---

## 9. Testing Strategy

### 9.1 Unit Tests

- **Rust (pale-core):** Test state machines (CallState transitions), codec priority logic, configuration parsing. Mock PJSIP FFI at the `pjsip-sys` boundary.
- **React (frontend):** Test components with React Testing Library. Test stores with direct Zustand invocations.

### 9.2 Integration Tests

- **SIP registration:** Stand up a local SIP server (Obeyswan, Obeyswan, or Kamailio in Docker) and verify registration, re-registration, and auth failure handling.
- **Call flows:** Automate inbound/outbound calls between two PJSIP instances (softphone ↔ `pjsua` CLI). Verify hold/resume, transfer, DTMF.
- **Audio pipeline:** Use virtual audio devices (PulseAudio null sink on Linux, BlackHole on macOS, Virtual Audio Cable on Windows) to verify audio routing without physical hardware.

### 9.3 Cross-Platform CI

```yaml
# .github/workflows/ci.yml (simplified)
strategy:
  matrix:
    os: [ubuntu-22.04, macos-14, windows-2022]
steps:
  - uses: actions/checkout@v4
  - name: Install system deps (Linux)
    if: runner.os == 'Linux'
    run: sudo apt-get install -y libwebkit2gtk-4.1-dev libasound2-dev libpulse-dev libssl-dev
  - name: Build PJSIP + Rust backend
    run: cargo build --release
  - name: Run Rust tests
    run: cargo test
  - name: Build frontend
    run: npm ci && npm run build
  - name: Bundle Tauri app
    run: npx tauri build
```

### 9.4 Performance Benchmarks

- **Audio latency:** Measure mouth-to-ear latency (target: < 150 ms one-way). Use RTP timestamps + NTP sync.
- **CPU usage:** Profile during an active Opus call. Target: < 5% CPU on modern hardware.
- **Memory:** Track RSS over a 1-hour call session. Watch for leaks in the FFI boundary.

---

## Appendix A: Key RFCs

| RFC | Title | Relevance |
|-----|-------|-----------|
| 3261 | SIP: Session Initiation Protocol | Core SIP signaling |
| 3264 | Offer/Answer Model with SDP | Call setup negotiation |
| 3515 | SIP REFER Method | Call transfer |
| 3891 | SIP Replaces Header | Attended transfer |
| 4028 | Session Timers in SIP | Keep-alive for long calls |
| 4733 | RTP Payload for DTMF | In-band DTMF events |
| 3711 | SRTP | Media encryption |
| 5245 | ICE | NAT traversal |
| 5389 | STUN | NAT discovery |
| 5766 | TURN | Relay for symmetric NAT |
| 7616 | HTTP Digest Auth (updated) | SIP authentication |

## Appendix B: Recommended PJSIP Build Flags

```bash
./configure \
  --disable-video \
  --enable-shared=no \
  --with-ssl=/usr/local/opt/openssl \
  --with-opus=/usr/local/opt/opus \
  --with-webrtc-aec3 \
  CFLAGS="-O2 -fPIC -DPJMEDIA_HAS_SRTP=1 \
          -DPJMEDIA_HAS_WEBRTC_AEC3=1 \
          -DPJ_HAS_IPV6=1"
```

Key flags:
- `--disable-video`: Softphone is audio-only; reduces binary size and attack surface.
- `--enable-shared=no`: Static linking for self-contained binary.
- `--with-webrtc-aec3`: Best echo cancellation quality.
- `-DPJ_HAS_IPV6=1`: IPv6 support for modern deployments.
- `-DPJMEDIA_HAS_SRTP=1`: Enable SRTP media encryption.
