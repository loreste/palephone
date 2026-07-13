# Android fleet notes (Phase 0.7 / M3.2)

## Current state

| Capability | Status |
|------------|--------|
| Signed sideload APK | Yes — https://drcpbx.com/downloads/Pale.apk |
| Foreground SIP + video path | Emulator-validated (API 34); physical E2E open |
| `SipForegroundService` | Present in tree |
| FCM push for calls | **Not shipped** — tracked Phase 3.1 |
| OEM battery / background ring | **Open** — document per OEM |

## Operator checklist

1. Uninstall older Pale builds (cert mismatch).  
2. Install signed `Pale.apk`.  
3. Grant **Microphone**, **Camera**, **Notifications**, **Phone** (as prompted).  
4. Disable battery optimization for Pale on aggressive OEMs (Xiaomi, Honor, Samsung).  
5. Confirm SIP register + audio call; then video call.  
6. Leave GitHub issue #1 open until human confirmation.

## Push direction (Phase 3.1)

- Server: FCM HTTP v1 adapter (env `PALE_FCM_*`)  
- Client: Firebase messaging + high-priority data messages for incoming INVITE  

Until FCM lands, treat Android as **foreground / user-opened** reliability, not
Teams-class always-on ringing.

Related: [ANDROID_SETUP.md](../../ANDROID_SETUP.md), [packaging/android/README.md](../../packaging/android/README.md).
