import { test, expect } from '@playwright/test';

// End-to-end smoke for the HTWicket recipe: a real browser drives the client SPA.
// Admin password is seeded by htwicket-init from CLAPSHOT_INITIAL_ADMIN_PASSWORD.
// Steps below print individually (list reporter), so a failure names the phase that broke.
const ADMIN_PW = process.env.CLAPSHOT_INITIAL_ADMIN_PASSWORD || '';

test('htwicket: login -> admin UI -> upload -> client renders', async ({ page }) => {
  expect(ADMIN_PW, 'CLAPSHOT_INITIAL_ADMIN_PASSWORD must be set for the test').not.toBe('');

  // Watch for severe client errors throughout (favicon 404s are benign noise).
  const errors: string[] = [];
  page.on('console', (m) => {
    if (m.type() === 'error' && !/favicon/i.test(m.text())) errors.push(m.text());
  });

  await test.step('unauthenticated visit shows the HTWicket login page', async () => {
    await page.goto('/');
    await expect(page.locator('#username')).toBeVisible();
    await expect(page.locator('#password')).toBeVisible();
  });

  await test.step('sign in as the seeded admin', async () => {
    await page.fill('#username', 'admin');
    await page.fill('#password', ADMIN_PW);
    await page.locator('form button[type=submit]').first().click();   // first submit = "Sign in" (locale-agnostic)
  });

  await test.step('SPA loads and we are off the login page', async () => {
    await expect(page).toHaveTitle(/Clapshot/i);
    await expect(page.locator('#app')).toBeVisible();
    await expect(page.locator('#username')).toHaveCount(0);
  });

  await test.step('HTWicket admin UI is reachable for the superadmin', async () => {
    await page.goto('/htwicket/admin');
    await expect(page.getByText('User management')).toBeVisible();
  });

  await test.step('upload a clip into the media dropzone', async () => {
    await page.goto('/');
    await page.locator('input[type=file][accept*="video"]').setInputFiles('/assets/sample.mp4');
  });

  await test.step('uploaded item appears as a tile', async () => {
    // Title = the uploaded filename, which is sample.mp4 (the mount path in compose.test.yml).
    // Ingest done; transcode may still be running.
    await expect(page.getByTitle(/sample\.mp4/i).first()).toBeVisible({ timeout: 90_000 });
  });

  expect(errors, `console errors:\n${errors.join('\n')}`).toEqual([]);
});
