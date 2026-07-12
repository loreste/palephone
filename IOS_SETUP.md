# iOS packaging (preview path)

Pale’s mobile client is Tauri-based. **Android** is exercised in CI. **iOS**
packaging is supported by Tauri Mobile but is not yet a release artifact in
this repository. This document is the operator/developer path to produce a
local iOS build when Xcode and Apple developer credentials are available.

## Requirements

- macOS with Xcode 15+ and Command Line Tools  
- Apple Developer account (signing team)  
- Node.js 22, Rust stable  
- CocoaPods (`sudo gem install cocoapods` or Homebrew)  
- iOS Simulator or a registered device  

```bash
rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
npm ci
```

## Initialize iOS project

```bash
npm run tauri ios init
```

Generated project lives under `src-tauri/gen/apple` (Tauri 2 layout). Re-run
after major Tauri upgrades.

## Develop

```bash
npm run tauri ios dev
```

## Release build (local)

```bash
npm run tauri ios build
```

Sign with your team in Xcode:

1. Open the generated Xcode workspace  
2. Set **Signing & Capabilities** → Team  
3. Archive → Distribute (Ad Hoc / App Store / Enterprise)  

## Permissions to verify on device

Same as Android operational checks:

- Microphone and camera for calls/meetings  
- Notifications for incoming call and chat  
- Background modes / VoIP as allowed by Apple policy  
- Network (HTTPS API + SIP TLS)  

## CI

A workflow scaffold (`.github/workflows/ios.yml`) can run on `macos-14` when
secrets for signing are configured. Until then, treat iOS as **manual** and do
not claim App Store readiness.

## Gaps vs production Teams mobile

- Push via APNs for native call wake (Web Push alone is insufficient on iOS)  
- CallKit / PushKit integration for system call UI  
- Background SIP re-registration under iOS networking limits  

Track those before replacing a managed Teams mobile fleet.
