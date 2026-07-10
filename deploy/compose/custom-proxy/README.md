# Clapshot behind your own authenticating proxy

Clapshot with authentication done by **your** reverse proxy against Authentik, Okta, Keycloak,
Kerberos/AD, or anything that authenticates a request, and then set
the `X-Remote-User-*` headers. This nearly the same stack as [`no-auth/`](../no-auth/),
except the nginx `site.conf` **trusts** the inbound `X-Remote-User-*` headers instead of
stripping them and there's no Caddy (your own proxy does TLS termination and auth).

See the header contract and examples in
[Advanced Authentication](../../../doc/sysadmin-guide.md#advanced-authentication). For a
full worked IdP example, see
[Gitea (OIDC) + oauth2-proxy](../../../doc/auth-example-gitea+oauth2proxy.md) — front this
recipe with that oauth2-proxy + nginx `auth_request` layer.

## ⚠️ Security — your proxy is the trust boundary

Because Clapshot trusts the `X-Remote-User-*` headers, **your proxy must**:

1. **TLS terminate**: accept HTTPS/WSS requests from clients.
2. **Authenticate every request** before forwarding.
3. **Set** (replace) `X-Remote-User-Id` / `-Name` / `-Is-Admin` / `-Can-Upload`. It must
   always set these to override what client sent - otherwise a user could
   send `X-Remote-User-Is-Admin: true` and become admin.

And this stack must be reachable **only through your proxy**, never directly from the
internet. Bind it to an internal network / localhost (the defaults below do) and point
your proxy at it. If clients can reach Clapshot directly, they can spoof any identity.

## How it fits together

```
[ your auth proxy ]   ← public entry: TLS + authentication, sets X-Remote-User-*
        │ http, internal only
[ Clapshot stack ]    ← clapshot-web (trusts headers) + clapshot-server
```

This recipe ships **no Caddy** — your proxy is the public entry and TLS terminator, so a
second one would just be a dead hop. Your proxy talks plain HTTP to `clapshot-web` (nginx),
which serves the SPA and proxies `/api`. Set `CLAPSHOT_URL_BASE` to the **public** URL your
proxy serves.

Easiest wiring: put your proxy on this stack's Compose network and target `clapshot-web:80`
(no published port at all — delete `WEB_BIND`/`WEB_PORT`). For a proxy on the host or
elsewhere, publish on a bound port (`WEB_BIND` / `WEB_PORT`) that only your proxy can reach.

## Quick start

```sh
cp .env.example .env        # set CLAPSHOT_URL_BASE + where your proxy reaches this stack
docker compose up -d
```

## What's in here

| File | Edit? | Purpose |
|------|-------|---------|
| `.env` (from `.env.example`) | **yes** | the only file you edit |
| `compose.yml` | no | the stack (clapshot-web [public entry], clapshot-server) — no Caddy |
| `site.conf` | no | nginx: SPA + `/api` proxy; **trusts** inbound `X-Remote-User-*` |
