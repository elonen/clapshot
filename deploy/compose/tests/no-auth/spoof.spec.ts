import { test, expect } from '@playwright/test';

// no-auth/site.conf STRIPS inbound X-Remote-User-* (it sets them empty, so nginx drops them),
// meaning a client can't spoof identity: even when admin headers arrive, the server sees none
// and falls back to the 'anonymous' default user. The stub 'authproxy' (overlay.yml) injects
// those spoofed headers on every request — including the WS upgrade, the path that actually
// carries identity — so this genuinely exercises the strip. This is no-auth's security contract
// and Clapshot's responsibility here; in custom-proxy the strip is the front proxy's job, so it
// is deliberately NOT tested there.
test('no-auth: spoofed X-Remote-User-* headers are stripped -> still anonymous', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#app')).toBeVisible();
  // Identity arrives over the WS welcome. Had the spoofed headers survived the strip, the NavBar
  // would show "Spoofed Hacker"; stripped, it shows the anonymous default user (--default-user).
  const username = page.locator('span:has(#user-button) h6');
  await expect(username).toHaveText(/anonymous/i);
  await expect(username).not.toHaveText(/hacker/i);
});
