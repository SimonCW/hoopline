import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import AxeBuilder from '@axe-core/playwright';

async function createMember(page) {
  const name = `Member ${randomUUID().slice(0, 8)}`;
  await page.goto('/');
  await page.locator('#new-member > summary').click();
  await page.getByLabel('New member name', { exact: true }).fill(name);
  await page.getByRole('button', { name: 'Create and continue' }).click();
  await expect(page).toHaveURL(/\/slots$/);
  await expect(page.locator('.identity-name')).toHaveText(name);
  return name;
}

test('compact cards, remembered identity, signup and safe cancellation', async ({ page }) => {
  await createMember(page);
  await expect(page.getByRole('combobox')).toHaveCount(0);
  const card = page.getByTestId('slot-card').first();
  await expect(card.locator('.roster')).not.toHaveAttribute('open', '');
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  await expect(card.locator('[data-feedback]')).toContainText('Booking saved');
  await expect(page.locator('#announcements')).toContainText('Booking saved');
  await card.locator('.roster > summary').click();
  await expect(card.locator('.current-player')).toBeVisible();
  await page.reload();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  page.once('dialog', dialog => dialog.dismiss());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  await expect(card.locator('[data-feedback]')).toContainText('Booking cancelled');
});

test('compact overview header shows the brand above a highlighted member greeting', async ({ page }) => {
  const name = await createMember(page);
  const header = page.locator('#user-selector');
  await expect(page.getByRole('heading', { name: 'Hoopline', exact: true })).toHaveCount(1);
  await expect(header.locator('.member-greeting')).toHaveText(`You're up next, ${name}`);
  await expect(header.locator('.member-greeting > strong.identity-name')).toBeVisible();
  const brand = await header.getByRole('heading', { name: 'Hoopline' }).boundingBox();
  const greeting = await header.locator('.member-greeting').boundingBox();
  const logout = await header.getByRole('button', { name: 'Log out' }).boundingBox();
  expect(brand.y).toBeLessThan(greeting.y);
  expect(logout.x).toBeGreaterThan(brand.x + brand.width);
  await header.getByRole('button', { name: 'Refresh slots' }).click();
  await expect(page.locator('#announcements')).toHaveText('Upcoming slots refreshed.');
  await expect(header.locator('.member-greeting > strong')).toHaveText(name);
});

test('phone widths, long names, touch targets and keyboard selection', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Your name' }).fill('Bne');
  await expect(page.getByRole('option', { name: 'Ben', exact: true })).toBeVisible();
  await page.getByRole('combobox').press('ArrowDown');
  await page.getByRole('combobox').press('Enter');
  await expect(page.getByRole('combobox')).toHaveValue('Ben');
  await page.getByRole('combobox').press('Tab');
  await expect(page.getByRole('button', { name: 'Continue', exact: true })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('.identity-name')).toHaveText('Ben');
  await page.getByRole('button', { name: 'Log out' }).click();
  await page.locator('#new-member > summary').click();
  await page.getByLabel('New member name', { exact: true }).fill(`Long${'name'.repeat(15)}`);
  await page.getByRole('button', { name: 'Create and continue' }).click();
  await expect(page.getByRole('combobox')).toHaveCount(0);
  for (const width of [320, 390, 430]) {
    await page.setViewportSize({ width, height: 844 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const button = page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true });
    const bounds = await button.boundingBox();
    expect(bounds.height).toBeGreaterThanOrEqual(44);
    expect(bounds.width).toBeGreaterThanOrEqual(44);
  }
});

test('failed booking stays unchanged and shows a local error', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  await page.route('**/slots/*/signup', route => route.fulfill({
    status: 409, contentType: 'text/html', body: '<main><p>The list changed. Refresh and try again.</p></main>',
  }));
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByRole('alert')).toHaveText('The list changed. Refresh and try again.');
  await expect(card.getByRole('alert')).toBeFocused();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  await expect(card.locator('[data-feedback]')).toHaveCount(0);
  await expect(card.getByRole('button', { name: 'Sign Up', exact: true })).toBeEnabled();
});

test('network failure stays unchanged and permits retry', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  await page.route('**/slots/*/signup', route => route.abort('failed'));
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByRole('alert')).toContainText('Connection failed');
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  await expect(card.getByRole('button', { name: 'Sign Up', exact: true })).toBeEnabled();
});

