# deploy/docker — per-service runtime images

Dockerfiles and entrypoints for the two **runtime images** the Compose recipes use:

| Image | Built from | Contents |
|-------|-----------|----------|
| `clapshot-server` | `clapshot-server.Dockerfile` | API server **+** basic-folders organizer (server spawns the organizer via `org-cmd`, so they share one container) |
| `clapshot-web` | `clapshot-web.Dockerfile` | nginx + the web client SPA + shared, **auth-agnostic** proxy snippets |

Both are built from the `.deb` packages so the container and VM / bare-metal (`deploy/debian/`) installs run the exact same binaries.

## Build

Context is the **repo root** (the Dockerfiles `COPY dist_deb/...`):

```sh
make build-docker-services                 # build both, multi-arch (amd64+arm64)
make build-docker-services-and-push-ghcr   # build + push to repo
```

(`GHCR_NS` overrides the registry namespace.)

## Files

- **`clapshot-server.Dockerfile`** — Clapshot server + ffmpeg/mediainfo/sqlite. Installs server + organizer debs. Runs as `www-data`, listens on `:8095`, data dir on a mounted volume at `/mnt/clapshot-data/data`.
	- **`server-entrypoint.sh`** — applies `CLAPSHOT_SERVER__*` env vars onto
  `/etc/clapshot-server.conf` (e.g. `CLAPSHOT_SERVER__URL_BASE` → `url-base`), then execs the
  deb's `run-with-conf.sh` (the `CMD`) which turns the conf into CLI args.
	- **`config-check.sh`** — baked into the server image as `clapshot-config-check`. The recipes run
  it as a one-shot service the others `depend_on`; it validates the cross-service `.env` contract
  (scheme vs. cookies, cert domain vs. URL host) and **fails fast** with a fix-it message.
- **`clapshot-web.Dockerfile`** — packages the client SPA, bakes the Nginx snippets and the container entrypoint script below.
	- **`web-entrypoint.sh`** — renders the runtime client config (`/etc/clapshot_client.conf`, served
  as `/clapshot_client.conf.json`) from env at container start.
	- **`snippets/clapshot-proxy-params.conf`** — reverse-proxy mechanics shared by every recipe
  (WebSocket upgrade, 2h timeouts, streamed 50 GB uploads). Included inside `location /api`.
	- **`snippets/clapshot-cache.conf`** — cache-control rules for the web client. Included inside `location /`.

## What's deliberately NOT here

**Authentication and TLS.** These images are intentionally auth- and TLS-agnostic so the same
`clapshot-web` works behind no-auth, HTWicket, or your own IdP. Those concerns live in the recipes
([`../compose/`](../compose/)): each mounts its own `site.conf` (which sets/strips the
`X-Remote-User-*` headers) over the baked default, and Caddy terminates TLS in front.
