# Pale deployment assets

| Path | Purpose |
|------|---------|
| [caddy/Caddyfile](caddy/Caddyfile) | Optional reverse proxy for `docker-compose.prod.yml --profile proxy` |
| [livekit/livekit.yaml](livekit/livekit.yaml) | LiveKit SFU for `--profile meetings` |
| [k8s/](k8s/) | Kubernetes lab manifests (single-replica registrar) |

Operator runbook: [docs/deploy/PRODUCTION.md](../docs/deploy/PRODUCTION.md)
