# Compose recipe smoke tests

End-to-end smoke tests that bring a recipe's stack up and check it actually works: the
stack boots, log in succeeds, a small upload ingests, and the SPA renders. A
headless browser drives the actual web client, so the protobuf WebSocket and upload
path are exercised by the real code.

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

`run.sh`:
1. builds `clapshot-server` + `clapshot-web` locally (single-arch, so they `docker load`),
2. `docker compose up -d` the htwicket recipe with `htwicket.env`,
3. `docker compose run --rm playwright` (its exit code is the test result),
4. `down -v` (always, via trap) for a clean slate next run.

`SKIP_BUILD=1` reuses already-built `clapshot-{server,web}:latest`.

## What the htwicket spec checks

`htwicket.smoke.spec.ts`: unauthenticated `/` → login page → sign in as the seeded `admin`
→ SPA loads (title, `#app`, off the login page, no console errors) → `/htwicket/admin`
reachable as superadmin → upload the clip into the media dropzone → its tile appears.

The admin password is seeded deterministically by `htwicket-init` from
`CLAPSHOT_INITIAL_ADMIN_PASSWORD` in `htwicket.env`. `BASE_URL` is the in-network
`http://caddy/`, so CORS / cookies / the WS URL all agree on one host.

## Files

| File | Purpose |
|------|---------|
| `run.sh` | orchestrates build → up → test → down (the only thing you run) |
| `compose.test.yml` | overlay adding the profile-gated `playwright` runner service |
| `htwicket.env` | test settings for the htwicket recipe (in-network URL, seeded admin pw) |
| `playwright.config.ts` | base URL, timeouts, list reporter (per-step output), output to `/tmp` |
| `package.json` / `package-lock.json` | pinned `@playwright/test`, installed via `npm ci` in-container |
| `global-setup.ts` | polls `/api/health` + `/htwicket/login` so tests don't race startup |
| `htwicket.smoke.spec.ts` | the htwicket end-to-end checks |

## Likely-fragile bits (adjust if a run fails on them)

- **Selectors**: htwicket login uses `#username`/`#password`; the SPA upload uses the
  dropzone's `input[type=file][accept*="video"]`; the uploaded tile is matched by its
  filename title. UI changes may need a selector tweak.
- **htwicket default locale is English** (the recipe sets none), so the `User management`
  text assertion holds; login/submit are matched structurally (locale-agnostic).
- **Transcode time**: the test waits for the *tile* (ingest), not full ffmpeg transcode.
