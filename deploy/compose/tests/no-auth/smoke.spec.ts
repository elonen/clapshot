import { test, expect } from '@playwright/test';

// End-to-end smoke for the no-auth recipe: a real browser drives the client SPA. There is no
// login — site.conf strips X-Remote-User-* so every request is the 'anonymous' default user,
// which has full access (upload included). Steps below print individually (list reporter), so
// a failure names the phase that broke.

test('no-auth: SPA loads as anonymous -> upload -> client renders', async ({ page }) => {
  // Watch for severe client errors throughout (favicon 404s are benign noise).
  const errors: string[] = [];
  page.on('console', (m) => {
    if (m.type() === 'error' && !/favicon/i.test(m.text())) errors.push(m.text());
  });

  await test.step('visiting / loads the SPA directly (no login page)', async () => {
    await page.goto('/');
    await expect(page).toHaveTitle(/Clapshot/i);
    await expect(page.locator('#app')).toBeVisible();
    await expect(page.locator('#username')).toHaveCount(0);   // no auth gateway in front
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
