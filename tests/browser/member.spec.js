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

async function overviewLayout(page) {
  return page.evaluate(() => {
    const card = document.querySelector('[data-testid="slot-card"]');
    return {
      header: document.getElementById('user-selector').getBoundingClientRect().toJSON(),
      card: card.getBoundingClientRect().toJSON(),
      action: card.querySelector('.slot-actions button').getBoundingClientRect().toJSON(),
      pageHeight: document.documentElement.scrollHeight,
    };
  });
}

test('compact cards, remembered identity, signup and safe cancellation', async ({ page }) => {
  await createMember(page);
  await expect(page.getByRole('combobox')).toHaveCount(0);
  const card = page.getByTestId('slot-card').first();
  await expect(card.locator('.roster')).not.toHaveAttribute('open', '');
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  await expect(card.locator('[data-feedback]')).toContainText('Booking saved');
  await expect(page.locator('#announcements')).toContainText('Booking saved');
  await card.locator('.roster > summary').click();
  await expect(card.locator('.current-player')).toBeVisible();
  await page.reload();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  page.once('dialog', dialog => dialog.dismiss());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
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
  await page.getByLabel('New member name', { exact: true }).fill(`Long${'name'.repeat(13)}${randomUUID().slice(0, 8)}`);
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
  await expect(page.getByTestId('slot-card').nth(1).getByRole('button', { name: 'Sign Up', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
  await expect(page.locator('#booking-request-status')).toBeHidden();
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
  await expect(page.locator('#booking-request-status')).toHaveClass('sr-only');
  await expect(page.locator('#booking-request-status')).toContainText('Saving changes');
  // A second synthetic submit must also be dropped, not queued.
  await card.locator('form').first().evaluate(form => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
  expect(requests).toBe(1);
  release();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  await expect(card.locator('.roster')).toHaveAttribute('open', '');
  await expect(card.locator('h3')).toBeFocused();
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
});

test('focus refresh discovers a booking made in another session', async ({ page, request }) => {
  await createMember(page);
  const identity = (await page.context().cookies()).find(cookie => cookie.name === 'user_id');
  const cookie = `user_id=${identity.value}`;
  const card = page.getByTestId('slot-card').first();
  const slotId = (await card.getAttribute('id')).replace('slot-', '');
  await request.post(`/slots/${slotId}/signup`, { headers: { Cookie: cookie, 'X-Hoopline-User': identity.value } });
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
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
      expect((await request.post(`/slots/${slotId}/signup`, { headers: { Cookie: cookie, 'X-Hoopline-User': cookie.split('=')[1] } })).ok()).toBe(true);
    }
    await page.getByRole('button', { name: 'Refresh slots' }).click();
    await card.getByRole('button', { name: 'Join Waitlist', exact: true }).click();
    await expect(card.getByTestId('booking-status')).toHaveText('Waitlist #1');
    await expect(card.locator('[data-feedback]')).toContainText('Waitlist #1');
    await request.post(`/slots/${slotId}/cancel`, { headers: { Cookie: cookies[0], 'X-Hoopline-User': cookies[0].split('=')[1] } });
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    await card.locator('.roster > summary').click();
    await expect(card.locator('.current-player')).toContainText(name);
    page.once('dialog', dialog => dialog.accept());
    await card.getByRole('button', { name: 'Cancel booking' }).click();
    await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  } finally {
    for (const cookie of cookies) await request.post(`/slots/${slotId}/cancel`, { headers: { Cookie: cookie, 'X-Hoopline-User': cookie.split('=')[1] } });
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
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
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
  await expect(card.locator('[data-feedback]')).toHaveClass('sr-only');
  await expect(card.locator('[data-feedback]')).toContainText('Booking saved');
  await page.route('**/slots/*/cancel', route => route.fulfill({
    status: 503, contentType: 'text/html', body: '<main><p>System busy. Refresh and try again.</p></main>',
  }));
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByRole('alert')).toContainText('System busy');
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
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
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
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
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
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

for (const action of ['signup', 'cancel']) {
  test(`refresh disables ${action} until the new overview has settled`, async ({ page }) => {
    await createMember(page);
    const card = page.getByTestId('slot-card').first();
    if (action === 'cancel') {
      await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
      await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    }
    let release;
    let captured;
    const gate = new Promise(resolve => { release = resolve; });
    const capture = new Promise(resolve => { captured = resolve; });
    await page.route('**/slots/fragment', async route => {
      const response = await route.fetch();
      captured();
      await gate;
      await route.fulfill({ response });
    });
    const requests = [];
    page.on('request', request => {
      if (request.method() === 'POST' && request.url().endsWith(`/${action}`)) requests.push(request);
    });
    await page.evaluate(() => { htmx.config.defaultSettleDelay = 400; window.dispatchEvent(new Event('focus')); });
    await capture;
    const button = card.getByRole('button', { name: action === 'signup' ? 'Sign Up' : 'Cancel booking', exact: true });
    await expect(button).toBeDisabled();
    await expect(page.getByTestId('slot-card').nth(1).getByRole('button', { name: 'Sign Up', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Refresh slots' })).toBeDisabled();
    await expect(page.locator('#booking-request-status')).toContainText('Updating slots');
    await button.evaluate(element => element.click());
    expect(requests).toHaveLength(0);
    release();
    await expect(page.locator('#slots-content')).toHaveClass(/htmx-settling/);
    await expect(button).toBeDisabled();
    await expect(button).toBeEnabled();
    await expect(page.locator('#booking-request-status')).toBeHidden();
    if (action === 'cancel') page.once('dialog', dialog => dialog.accept());
    await button.click();
    await expect(card.getByTestId('booking-status')).toHaveText(action === 'signup' ? 'Confirmed 🏀' : 'Not booked');
    expect(requests).toHaveLength(1);
    if (action === 'signup') {
      page.once('dialog', dialog => dialog.accept());
      await card.getByRole('button', { name: 'Cancel booking' }).click();
      await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
    }
  });
}

test('a failed booking releases the shared controls on every card', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  const otherButton = page.getByTestId('slot-card').nth(1).getByRole('button', { name: 'Sign Up', exact: true });
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/slots/*/signup', async route => {
    await gate;
    await route.fulfill({ status: 503, contentType: 'text/html', body: '<main><p>System busy. Try again.</p></main>' });
  });
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(otherButton).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Refresh slots' })).toBeDisabled();
  await expect(page.locator('#booking-request-status')).toContainText('Saving changes');
  release();
  await expect(card.getByRole('alert')).toContainText('System busy');
  await expect(otherButton).toBeEnabled();
  await expect(card.getByRole('button', { name: 'Sign Up', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
  await expect(page.locator('#booking-request-status')).toBeHidden();
});

test('name selection submits the selected ID and editing clears it', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('combobox').fill('Ben');
  await page.getByRole('option', { name: 'Ben', exact: true }).click();
  const selected = page.locator('#selected-user-id');
  await expect(selected).toHaveValue('2');
  await expect(selected).toBeEnabled();
  await page.getByRole('combobox').fill('Alex');
  await expect(selected).toBeDisabled();
  await expect(selected).toHaveValue('');
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(page.locator('#name-step').getByRole('alert')).toContainText('Choose your name from the list');
  await expect(page.getByTestId('slot-card')).toHaveCount(0);
  await page.getByRole('combobox').focus();
  await page.getByRole('option', { name: 'Alex', exact: true }).click();
  const request = page.waitForRequest(request => request.url().endsWith('/users/select') && request.method() === 'POST');
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  expect(new URLSearchParams((await request).postData()).get('selected_user_id')).toBe('1');
  await expect(page.locator('.identity-name')).toHaveText('Alex');
  expect((await page.context().cookies()).find(cookie => cookie.name === 'user_id').value).toBe('1');
});

test('a failed refresh restores booking controls and shows a visible error', async ({ page }) => {
  await createMember(page);
  let release;
  let captured;
  const gate = new Promise(resolve => { release = resolve; });
  const capture = new Promise(resolve => { captured = resolve; });
  await page.route('**/slots/fragment', async route => {
    captured();
    await gate;
    await route.fulfill({ status: 503, contentType: 'text/html', body: '<main><p>Refresh failed. Try again.</p></main>' });
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await capture;
  const button = page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true });
  await expect(button).toBeDisabled();
  await expect(page.locator('#booking-request-status')).toHaveClass('sr-only');
  await expect(page.locator('#booking-request-status')).toContainText('Updating slots');
  release();
  await expect(page.locator('#global-error-banner')).toContainText('Refresh failed');
  await expect(button).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
  await expect(page.locator('#booking-request-status')).toBeHidden();
  await expect(page.getByTestId('slot-card').first().getByTestId('booking-status')).toHaveText('Not booked');
});

test('background refresh keeps layout and expanded rosters stable through the first frame', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  await card.locator('.roster > summary').click();
  const before = await overviewLayout(page);
  let release;
  let captured;
  const gate = new Promise(resolve => { release = resolve; });
  const capture = new Promise(resolve => { captured = resolve; });
  await page.route('**/slots/fragment', async route => {
    const response = await route.fetch();
    captured();
    await gate;
    await route.fulfill({ response });
  });
  await page.evaluate(() => {
    htmx.config.defaultSettleDelay = 400;
    document.body.addEventListener('htmx:afterSwap', event => {
      if (event.detail.elt.id !== 'slots-content') return;
      requestAnimationFrame(() => {
        const card = document.querySelector('[data-testid="slot-card"]');
        window.refreshFrame = { open: card.querySelector('.roster').open, height: card.getBoundingClientRect().height };
      });
    });
    window.dispatchEvent(new Event('focus'));
  });
  await capture;
  await expect(card.locator('.slot-actions button')).toBeDisabled();
  expect(await overviewLayout(page)).toEqual(before);
  release();
  await expect.poll(() => page.evaluate(() => window.refreshFrame)).toEqual({ open: true, height: before.card.height });
  await expect(card.locator('.slot-actions button')).toBeEnabled();
  expect(await overviewLayout(page)).toEqual(before);
});

test('booking feedback does not resize the card or button', async ({ page }) => {
  await createMember(page);
  const card = page.getByTestId('slot-card').first();
  const before = await overviewLayout(page);
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/slots/*/signup', async route => { await gate; await route.continue(); });
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.locator('.slot-actions button')).toBeDisabled();
  await expect(page.locator('.refresh-icon')).toHaveCSS('animation-name', 'none');
  expect(await overviewLayout(page)).toEqual(before);
  release();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  await expect(card.locator('.slot-actions button')).toBeEnabled();
  expect(await overviewLayout(page)).toEqual(before);
  await expect(page.locator('#announcements')).toContainText('Booking saved');
  page.once('dialog', dialog => dialog.accept());
  await card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  expect(await card.locator('.slot-actions button').evaluate(button => getComputedStyle(button).transitionDuration)).toBe('0s');
});

test('refresh has a CSS activity icon and clear disabled colors without layout shifts', async ({ page }) => {
  await createMember(page);
  const refresh = page.getByRole('button', { name: 'Refresh slots' });
  const icon = refresh.locator('.refresh-icon');
  const signup = page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true });
  const before = await refresh.boundingBox();
  await expect(icon).toBeVisible();
  await expect(icon).toHaveCSS('animation-name', 'none');
  const activeColor = await signup.evaluate(button => getComputedStyle(button).backgroundColor);
  let release;
  let captured;
  const gate = new Promise(resolve => { release = resolve; });
  const capture = new Promise(resolve => { captured = resolve; });
  await page.route('**/slots/fragment', async route => {
    const response = await route.fetch();
    captured();
    await gate;
    await route.fulfill({ response });
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await capture;
  await expect(refresh).toBeDisabled();
  await expect(icon).toHaveCSS('animation-name', 'refresh-spin');
  await expect(signup).toHaveCSS('background-color', 'rgb(220, 227, 222)');
  await expect(signup).toHaveCSS('color', 'rgb(74, 93, 83)');
  expect(await refresh.boundingBox()).toEqual(before);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(icon).toHaveCSS('animation-name', 'none');
  expect(await refresh.boundingBox()).toEqual(before);
  release();
  await expect(refresh).toBeEnabled();
  await expect(signup).toHaveCSS('background-color', activeColor);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await expect(icon).toHaveCSS('animation-name', 'none');
  expect(await refresh.boundingBox()).toEqual(before);
});

for (const action of ['signup', 'cancel']) {
  test(`failed identity refresh cannot ${action} under another member's cookie`, async ({ page, context, request }) => {
    const oldName = await createMember(page);
    const oldId = (await context.cookies()).find(cookie => cookie.name === 'user_id').value;
    const card = page.getByTestId('slot-card').first();
    const slotId = (await card.getAttribute('id')).replace('slot-', '');
    if (action === 'cancel') {
      await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
      await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    }
    await page.route('**/slots/fragment', route => route.fulfill({
      status: 503, contentType: 'text/html', body: '<main><p>Refresh failed. Try again.</p></main>',
    }));
    const otherTab = await context.newPage();
    await otherTab.goto('/');
    await otherTab.getByRole('button', { name: 'Log out' }).click();
    const newName = await createMember(otherTab);
    const newId = (await context.cookies()).find(cookie => cookie.name === 'user_id').value;
    expect(newId).not.toBe(oldId);
    if (action === 'cancel') {
      await otherTab.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true }).click();
      await expect(otherTab.getByTestId('slot-card').first().getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    }
    await page.bringToFront();
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(page.locator('#global-error-banner')).toContainText('Refresh failed');
    await expect(page.locator('.identity-name')).toHaveText(oldName);
    const response = page.waitForResponse(response => response.url().endsWith(`/slots/${slotId}/${action}`));
    if (action === 'cancel') page.once('dialog', dialog => dialog.accept());
    await card.getByRole('button', { name: action === 'signup' ? 'Sign Up' : 'Cancel booking', exact: true }).click();
    const rejected = await response;
    expect(rejected.status()).toBe(409);
    const headers = await rejected.request().allHeaders();
    expect(headers['x-hoopline-user']).toBe(oldId);
    expect(headers.cookie).toContain(`user_id=${newId}`);
    await expect(card.getByRole('alert')).toContainText('Your selected name changed');
    await expect(card.getByTestId('booking-status')).toHaveText(action === 'signup' ? 'Not booked' : 'Confirmed 🏀');
    // Both accounts must be unchanged, not merely the stale card in this tab.
    for (const id of [oldId, newId]) {
      const headers = { Cookie: `user_id=${id}`, 'X-Hoopline-User': id };
      if (action === 'signup') expect((await request.post(`/slots/${slotId}/signup`, { headers })).ok()).toBe(true);
      expect((await request.post(`/slots/${slotId}/cancel`, { headers })).ok()).toBe(true);
    }
    await page.unroute('**/slots/fragment');
    await page.getByRole('button', { name: 'Refresh slots' }).click();
    await expect(page.locator('.identity-name')).toHaveText(newName);
    await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
    await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
    await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    page.once('dialog', dialog => dialog.accept());
    await card.getByRole('button', { name: 'Cancel booking' }).click();
    await expect(card.getByTestId('booking-status')).toHaveText('Not booked');
    await otherTab.close();
  });
}
