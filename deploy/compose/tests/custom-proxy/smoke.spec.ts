import { test, expect } from '@playwright/test';

// End-to-end smoke for the custom-proxy recipe. A stub 'authproxy' (overlay.yml) stands in for
// your real reverse proxy: it injects X-Remote-User-* on every request, including the WebSocket
// upgrade (a browser's extraHTTPHeaders can't reach that — hence the proxy). site.conf TRUSTS
// those headers, so the SPA loads as the injected user with no login. The injected name must
// be set in authproxy.Caddyfile. Steps print individually so a failure names the phase.
const USER_NAME = 'Test Admin';   // = X-Remote-User-Name in authproxy.Caddyfile

test('custom-proxy: trusts injected identity -> upload -> client renders', async ({ page }) => {
  // Watch for severe client errors throughout (favicon 404s are benign noise).
  const errors: string[] = [];
  page.on('console', (m) => {
    if (m.type() === 'error' && !/favicon/i.test(m.text())) errors.push(m.text());
  });

  await test.step('SPA loads directly as the injected user (no login page)', async () => {
    await page.goto('/');
    await expect(page).toHaveTitle(/Clapshot/i);
    await expect(page.locator('#app')).toBeVisible();
    await expect(page.locator('#username')).toHaveCount(0);   // no auth gateway in front
    // The username arrives over the WS welcome; seeing it proves the injected header propagated
    // authproxy -> nginx (trusts) -> server -> WS -> client, including across the WS upgrade.
    await expect(page.locator('span:has(#user-button) h6')).toHaveText(USER_NAME);
  });

  await test.step('upload a clip into the media dropzone', async () => {
    await page.locator('input[type=file][accept*="video"]').setInputFiles('/assets/sample.mp4');
  });

  await test.step('uploaded item appears as a tile', async () => {
    // Title = the uploaded filename, which is sample.mp4 (the mount path in compose.test.yml).
    // Ingest done; transcode may still be running.
    await expect(page.getByTitle(/sample\.mp4/i).first()).toBeVisible({ timeout: 90_000 });
  });

  expect(errors, `console errors:\n${errors.join('\n')}`).toEqual([]);
});
