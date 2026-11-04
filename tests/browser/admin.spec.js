import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import AxeBuilder from '@axe-core/playwright';

async function enterAdmin(page) {
  await page.goto('/slots');
  await page.getByRole('link', { name: 'Admin UI' }).click();
  await expect(page.getByLabel('Password')).toBeFocused();
  await page.getByLabel('Password').fill('admin');
  await page.getByRole('button', { name: 'Enter admin UI' }).click();
  await expect(page.getByRole('button', { name: 'Leave Admin' })).toBeVisible();
}

test('admin mode adds and reorders waitlisted members', async ({ page, context }) => {
  const names = [`Admin test ${randomUUID()}`, `Admin test ${randomUUID()}`];
  for (const name of names) {
    await page.goto('/');
    await page.locator('#new-member > summary').click();
    await page.getByLabel('New member name').fill(name);
    await page.getByRole('button', { name: 'Create and continue' }).click();
    await page.getByRole('button', { name: 'Log out' }).click();
  }
  await context.addCookies([{ name: 'user_id', value: '1', url: 'http://127.0.0.1:5052' }]);
  await enterAdmin(page);
  await expect(page.getByText('Admin mode', { exact: true }).last()).toBeVisible();
  const card = page.getByTestId('slot-card').first();
  await card.locator('.admin-add > summary').click();
  await card.locator('select[name="user_id"]').selectOption({ label: names[0] });
  await card.locator('select[name="list"]').selectOption('waitlist');
  await card.getByRole('button', { name: 'Add member' }).click();
  await expect(card.locator('[data-feedback]')).toHaveText('Member added.');
  await card.locator('.admin-add > summary').click();
  await card.locator('select[name="user_id"]').selectOption({ label: names[1] });
  await card.locator('select[name="list"]').selectOption('waitlist');
  await card.getByRole('button', { name: 'Add member' }).click();
  await expect(card.locator('[data-feedback]')).toHaveText('Member added.');
  await card.locator('.roster > summary').click();
  const rows = card.getByTestId('waitlist-row');
  const name = await rows.last().locator('.member-name').textContent();
  await rows.last().getByRole('button', { name: /Move .* up/ }).click();
  await expect(rows.first().locator('.member-name')).toHaveText(name);
  for (let count = 0; count < 2; count += 1) {
    page.once('dialog', dialog => dialog.accept());
    await rows.first().getByRole('button', { name: 'Remove' }).click();
    await expect(card.locator('[data-feedback]')).toHaveText('Player removed. The waitlist has been updated.');
  }
  await expect(rows).toHaveCount(0);
  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});

test('admin can edit a game and the recurring release setting', async ({ page, context }) => {
  await context.addCookies([{ name: 'user_id', value: '1', url: 'http://127.0.0.1:5052' }]);
  await enterAdmin(page);
  await page.getByRole('link', { name: 'Edit this game' }).first().click();
  const venue = page.getByLabel('Location');
  const original = await venue.inputValue();
  await venue.fill(`${original} test`);
  await page.getByLabel('Maps link (optional)').fill('https://example.com/game-map');
  const keep = page.getByLabel('Keep all bookings');
  if (await keep.count()) await keep.check();
  await page.getByRole('button', { name: 'Save game' }).click();
  await expect(page).toHaveURL(/\/slots\?mode=admin/);
  await expect(page.getByText('Changed game').first()).toBeVisible();
  const map = page.getByRole('link', { name: `${original} test (opens map in a new tab)` });
  await expect(map).toHaveAttribute('href', 'https://example.com/game-map');
  await expect(map).toHaveAttribute('target', '_blank');
  await page.getByRole('link', { name: 'Edit recurring schedule' }).click();
  await expect(page.getByRole('heading', { name: 'Recurring schedule' })).toBeVisible();
  const schedule = page.locator('form').filter({ has: page.getByLabel('Maps link (optional)') }).first();
  const scheduleMap = schedule.getByLabel('Maps link (optional)');
  const originalMap = await scheduleMap.inputValue();
  await scheduleMap.fill('https://example.com/schedule-map');
  await Promise.all([page.waitForEvent('load'), schedule.getByRole('button', { name: /^Save / }).click()]);
  await expect(scheduleMap).toHaveValue('https://example.com/schedule-map');
  await scheduleMap.fill(originalMap);
  await Promise.all([page.waitForEvent('load'), schedule.getByRole('button', { name: /^Save / }).click()]);
  await expect(scheduleMap).toHaveValue(originalMap);
  const release = page.getByLabel('Sunday release time in Berlin');
  await expect(release).toHaveValue('19:00');
  await release.fill('19:05');
  await Promise.all([page.waitForEvent('load'), page.getByRole('button', { name: 'Save release time' }).click()]);
  await expect(release).toHaveValue('19:05');
  await release.fill('19:00');
  await Promise.all([page.waitForEvent('load'), page.getByRole('button', { name: 'Save release time' }).click()]);
  await expect(release).toHaveValue('19:00');
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);
});

