import { defineConfig } from '@playwright/test';

// Runs INSIDE the official Playwright container (see compose.test.yml).
// baseURL points at the recipe's in-network ingress (http://caddy/).
// RECIPE (set by run.sh) selects which recipe's spec folder to run; the recipe
// folders sit alongside this harness/ dir, hence '../'. Falls back to the whole tree.
const RECIPE = process.env.RECIPE;
export default defineConfig({
  testDir: RECIPE ? `../${RECIPE}` : '..',
  outputDir: '/tmp/test-results',     // /tests is mounted read-only
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  reporter: [['list']],
  globalSetup: './global-setup.ts',
  use: {
    baseURL: process.env.BASE_URL || 'http://caddy/',
    actionTimeout: 15_000,
    ignoreHTTPSErrors: true,
    trace: 'retain-on-failure',
  },
});
