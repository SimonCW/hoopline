import { test, expect, scenarioNow, scenarioSlotId } from './fixtures.js';

test.describe('Sunday release boundary', () => {
  test.use({ seedProfile: 'release', now: '2030-01-06T17:59:59Z' });

  test('the next game week opens at exactly 19:00 Berlin time', async ({ app }) => {
    const member = await app.newActor('Scenario Alice');
    const occurrences = () => app.query(
      'SELECT occurrence_date FROM slots WHERE schedule_id = 1001 ORDER BY occurrence_date',
    );
    expect(await occurrences()).toEqual([{ occurrence_date: '2030-01-07' }]);
    await expect(member.page.getByTestId('slot-card')).toHaveCount(1);

    // Restart at the same instant. Generation must remain idempotent before release.
    await app.restart('2030-01-06T17:59:59Z');
    await member.page.reload();
    expect(await occurrences()).toEqual([{ occurrence_date: '2030-01-07' }]);
    await expect(member.page.getByTestId('slot-card')).toHaveCount(1);

    // January uses CET: 18:00 UTC is Sunday 19:00 in Berlin.
    await app.restart('2030-01-06T18:00:00Z');
    await member.page.reload();
    const released = [
      { occurrence_date: '2030-01-07' },
      { occurrence_date: '2030-01-14' },
    ];
    expect(await occurrences()).toEqual(released);
    await expect(member.page.getByTestId('slot-card')).toHaveCount(2);
    const newGame = (await app.query(
      "SELECT id, datetime FROM slots WHERE schedule_id = 1001 AND occurrence_date = '2030-01-14'",
    ))[0];
    expect(newGame.datetime).toBe('2030-01-14T19:00:00Z');
    await expect(member.page.locator(`#slot-${newGame.id}`).getByRole('button', { name: 'Sign Up', exact: true })).toBeEnabled();

    await app.restart('2030-01-06T18:00:00Z');
    await member.page.reload();
    expect(await occurrences()).toEqual(released);
    await expect(member.page.getByTestId('slot-card')).toHaveCount(2);
  });
});

test.describe('admin session boundary', () => {
  test.use({ seedProfile: 'last-spot', now: scenarioNow });

  test('the real login cookie works before 12 hours and fails at expiry', async ({ app }) => {
    const admin = await app.newActor('Scenario Admin', { admin: true });
    const session = (await admin.context.cookies()).find(cookie => cookie.name === 'admin_session');
    expect(session).toBeDefined();
    expect(session.httpOnly).toBe(true);
    const expiresAt = Date.parse(scenarioNow) / 1000 + 12 * 60 * 60;
    expect(await app.query('SELECT expires_at FROM admin_sessions WHERE token = ?', [session.value]))
      .toEqual([{ expires_at: expiresAt }]);
    const editURL = `/admin/slots/${scenarioSlotId}/edit`;

    await app.restart('2030-01-07T23:59:59Z');
    const before = await admin.page.goto(editURL);
    expect(before.status()).toBe(200);
    await expect(admin.page.getByRole('heading', { name: 'Edit this game', exact: true })).toBeVisible();
    expect((await admin.context.cookies()).find(cookie => cookie.name === 'admin_session').value).toBe(session.value);

    await app.restart('2030-01-08T00:00:00Z');
    const at = await admin.page.goto(editURL);
    expect(at.status()).toBe(403);
    await expect(admin.page.getByText('admin access required', { exact: true })).toBeVisible();
    // Retain the actual cookie. Server-side expiry, not browser deletion, rejects it.
    expect((await admin.context.cookies()).find(cookie => cookie.name === 'admin_session').value).toBe(session.value);
    await admin.page.goto('/slots?mode=admin');
    await expect(admin.page.locator('.identity-name')).toHaveText(admin.name);
    await expect(admin.page.getByRole('link', { name: 'Edit this game' })).toHaveCount(0);
    await expect(admin.card.getByRole('button', { name: 'Sign Up', exact: true })).toBeVisible();
  });

  test('login failures block before 900 seconds and expire at exactly 900 seconds', async ({ app }) => {
    const attemptedAt = Date.parse(scenarioNow) / 1000;
    await app.seed(
      `INSERT INTO admin_login_failures (user_id, attempted_at)
       VALUES (1003, ?), (1003, ?), (1003, ?), (1003, ?), (1003, ?)`,
      Array(5).fill(attemptedAt),
    );
    const admin = await app.newActor('Scenario Admin');
    await app.restart('2030-01-07T12:14:59Z');
    await admin.page.goto('/admin/login');
    await admin.page.getByLabel('Password', { exact: true }).fill('admin');
    await admin.page.getByRole('button', { name: 'Enter admin UI' }).click();
    await expect(admin.page.getByRole('alert')).toHaveText('Too many attempts for this member. Try again in 15 minutes.');
    expect(await app.query('SELECT count(*) AS count FROM admin_login_failures WHERE user_id = 1003'))
      .toEqual([{ count: 5 }]);
    expect((await admin.context.cookies()).find(cookie => cookie.name === 'admin_session')).toBeUndefined();

    await app.restart('2030-01-07T12:15:00Z');
    await admin.page.getByLabel('Password', { exact: true }).fill('admin');
    await admin.page.getByRole('button', { name: 'Enter admin UI' }).click();
    await expect(admin.page.getByRole('link', { name: 'Edit this game' })).toBeVisible();
    expect(await app.query('SELECT count(*) AS count FROM admin_login_failures WHERE user_id = 1003'))
      .toEqual([{ count: 0 }]);
    expect((await admin.context.cookies()).find(cookie => cookie.name === 'admin_session')).toBeDefined();
  });
});

test.describe('game start boundary and RFC3339 offset', () => {
  test.use({ seedProfile: 'last-spot', now: '2030-01-08T18:59:59+01:00' });

  test('signup works one second before start and a stale form fails exactly at start', async ({ app }) => {
    const member = await app.newActor('Scenario Alice');
    await member.card.getByRole('button', { name: 'Sign Up', exact: true }).click();
    await expect(member.card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    await expect(member.page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
    member.page.once('dialog', dialog => dialog.accept());
    await member.card.getByRole('button', { name: 'Cancel booking' }).click();
    await expect(member.card.getByTestId('booking-status')).toHaveText('Not booked');
    await expect(member.card.getByRole('button', { name: 'Sign Up', exact: true })).toBeEnabled();

    await app.restart('2030-01-08T19:00:00+01:00');
    const response = member.page.waitForResponse(response =>
      new URL(response.url()).pathname === `/slots/${scenarioSlotId}/signup`,
    );
    await member.card.getByRole('button', { name: 'Sign Up', exact: true }).click();
    expect((await response).status()).toBe(400);
    await expect(member.card.getByRole('alert')).toContainText('This slot has started');
    expect(await app.query('SELECT count(*) AS count FROM bookings WHERE slot_id = ? AND user_id = 1001', [scenarioSlotId]))
      .toEqual([{ count: 0 }]);
    await member.page.reload();
    await expect(member.card.getByText('This game has started. Signups are closed.', { exact: true })).toBeVisible();
    await expect(member.card.getByRole('button', { name: 'Sign Up', exact: true })).toHaveCount(0);
  });
});
