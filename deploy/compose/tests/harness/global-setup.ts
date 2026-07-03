import { request } from '@playwright/test';

// The stack is brought up with `docker compose up -d`, but one-shots (config-check,
// htwicket-init) and healthchecks may still be settling. Poll the unauthenticated
// endpoints until they answer, so tests don't race startup. Which endpoints depends on
// the recipe (no-auth has no HTWicket), so READY_PROBES comes from the recipe's test.env.
export default async function globalSetup() {
  const base = process.env.BASE_URL || 'http://caddy/';
  const ctx = await request.newContext({ baseURL: base, ignoreHTTPSErrors: true });
  const probes = (process.env.READY_PROBES || 'api/health').split(',').map((p) => p.trim()).filter(Boolean);
  const deadlineMs = Date.now() + 120_000;

  for (const path of probes) {
    for (;;) {
      try {
        const r = await ctx.get(path, { timeout: 5_000 });
        if (r.ok()) break;
      } catch { /* not up yet */ }
      if (Date.now() > deadlineMs) throw new Error(`Timed out waiting for ${base}${path}`);
      await new Promise((res) => setTimeout(res, 2_000));
    }
  }
  await ctx.dispose();
}