test('slow request disables repeat submits and keeps roster open', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  await card.locator('.roster > summary').click();
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  let requests = 0;
  await page.route('**/slots/*/signup', async route => {
    requests += 1;
    await gate;
    await route.continue();
  });
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByRole('button', { name: 'Sign Up', exact: true })).toBeDisabled();
  await expect(card).toHaveAttribute('aria-busy', 'true');
  await expect(card.locator('.htmx-indicator')).toBeVisible();
  // A second synthetic submit must also be dropped, not queued.
  await card.locator('form').first().evaluate(form => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
  expect(requests).toBe(1);
  release();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  await expect(card.locator('.roster')).toHaveAttribute('open', '');
  await expect(card.locator('h3')).toBeFocused();
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('focus refresh discovers a booking made in another session', async ({ page, request }) => {
  const name = await createMember(page);
  const identityResponse = await request.post('/users/select', { form: { existing_user_name: name }, maxRedirects: 0 });
  const cookie = identityResponse.headers()['set-cookie'].split(';')[0];
  const card = page.getByTestId('slot-card').first();
  const slotId = (await card.getAttribute('id')).replace('slot-', '');
  await request.post(`/slots/${slotId}/signup`, { headers: { Cookie: cookie } });
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('waitlist status updates after automatic promotion', async ({ page, request }) => {
  const name = await createMember(page);
  const card = page.getByTestId('slot-card').first();
  const slotId = (await card.getAttribute('id')).replace('slot-', '');
  const [count, capacity] = (await card.locator('.availability strong').first().textContent()).split('/').map(Number);
  const cookies = [];
  try {
    for (let index = count; index < capacity; index += 1) {
      const response = await request.post('/users/select', { form: { new_user_name: `Fill ${randomUUID().slice(0, 8)}` }, maxRedirects: 0 });
      const cookie = response.headers()['set-cookie'].split(';')[0];
      cookies.push(cookie);
      expect((await request.post(`/slots/${slotId}/signup`, { headers: { Cookie: cookie } })).ok()).toBe(true);
    }
    await page.getByRole('button', { name: 'Refresh slots' }).click();
    await card.getByRole('button', { name: 'Join Waitlist', exact: true }).click();
    await expect(card.getByTestId('booking-status')).toHaveText('Waitlist #1');
    await expect(card.locator('[data-feedback]')).toContainText('Waitlist #1');
    await request.post(`/slots/${slotId}/cancel`, { headers: { Cookie: cookies[0] } });
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
    await card.locator('.roster > summary').click();
    await expect(card.locator('.current-player')).toContainText(name);
    page.once('dialog', dialog => dialog.accept());
    await card.getByRole('button', { name: 'Cancel booking' }).click();
    await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  } finally {
    for (const cookie of cookies) await request.post(`/slots/${slotId}/cancel`, { headers: { Cookie: cookie } });
  }
});

test('member forms and expanded rosters pass automated accessibility checks', async ({ page }) => {
  await page.goto('/');
  await page.locator('#new-member > summary').click();
  await page.getByRole('combobox').fill('Jmli');
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  expect(results.violations).toEqual([]);
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  await card.locator('.roster > summary').click();
  const bookedResults = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  expect(bookedResults.violations).toEqual([]);
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('focus refresh keeps identity header aligned with the cookie', async ({ page }) => {
  await createMember(page);
  await page.context().addCookies([{ name: 'user_id', value: '2', url: 'http://127.0.0.1:5051' }]);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.locator('.identity-name')).toHaveText('Ben');
});

test('a focus refresh during an older response is not lost', async ({ page }) => {
  await createMember(page);
  let release;
  let captured;
  const gate = new Promise(resolve => { release = resolve; });
  const capture = new Promise(resolve => { captured = resolve; });
  let first = true;
  await page.route('**/slots/fragment', async route => {
    if (!first) return route.continue();
    first = false;
    const response = await route.fetch();
    captured();
    await gate;
    await route.fulfill({ response });
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await capture;
  await page.context().addCookies([{ name: 'user_id', value: '2', url: 'http://127.0.0.1:5051' }]);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  release();
  await expect(page.locator('.identity-name')).toHaveText('Ben');
});

test('focus refresh does not erase a name being typed', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('combobox').fill('Jam');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('combobox')).toHaveValue('Jam');
  await expect(page.getByRole('option', { name: 'Jamali', exact: true })).toBeVisible();
});

test('failed cancellation keeps the booking and hides old success feedback', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.locator('[data-feedback]')).toBeVisible();
  await page.route('**/slots/*/cancel', route => route.fulfill({
    status: 503, contentType: 'text/html', body: '<main><p>System busy. Refresh and try again.</p></main>',
  }));
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByRole('alert')).toContainText('System busy');
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  await expect(card.locator('[data-feedback]')).toBeHidden();
  await page.unroute('**/slots/*/cancel');
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('fuzzy search allows explicit pointer selection and does not guess a user', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('combobox').fill('Jmli');
  await expect(page.getByRole('option', { name: 'Jamali', exact: true })).toBeVisible();
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(page.locator('#name-step').getByRole('alert')).toContainText('Choose your name from the list');
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  await page.getByRole('combobox').focus();
  await page.getByRole('option', { name: 'Jamali', exact: true }).click();
  await expect(page.getByRole('combobox')).toHaveValue('Jamali');
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(page.locator('.identity-name')).toHaveText('Jamali');
  await expect(page.getByTestId('slot-card').first()).toBeVisible();
  await expect(page.getByText('Berlin local time')).toHaveCount(0);
});

test('cookie remembers a member until logout, without cancelling their booking', async ({ page }) => {
  const name = await createMember(page);
  const cookies = await page.context().cookies();
  const identity = cookies.find(cookie => cookie.name === 'user_id');
  expect(identity.httpOnly).toBe(true);
  expect(identity.sameSite).toBe('Lax');
  expect(identity.expires).toBeGreaterThan(Date.now() / 1000 + 350 * 86400);
  const card = page.getByTestId('slot-card').first();
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  await page.goto('/');
  await expect(page.locator('.identity-name')).toHaveText(name);
  await expect(page.getByRole('combobox')).toHaveCount(0);
  await page.getByRole('button', { name: 'Log out' }).click();
  await expect(page.getByRole('combobox')).toBeVisible();
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  expect((await page.context().cookies()).find(cookie => cookie.name === 'user_id')).toBeUndefined();
  await page.goto('/slots');
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  await page.getByRole('combobox').fill(name);
  await page.getByRole('option', { name, exact: true }).click();
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed player');
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('logout in another tab returns the overview to name selection on focus', async ({ page, context }) => {
  await createMember(page);
  const otherTab = await context.newPage();
  await otherTab.goto('/');
  await otherTab.getByRole('button', { name: 'Log out' }).click();
  await page.bringToFront();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('combobox')).toBeVisible();
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  await otherTab.close();
});
