import { test, expect } from './fixtures.js';

test('a player can book, refresh, and cancel in German without changing identity', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Deutsch' }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');
  await expect(page.getByRole('button', { name: 'Deutsch' })).toHaveAttribute('aria-current', 'true');
  await page.getByRole('combobox', { name: 'Dein Name' }).fill('Jamali');
  await expect(page.locator('#search-status')).toHaveText(/1 passender Name/);
  await page.getByRole('option', { name: 'Jamali', exact: true }).click();
  await expect(page.locator('#search-status')).toHaveText(/Jamali ausgewählt/);
  await page.getByRole('button', { name: 'Weiter', exact: true }).click();

  const card = page.locator('#slot-2');
  await expect(card.getByRole('heading', { level: 3 })).toHaveText('Di., 08.01.2030 · 20:00 Uhr');
  await card.getByRole('button', { name: 'Anmelden', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Dabei 🏀');
  await expect(page.locator('#announcements')).toHaveText(/Anmeldung gespeichert/);
  await page.getByRole('button', { name: 'Spiele aktualisieren' }).click();
  await expect(page.locator('#announcements')).toHaveText('Die nächsten Spiele wurden aktualisiert.');
  await expect(card.getByText('Wer spielt mit?')).toBeVisible();
  const dialogText = page.waitForEvent('dialog').then(async dialog => {
    const text = dialog.message();
    await dialog.accept();
    return text;
  });
  await card.getByRole('button', { name: 'Anmeldung zurückziehen' }).click();
  expect(await dialogText).toMatch(/^Anmeldung zurückziehen\?/);
  await expect(card.getByTestId('booking-status')).toHaveText('Nicht angemeldet');
  await expect(page.locator('#announcements')).toHaveText(/Anmeldung zurückgezogen/);

  await page.goto('/slots#slot-2');
  await page.getByRole('button', { name: 'English' }).click();
  await expect(page).toHaveURL(/\/slots#slot-2$/);
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.locator('.identity-name')).toHaveText('Jamali');
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('admin pages keep German through login, updates, and validation', async ({ app }) => {
  const { page } = await app.newActor('Alex');
  await page.getByRole('link', { name: 'Admin UI' }).click();
  await page.getByRole('button', { name: 'Deutsch' }).click();
  await expect(page).toHaveURL(/\/admin\/login$/);
  await page.getByLabel('Passwort', { exact: true }).fill('admin');
  await page.getByRole('button', { name: 'Admin-Bereich öffnen' }).click();
  await expect(page).toHaveURL(/\/slots\?mode=admin$/);
  await expect(page.getByText('Admin-Modus')).toBeVisible();

  const edit = page.locator('#slot-2').getByRole('link', { name: 'Spiel bearbeiten' });
  await edit.click();
  await expect(page.getByRole('heading', { name: 'Spiel bearbeiten' })).toBeVisible();
  await expect(page.getByLabel('Spielort')).toHaveValue('Ceci');
  await page.getByRole('link', { name: 'Zurück zu den Spielen im Admin-Bereich' }).click();

  await page.getByRole('link', { name: 'Spielplan bearbeiten' }).click();
  await expect(page.getByRole('heading', { name: 'Regelmäßiger Spielplan' })).toBeVisible();
  await page.getByRole('button', { name: 'Freigabezeit speichern' }).click();
  await expect(page.getByRole('heading', { name: 'Freigabe am Sonntag' })).toBeVisible();
  const error = await page.request.post('/admin/release-time', {
    headers: { 'X-Hoopline-User': '1' },
    form: { sunday_release_time: '02:00', revision: '1' },
  });
  expect(error.status()).toBe(400);
  expect(await error.text()).toContain('Wähle eine Freigabezeit zwischen 04:00 und 23:59 Uhr');

  await page.getByRole('button', { name: 'English' }).click();
  await expect(page).toHaveURL(/\/admin\/schedule$/);
  await expect(page.getByRole('heading', { name: 'Recurring schedule' })).toBeVisible();
});
