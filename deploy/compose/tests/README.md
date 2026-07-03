# Docker Compose recipe smoke tests

End-to-end smoke tests that bring a recipe's stack up and check it actually works: the
stack boots, the SPA renders, login succeeds (where the recipe has one), and a small
upload ingests. A headless browser drives the actual web client, so the protobuf
WebSocket and upload path are exercised by the real code.

## Playwright in container

Playwright runs inside the official Playwright container, joined
to the recipe's Compose network as a profile-gated service. The image ships the browsers but
not the `@playwright/test` runner, so the runner is installed in the ephemeral container.

> **Pinning.** `package-lock.json` pins `@playwright/test`; it must match
> `PLAYWRIGHT_IMAGE`. Pin the image to a digest you trust:
> `PLAYWRIGHT_IMAGE=mcr.microsoft.com/playwright@sha256:… ./run.sh`. To bump the version,
> change `package.json` and regenerate the lock (`npm install --package-lock-only
> --ignore-scripts` inside the image), and update the image tag together.

## Run

```sh
make debian-docker          # once: build the .debs the images install (if not already in dist_deb/)
deploy/compose/tests/run.sh # build images locally, up, test in Docker, down -v
```

`run.sh <recipe>` (default `htwicket`):
1. builds `clapshot-server` + `clapshot-web` locally (single-arch, so they `docker load`),
2. `docker compose up -d` the recipe with `<recipe>/test.env`,
3. `docker compose run --rm playwright` — runs only `<recipe>/`'s specs (its exit code is the test result),
4. `down -v` (always, via trap) for a clean slate next run.

`SKIP_BUILD=1` reuses already-built `clapshot-{server,web}:latest`.

## What the specs check

`htwicket/smoke.spec.ts`: unauthenticated `/` → login page → sign in as the seeded `admin`
→ SPA loads (title, `#app`, off the login page, no console errors) → `/htwicket/admin`
reachable as superadmin → upload the clip into the media dropzone → its tile appears.
The admin password is seeded deterministically by `htwicket-init` from
`CLAPSHOT_INITIAL_ADMIN_PASSWORD` in `htwicket/test.env`.

`no-auth/smoke.spec.ts`: `/` loads the SPA directly with no login page (site.conf strips
`X-Remote-User-*`, so every request is the `anonymous` default user) → upload the clip →
its tile appears. Exercises that anonymous has full access, upload included.

`no-auth/spoof.spec.ts`: a stub `authproxy` (see below) injects admin `X-Remote-User-*`, and
the test asserts the NavBar still shows `anonymous` — i.e. site.conf stripped the spoofed
identity. The strip is no-auth's security contract, so it's tested here.

`custom-proxy/smoke.spec.ts`: this recipe trusts `X-Remote-User-*` from a front proxy and ships
none of its own, so the stub `authproxy` injects them; the test asserts the NavBar shows the
injected user (`Test Admin`) → upload → tile. The strip is the proxy's job here, not Clapshot's,
so it's deliberately NOT tested. The injected name lives in `custom-proxy/authproxy.Caddyfile`.

### Why a proxy injects the headers (not Playwright)

Clapshot reads identity from the `X-Remote-User-*` HTTP headers — including on the **WebSocket
upgrade**, which is where the SPA's identity (`welcome` message → NavBar) comes from. Playwright's
`extraHTTPHeaders` are applied by the browser to HTTP requests but **not** to the WS handshake
(page JS can't set WS headers either). So header injection has to live in the network path: a
tiny Caddy (`overlay.yml` + `authproxy.Caddyfile`) sets the headers and forwards to the stack.
`ws_url` is absolute (`${CLAPSHOT_URL_BASE}api/ws`), so pointing `CLAPSHOT_URL_BASE` at the proxy
routes the WS through it too. htwicket/no-auth-smoke need no proxy (`BASE_URL=http://caddy/`).

## Layout

```
run.sh                  # the only thing you run: build → up → test → down
harness/                # shared, recipe-agnostic Playwright-in-container plumbing
  compose.test.yml      #   overlay adding the profile-gated `playwright` runner service
  playwright.config.ts  #   base URL, timeouts, list reporter; runs $RECIPE's spec folder
  global-setup.ts       #   polls $READY_PROBES so tests don't race startup
  package.json          #   pinned `@playwright/test`, installed via `npm ci` in-container
  package-lock.json
htwicket/               # one folder per recipe under test (mirrors deploy/compose/<recipe>/)
  test.env              #   in-network URL, seeded admin pw, READY_PROBES
  smoke.spec.ts         #   the HTWicket end-to-end checks
no-auth/
  test.env              #   in-network URL, READY_PROBES (no HTWicket)
  smoke.spec.ts         #   anonymous loads + uploads
  spoof.spec.ts         #   spoofed X-Remote-User-* are stripped -> still anonymous
  overlay.yml           #   stub authproxy injecting spoofed headers (optional per-recipe overlay)
  authproxy.Caddyfile   #   the headers it injects + upstream
custom-proxy/
  test.env              #   in-network URL (-> authproxy), READY_PROBES
  smoke.spec.ts         #   trusts injected identity -> uploads as that user
  overlay.yml           #   stub authproxy injecting the trusted headers
  authproxy.Caddyfile   #   the headers it injects + upstream
```

The harness is recipe-agnostic; everything a recipe-test varies lives in its own folder.

### Adding a recipe test

Create `<recipe>/test.env` (set `CLAPSHOT_URL_BASE`, `READY_PROBES`, and whatever the
recipe's `compose.yml` substitutes) plus one or more `<recipe>/*.spec.ts`, then
`run.sh <recipe>`. `run.sh` picks the env file and `playwright.config.ts` scopes the run to
that folder via `RECIPE` — no harness changes needed. If the recipe needs an injecting front
proxy (see above), add an optional `<recipe>/overlay.yml`; `run.sh` applies it last when present.

## Likely-fragile bits (adjust if a run fails on them)

- **Selectors**: HTWicket login uses `#username`/`#password`; the SPA upload uses the
  dropzone's `input[type=file][accept*="video"]`; the uploaded tile is matched by its
  filename title; the current user is the NavBar `<h6>` next to `#user-button`. UI changes
  may need a selector tweak.
- **HTWicket default locale is English** (the recipe sets none), so the `User management`
  text assertion holds; login/submit are matched structurally (locale-agnostic).
- **Transcode time**: the test waits for the *tile* (ingest), not full ffmpeg transcode.
