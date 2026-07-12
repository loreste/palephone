# Pale Server Windows Installer

This directory contains the Windows packaging for Pale Server. The installer is
intended for operators who want a normal Windows setup flow instead of manually
placing binaries, writing environment files, and creating a service.

The installer is branded as **Pale Server** and uses the Pale icon from
`src-tauri/icons/icon.ico`. It:

- installs `pale-server.exe` under `Program Files`;
- writes local configuration to `C:\ProgramData\Pale Server\pale-server.env`;
- generates machine-local server and storage secrets;
- creates a real `PaleServer` Windows service through WinSW;
- adds Start Menu shortcuts for configure, start, stop, restart, health check,
  and uninstall;
- locks the generated config directory down to Administrators and SYSTEM;
- keeps local secrets out of the repository and out of public release metadata.

Default behavior is intentionally conservative: the HTTP API binds to
`127.0.0.1:8080`, and no firewall rule is opened unless the admin chooses a
non-loopback bind address. `Configure-PaleServer.ps1` defaults to the
`udp-parser` SIP backend (built-in REGISTER/PBX). Pass `-SipBackend off` to
disable the registrar. For production TLS, TURN, and Postgres, see
[docs/deploy/windows.md](../../../docs/deploy/windows.md) and
[docs/deploy/PRODUCTION.md](../../../docs/deploy/PRODUCTION.md).

## Build Locally on Windows

```powershell
cd src-tauri
cargo build --release -p pale-server --bin pale-server --no-default-features
cd ..
New-Item -ItemType Directory -Force dist\windows-server
Copy-Item src-tauri\target\release\pale-server.exe dist\windows-server\pale-server.exe
Invoke-WebRequest `
  -Uri https://github.com/winsw/winsw/releases/download/v2.12.0/WinSW-x64.exe `
  -OutFile dist\windows-server\PaleServerService.exe
iscc /DMyAppVersion=0.1.1 packaging\windows\server\PaleServer.iss
```

The output is written to `dist\windows-server\PaleServerSetup-0.1.1-x64.exe`.
