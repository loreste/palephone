# Pale on Kubernetes

Minimal manifests for a **single-node production lab** (Phase 0). Pale Server's
SIP registrar uses process-local state — run **one** pale-server replica for
calling until an external SIP edge (OpenSIPS/Kamailio) is in front.

## Layout

| File | Purpose |
|------|---------|
| `namespace.yaml` | `pale` namespace |
| `secrets.example.yaml` | Copy to `secrets.yaml` with real values (gitignored pattern) |
| `configmap.yaml` | Non-secret server settings (`udp-parser`, SRTP, retention) |
| `postgres.yaml` | Lab Postgres + PVC (prefer managed PG in real prod) |
| `pale-server.yaml` | Deployment (1 replica), ClusterIP + SIP LoadBalancer |
| `ingress.yaml` | HTTPS API edge |
| `networkpolicy.yaml` | PG isolation + server ingress ports |

Coturn is **not** included here — TURN usually needs host networking or a
dedicated public node. Run coturn on the host or a VM with `TURN_EXTERNAL_IP`
and point `PALE_TURN_SERVER` at it. LiveKit is optional (Phase 1 meetings).

## Quick apply

```bash
# 1. Build/push the image your cluster can pull
docker build -f Dockerfile.pale-server -t your-registry/pale-server:0.2.0 .
docker push your-registry/pale-server:0.2.0

# 2. Secrets
cp deploy/k8s/secrets.example.yaml /tmp/pale-secrets.yaml
# edit /tmp/pale-secrets.yaml
kubectl apply -f deploy/k8s/namespace.yaml
kubectl -n pale apply -f /tmp/pale-secrets.yaml

# 3. TLS secret for SIP (and optionally Ingress)
kubectl -n pale create secret tls pale-tls \
  --cert=fullchain.pem --key=privkey.pem

# 4. Edit configmap with public SIP/TURN hostnames, then:
kubectl apply -f deploy/k8s/configmap.yaml
kubectl apply -f deploy/k8s/postgres.yaml
# set image in pale-server.yaml, then:
kubectl apply -f deploy/k8s/pale-server.yaml
kubectl apply -f deploy/k8s/ingress.yaml
kubectl apply -f deploy/k8s/networkpolicy.yaml
```

## Constraints

- **Replicas = 1** for pale-server while it owns the registrar.
- Do not expose Postgres outside the cluster.
- Scrape `/metrics` only from a private Prometheus (NetworkPolicy / separate port).
- Set `PALE_SIP_EXTERNAL_ADDR` and `PALE_SIP_TLS_EXTERNAL_ADDR` to the
  LoadBalancer or public DNS that clients dial.

See [docs/deploy/PRODUCTION.md](../../docs/deploy/PRODUCTION.md).
