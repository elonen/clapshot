# Clapshot + htwicket (built-in login)

This compose recipes provides Clapshot with a small built-in login form and user manager, 
[htwicket](https://github.com/elonen/htwicket) — a more secure replacement for the old `htadmin` demo. Fine for small and simple deployments.

> If you have an external identity provider (Authentik, Okta, Keycloak,
> Kerberos, ...), use the [`custom-proxy`](../custom-proxy/) recipe instead, and see
> [Advanced Authentication](../../../doc/sysadmin-guide.md#advanced-authentication).
> Htwicket is the minimum, not the recommendation.

## Quick start (local)

```sh
cp .env.example .env        # the defaults serve at http://127.0.0.1:8080/
docker compose up -d
```

Open <http://127.0.0.1:8080/> — you'll be sent to a login page.

Fish the generated admin password from log (unless you set `CLAPSHOT_INITIAL_ADMIN_PASSWORD`).

Log in, then create your users in the admin UI at
`/htwicket/admin` — or by running `docker compose exec htwicket htwicket user add <name>`.

## Production / HTTPS / Portainer / Komodo

See the shared guide: [`../README.md`](../README.md).

In short: Deploy this directory from the git repo, copy the production block of `.env`, and Caddy handles Let's Encrypt.

## What's in here

| File | Edit? | Purpose |
|------|-------|---------|
| `.env` (from `.env.example`) | **yes** | configuration options |
| `compose.yml` | no | the stack (Caddy, clapshot-web, clapshot-server, htwicket) |
| `site.conf` | no | nginx configs: client SPA + `/api` proxy + `auth_request` (→ htwicket) |
| `htwicket.toml` | no | htwicket configs |
