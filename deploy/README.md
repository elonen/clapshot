# Deploying Clapshot

Ways to run Clapshot in production:

| Path | For | Where |
|------|-----|-------|
| **Docker Compose** | Portainer, Komodo, plain `docker compose` | [`compose/`](compose/) |
| **Debian package + systemd** | A VM/host without containers | [`debian/`](debian/) |

## How Clapshot thinks about authentication

Clapshot has **no login system of its own**. The API server trusts a few HTTP headers
set by whatever sits in front of it:

- `X-Remote-User-Id` — stable user id
- `X-Remote-User-Name` — display name
- `X-Remote-User-Is-Admin` — `true` / `false`
- `X-Remote-User-Can-Upload` — `true` / `false`

**Anything** that can authenticate a request and set those headers works: a login
gateway, company SSO, an identity-aware proxy, Kerberos, …

The [Docker Compose recipes](compose/) ship three ready options:

- **HTWicket** — a small built-in login form + user manager. Fine for simple deployments.
- **no-auth** — no login; every request is the `anonymous` user. nginx **strips** any
  inbound `X-Remote-User-*` so they can't be spoofed. Dev / demo only.
- **custom-proxy** — Clapshot behind **your own** authenticating reverse proxy: nginx
  trusts the headers your proxy sets. The path for real SSO identity providers.

> **HTWicket is the _floor_, not a recommendation.** This is a modernized replacement for the old
> *htadmin* demo. Consider a proper identity provider — Authentik, Okta, Keycloak (via
> oauth2-proxy / Vouch), or Kerberos/AD — in front, with the **custom-proxy** recipe.
> Clapshot only ever sees the `X-Remote-User-*` headers, so swapping the auth layer
> never touches the app. See
> [Advanced Authentication](../doc/sysadmin-guide.md#advanced-authentication).

## TLS / HTTPS

TLS is terminated **in front** of the app — the same idea as auth, and not the app's
job. The Compose recipes include [Caddy](https://caddyserver.com/), which obtains and
renews **Let's Encrypt** certificates automatically; you can also point it at your own
certificates, or turn it off and use your own reverse proxy.

See [`compose/README.md` → HTTPS](compose/README.md#https).

## Container Images

The per-service images used by the Compose recipes are on **GitHub Container Registry**:

- `ghcr.io/elonen/clapshot-server` — API server + organizer
- `ghcr.io/elonen/clapshot-web` — nginx + the web client
- `ghcr.io/elonen/htwicket` — the HTWicket login gateway (its own project)

The all-in-one **demo** image is on **Docker Hub** (demo/eval only, not used by the recipes):

- `elonen/clapshot:latest-demo` (and `…-demo-htwicket`) — single-container demo
