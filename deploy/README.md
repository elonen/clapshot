# Deploying Clapshot

Ways to run Clapshot, from a 30-second look to a real install. Pick one:

| Path | For | Where |
|------|-----|-------|
| **Docker Compose** | Portainer, Komodo, plain `docker compose` | [`compose/`](compose/) |
| **Debian package + systemd** | A VM/host without containers | [`debian/`](debian/) |
| **Single-image demo** | A quick, no-login look | `docker run` (see [below](#single-image-demo-no-auth)) |

## How Clapshot thinks about authentication

Clapshot has **no login system of its own**. The API server trusts a few HTTP headers
set by whatever sits in front of it:

- `X-Remote-User-Id` — stable user id
- `X-Remote-User-Name` — display name
- `X-Remote-User-Is-Admin` — `true` / `false`
- `X-Remote-User-Can-Upload` — `true` / `false`

**Anything** that can authenticate a request and set those headers works: a login
gateway, company SSO, an identity-aware proxy, Kerberos, … The Compose recipes ship three
ready options:

- **no-auth** — no login; every request is the `anonymous` user. nginx **strips** any
  inbound `X-Remote-User-*` so they can't be spoofed. Dev / demo only.
- **htwicket** — a small built-in login form + user manager. Fine for small or internal deployments.
- **custom-proxy** — Clapshot behind **your own** authenticating reverse proxy: nginx
  trusts the headers your proxy sets. The path for real identity providers.

> **htwicket is the _floor_, not a recommendation.** This is a modernized replacement for the old
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

> **Run anything public over HTTPS.** Plain HTTP is for local testing only.

See [`compose/README.md` → HTTPS](compose/README.md#https).

## Single-image demo (no auth)

A throwaway, no-login look in one command:

```sh
docker run --rm -it -p 8080:80 -v clapshot-demo:/mnt/clapshot-data/data \
  ghcr.io/elonen/clapshot:demo
```

Open <http://127.0.0.1:8080/>. This is a convenience image, **not** for production — use
a Compose recipe or `.deb` packages for that.

## Container Images

All images are published to GitHub Container Registry:

- `ghcr.io/elonen/clapshot-server` — API server + organizer
- `ghcr.io/elonen/clapshot-web` — nginx + the web client
- `ghcr.io/elonen/htwicket` — the htwicket login gateway (its own project)
- `ghcr.io/elonen/clapshot:demo` — the all-in-one no-auth demo
