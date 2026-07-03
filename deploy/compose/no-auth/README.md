# Clapshot — no auth

Clapshot with **no login**: every request is the `anonymous` user. nginx **strips** any
inbound `X-Remote-User-*` headers so identity can't be spoofed — which also means this
recipe does **not** work behind an authenticating proxy (the proxy's headers would be
stripped too). For that, use [`custom-proxy/`](../custom-proxy/).

**Local development / evaluation only.** Don't expose it publicly — anyone who can reach
it is `anonymous` with full anonymous access.

## Quick start (local)

```sh
cp .env.example .env        # the defaults serve http://127.0.0.1:8080/
docker compose up -d
```

Open <http://127.0.0.1:8080/>.

## Production / HTTPS / Portainer / Komodo

See the shared guide: [`../README.md`](../README.md).

## What's in here

| File | Edit? | Purpose |
|------|-------|---------|
| `.env` (from `.env.example`) | **yes** | the only file you edit |
| `compose.yml` | no | the stack (Caddy, clapshot-web, clapshot-server) |
| `site.conf` | no | nginx: SPA + `/api` proxy; **strips** `X-Remote-User-*` → server uses `anonymous` |
