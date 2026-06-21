# Clapshot with Docker Compose

Self-contained recipes. Each recipe is **one directory you deploy as-is** — the only
file you edit is `.env`. Upgrades are `git pull` + redeploy.

## Topology

```
        ┌─ Caddy ────────────────┐    TLS termination: Let's Encrypt / your certs (/ plain HTTP)
        │  (the only container   │    — the single public entry point
        │   that publishes ports)│
        └───────────┬────────────┘
                    │ http/ws to Docker network
        ┌─ clapshot-web ─────────┐    nginx: serves the web client, proxies /api,
        │                        │    auth_request → the auth layer
        └───────────┬────────────┘
          ┌─────────┴──────────┐
   clapshot-server          [ auth ]  ← htwicket, or nothing, or your own IdP
   + organizer
```

Only **Caddy** publishes host ports; the rest talk over the internal Compose network on
fixed ports. (Exception: the **`custom-proxy`** recipe ships **no Caddy** — your own proxy
is the front and TLS terminator, so there `clapshot-web` publishes the port instead.)

## Recipes (= your auth choice)

| Recipe | Auth | Use for |
|--------|------|---------|
| [`no-auth/`](no-auth/) | none — `anonymous`; inbound `X-Remote-User-*` **stripped** | local dev / evaluation |
| [`htwicket/`](htwicket/) | built-in login form + user manager | simple deployments |
| [`custom-proxy/`](custom-proxy/) | trusts `X-Remote-User-*` from your reverse proxy | production with a real IdP |

For **Authentik, Okta, Keycloak, Kerberos/AD**, etc.: run that in front and use the
[`custom-proxy/`](custom-proxy/) recipe. See
[Advanced Authentication](../../doc/sysadmin-guide.md#advanced-authentication).

## Deploy

The recipes mount files by **relative path** (`./site.conf`, `./htwicket.toml`), so deploy
them **from the git repo** — not by pasting a single file into a web editor.

### Portainer
**Stacks** → **Add stack** → **Repository**:

- Repository URL: this repository
- Compose path: `deploy/compose/htwicket/compose.yml` (or another recipe)
- Set the variables from `.env.example` in the **Environment variables** panel
- Turn on **Automatic updates** (or add a webhook) to redeploy when the repo changes

### Komodo
Create a **Stack** from this git repo with compose path
`deploy/compose/<recipe>/compose.yml`; set the variables in the UI.

### Plain `docker compose`
```sh
cd deploy/compose/htwicket
cp .env.example .env        # then edit it
docker compose up -d
```

**Upgrading** (any method): pull the repo and redeploy. The nginx config and
`htwicket.toml` are versioned in the repo and mounted read-only, so they update with the
pull; your data and users live in volumes and persist. Pin `CLAPSHOT_VERSION` /
`HTWICKET_VERSION` if you want upgrades to be deliberate.

## Configuration (`.env`)

Your edits go here — see each recipe's `.env.example` for the full,
commented set. The essentials:

| Variable | Meaning |
|----------|---------|
| `CLAPSHOT_URL_BASE` | the public URL — the source of truth (its scheme drives http/https everywhere) |
| `CADDY_CERT_DOMAIN` | set → Caddy gets Let's Encrypt certs this hostname; unset → plain HTTP |
| `CADDY_BIND` / `CADDY_HTTP_PORT` / `CADDY_HTTPS_PORT` | where Caddy publishes on the Docker host |
| `CLAPSHOT_DATA_DIR` (and `HTWICKET_DATA_DIR`, `CADDY_DATA_DIR`) | unset → managed volume; set → bind to a host path |

The `CADDY_*` rows apply to **no-auth** and **htwicket**. **`custom-proxy`** has no Caddy:
it uses `WEB_BIND` / `WEB_PORT` for the published port, and TLS is your front proxy's job.

A **`config-check`** step runs on every `up` and stops startup if the settings are inconsistent.

## HTTPS

In the **no-auth** and **htwicket** recipes, Caddy terminates TLS — three modes, selected
by `.env`. (The **`custom-proxy`** recipe has no Caddy; TLS is handled by your own front
proxy, so this section doesn't apply to it.)

1. **Automatic Let's Encrypt (most common).** Set `CADDY_CERT_DOMAIN=clapshot.example.com`,
   point that name's DNS at the host, and open ports **80 and 443**. Caddy obtains and
   renews the certificate.
2. **Your own certificates.** Set `CADDY_TLS_CERT` / `CADDY_TLS_KEY` to mounted PEM files;
   ACME is skipped.
3. **Plain HTTP** (local testing, or behind your own TLS proxy). Leave `CADDY_CERT_DOMAIN`
   unset.

How LE validation works: Caddy uses the HTTP-01 (:80) or TLS-ALPN-01 (:443) challenge,
so **both ports must be publicly reachable**. **Keep the `CADDY_DATA_DIR` volume** — it stores 
the certs and the ACME account; losing it on every restart will hit Let's Encrypt rate limits. 

## Going to production

```sh
CLAPSHOT_URL_BASE=https://clapshot.example.com/
CADDY_CERT_DOMAIN=clapshot.example.com      # LE cert; host must equal the URL's host
CADDY_BIND=0.0.0.0
CADDY_HTTP_PORT=80
CADDY_HTTPS_PORT=443
HTWICKET_INSECURE_COOKIES=false             # htwicket on https (omit if not using htwicket)
CLAPSHOT_VERSION=<pin a released version>
```

## Users (htwicket recipe)

First start seeds a single `admin` user — create everyone else yourself in the admin UI at
`/htwicket/admin` (htwicket's whole job), or with
`docker compose exec htwicket /usr/bin/htwicket user add <name>`.

There is no default admin password. You can either:

- set the admin password with `CLAPSHOT_INITIAL_ADMIN_PASSWORD` (or `…_FILE` for a
Docker/Swarm secret); or
- leave it unset to get a random one printed in the logs. It's applied
**only on first run** (or, when missing) — change it later in the GUI, or by running (this prompts for new password):

```
docker compose exec htwicket /usr/bin/htwicket user passwd admin
```

## Data & backups

By default each store is a **managed Docker volume**. To put data on a **host path**
(e.g. on a NAS for easy backups), set the matching `*_DIR` variable to an absolute path.
Each service runs as a different user, so the host path must be writable by **that
service's uid** (a fresh named volume is chowned automatically; a host bind is not):

| Variable | Holds | Host path writable by | Back up? |
|----------|-------|-----------------------|----------|
| `CLAPSHOT_DATA_DIR` | videos + SQLite database | uid `33` (`www-data`) | **yes — the important one** |
| `HTWICKET_DATA_DIR` | users (`.htpasswd`), per-user flags, session key | uid `65532` (`nonroot`) | yes |
| `CADDY_DATA_DIR` | TLS certs + ACME account | uid `0` (`root`) | optional (re-fetchable, but avoids rate limits) |
