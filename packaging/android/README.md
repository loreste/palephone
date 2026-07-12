# Android packaging

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
