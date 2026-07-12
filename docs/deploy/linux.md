# Pale Server on Linux (bare metal / packages)

## Installer

Supported families: Debian/Ubuntu (apt) and RHEL/Rocky/Alma/Fedora (dnf/yum).

```bash
curl -fsSL https://drcpbx.com/install-pale-server.sh | sudo bash
```

The installer:

1. Installs the `pale-server` package from the Pale repository
2. Writes `/etc/pale-server/pale-server.env` (mode 0600)
3. Enables the `pale-server` systemd unit
4. Prompts for admin password and SIP backend

**Choose `udp-parser`** for the built-in REGISTER/PBX registrar. The `pjsip`
backend does not provide a client registrar on no-native package builds and will
leave SIP disabled.

Default HTTP bind: `127.0.0.1:8080` (recommended). Put TLS in front.

## Production environment file

Edit `/etc/pale-server/pale-server.env` after install:

```bash
PALE_ADMIN_USERNAME=admin
PALE_ADMIN_PASSWORD=<from installer>
PALE_SERVER_TOKEN=<generated>
PALE_STORAGE_KEY=<generated>
PALE_DATA_DIR=/var/lib/pale-server
PALE_HTTP_ADDR=127.0.0.1:8080
PALE_SIP_BACKEND=udp-parser
PALE_SIP_TCP=true
PALE_SIP_UDP=false
PALE_SIP_SRTP=true
PALE_SIP_TLS=true
PALE_SIP_TLS_CERT=/etc/letsencrypt/live/pale.example.com/fullchain.pem
PALE_SIP_TLS_KEY=/etc/letsencrypt/live/pale.example.com/privkey.pem
PALE_SIP_TLS_PORT=5061
PALE_SIP_EXTERNAL_ADDR=pale.example.com:5060
PALE_SIP_TLS_EXTERNAL_ADDR=pale.example.com:5061
PALE_DATABASE_URL=host=127.0.0.1 user=pale password=... dbname=pale
PALE_TURN_SERVER=turn:pale.example.com:3478
PALE_TURN_PASSWORD=<same as coturn static auth secret>
PALE_TURN_REALM=pale.local
PALE_RETENTION_ENFORCEMENT_INTERVAL_SECS=86400
PALE_LOG_JSON=true
RUST_LOG=info
```

```bash
sudo systemctl restart pale-server
curl -sf http://127.0.0.1:8080/health
journalctl -u pale-server -f
```

## Reverse proxy (Caddy example)

```caddy
pale.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Clients use `https://pale.example.com` as the server URL.

## PostgreSQL

```bash
# Debian/Ubuntu example
sudo apt install postgresql
sudo -u postgres createuser pale
sudo -u postgres createdb -O pale pale
# set password and PALE_DATABASE_URL
```

Migrations apply automatically on pale-server start.

## coturn

Install coturn on the same host or a DMZ host with a public IP. Set
`--external-ip=<public-ipv4>` and open UDP 3478 + relay ports. Align
`static-auth-secret` with `PALE_TURN_PASSWORD` / `TURN_SECRET`.

## Backups

```bash
export PGHOST=127.0.0.1 PGUSER=pale PGPASSWORD=... PGDATABASE=pale
export BACKUP_DIR=/var/backups/pale
sudo ./scripts/backup.sh
```

Schedule daily with systemd timer; copy dumps off-host.

## Firewall (nftables/ufw sketch)

Allow: 443/tcp (HTTPS), 5061/tcp (SIP TLS), 3478/udp (TURN), relay UDP range.  
Deny public access to 5432, 8080 (if only local), 4222.

## Restore

```bash
gunzip -c pale_YYYYMMDD_HHMMSS.sql.gz | psql -h 127.0.0.1 -U pale pale
sudo systemctl restart pale-server
```

## See also

[PRODUCTION.md](PRODUCTION.md)
