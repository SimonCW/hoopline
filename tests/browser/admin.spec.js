import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import AxeBuilder from '@axe-core/playwright';

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
  await page.goto('/slots?mode=admin');
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
  await page.goto('/slots?mode=admin');
  await page.getByRole('link', { name: 'Edit this game' }).first().click();
  const venue = page.getByLabel('Location');
  const original = await venue.inputValue();
  await venue.fill(`${original} test`);
  const keep = page.getByLabel('Keep all bookings');
  if (await keep.count()) await keep.check();
  await page.getByRole('button', { name: 'Save game' }).click();
  await expect(page).toHaveURL(/\/slots\?mode=admin/);
  await expect(page.getByText('Changed game').first()).toBeVisible();
  await page.getByRole('link', { name: 'Edit recurring schedule' }).click();
  await expect(page.getByRole('heading', { name: 'Recurring schedule' })).toBeVisible();
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