test('password controls admin access and leave keeps the member', async ({ page, context }) => {
  await context.addCookies([{ name: 'user_id', value: '2', url: 'http://127.0.0.1:5052' }]);
  await page.goto('/slots?mode=admin');
  await expect(page.getByText('Admin mode', { exact: true })).toHaveCount(0);
  await page.getByRole('link', { name: 'Admin UI' }).click();
  await page.getByLabel('Password').fill('wrong');
  await page.getByRole('button', { name: 'Enter admin UI' }).click();
  await expect(page.getByRole('alert').filter({ hasText: 'Incorrect password' })).toBeVisible();
  await page.getByLabel('Password').fill('admin');
  await page.getByRole('button', { name: 'Enter admin UI' }).click();
  await expect(page.locator('.identity-name')).toContainText('| Admin');
  const session = (await context.cookies()).find(cookie => cookie.name === 'admin_session');
  expect(session.httpOnly).toBe(true);
  expect(session.sameSite).toBe('Lax');
  expect(session.expires - Date.now() / 1000).toBeGreaterThan(43100);
  await page.getByRole('button', { name: 'Leave Admin' }).click();
  await expect(page.getByRole('link', { name: 'Admin UI' })).toBeVisible();
  await expect(page.locator('.identity-name')).not.toContainText('| Admin');
  await page.goto('/admin/schedule');
  await expect(page.getByText('admin access required')).toBeVisible();
});

test('a stale password form cannot change the newly selected member', async ({ page, context }) => {
  await context.addCookies([{ name: 'user_id', value: '1', url: 'http://127.0.0.1:5052' }]);
  await page.goto('/admin/login');
  await context.addCookies([{ name: 'user_id', value: '2', url: 'http://127.0.0.1:5052' }]);
  await page.getByLabel('Password').fill('admin');
  await page.getByRole('button', { name: 'Enter admin UI' }).click();
  await expect(page.getByText('Your selected name changed. Refresh slots before booking.')).toBeVisible();
  expect((await context.cookies()).find(cookie => cookie.name === 'admin_session')).toBeUndefined();
});

test('failed attempts for one member do not lock another out', async ({ page, context }) => {
  await page.goto('/');
  await page.locator('#new-member > summary').click();
  await page.getByLabel('New member name').fill(`Login test ${randomUUID()}`);
  await page.getByRole('button', { name: 'Create and continue' }).click();
  await page.getByRole('link', { name: 'Admin UI' }).click();
  for (let i = 0; i < 5; i += 1) {
    await page.getByLabel('Password').fill('wrong');
    await page.getByRole('button', { name: 'Enter admin UI' }).click();
    await expect(page.getByText('Incorrect password.')).toBeVisible();
  }
  await page.getByLabel('Password').fill('admin');
  await page.getByRole('button', { name: 'Enter admin UI' }).click();
  await expect(page.getByText(/Too many attempts for this member/)).toBeVisible();
  await context.addCookies([{ name: 'user_id', value: '2', url: 'http://127.0.0.1:5052' }]);
  await enterAdmin(page);
});
