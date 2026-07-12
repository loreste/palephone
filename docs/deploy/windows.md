# Pale Server on Windows Server

## Installer

Download `PaleServerSetup-<version>-x64.exe` from project releases or the
`pale-server-windows-installer` GitHub Actions artifact.

The installer:

- Installs under Program Files
- Writes `C:\ProgramData\Pale Server\pale-server.env`
- Creates the `PaleServer` Windows service (WinSW)
- Generates local secrets
- Defaults HTTP to `127.0.0.1:8080`

Local secrets stay on the machine; do not commit `pale-server.env`.

## Configure for production SIP

Re-run configuration as Administrator:

```powershell
cd "C:\Program Files\Pale Server"   # path may vary with installer version
.\Configure-PaleServer.ps1 `
  -HttpAddr "127.0.0.1:8080" `
  -SipBackend "udp-parser" `
  -DatabaseUrl "host=... user=pale password=... dbname=pale" `
  -AdminPassword "<strong-password-24-chars-min>"
```

`udp-parser` enables the built-in REGISTER/PBX path. Leaving SIP `off` keeps
HTTP/admin only (safe default until you are ready for calling).

## TLS reverse proxy

Do not expose plain HTTP on a public interface.

Options:

1. **Caddy for Windows** reverse_proxy to `127.0.0.1:8080` with automatic HTTPS
2. **IIS** Application Request Routing / reverse proxy with a public certificate

Clients should use `https://pale.example.com`.

For **SIP TLS**, mount or path PEM certs in the environment file (if using
parser TLS) or terminate SIP on an SBC in front of Pale:

```
PALE_SIP_TLS=true
PALE_SIP_TLS_CERT=C:\ProgramData\Pale Server\certs\fullchain.pem
PALE_SIP_TLS_KEY=C:\ProgramData\Pale Server\certs\privkey.pem
PALE_SIP_EXTERNAL_ADDR=pale.example.com:5060
PALE_SIP_TLS_EXTERNAL_ADDR=pale.example.com:5061
PALE_SIP_SRTP=true
```

## Postgres and TURN

- Use a dedicated PostgreSQL instance (Windows or Linux). SQLite fallback is
  only for tiny single-node labs.
- Run coturn on Linux or a network appliance with a public IP; set
  `PALE_TURN_SERVER` and shared secret to match.

## Firewall

If you change `HttpAddr` off loopback, the configure script may open a Windows
Firewall rule for that TCP port. Prefer loopback + reverse proxy. Open SIP TLS
and TURN ports only as needed.

## Health and logs

```powershell
Invoke-WebRequest http://127.0.0.1:8080/health
# Logs under C:\ProgramData\Pale Server\logs
Get-Service PaleServer
```

## Backup

Use `pg_dump` against Postgres on a schedule. Also back up
`C:\ProgramData\Pale Server\data` if local file storage is used.

## See also

- [packaging/windows/server/README.md](../../packaging/windows/server/README.md)
- [PRODUCTION.md](PRODUCTION.md)
