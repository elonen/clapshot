import { defineConfig } from '@playwright/test';

// Runs INSIDE the official Playwright container (see compose.test.yml).
// baseURL points at the recipe's in-network ingress (http://caddy/).
export default defineConfig({
  testDir: '.',
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
