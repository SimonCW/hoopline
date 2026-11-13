import { test, expect, scenarioSlotId } from './fixtures.js';
import { setTimeout as delay } from 'node:timers/promises';

test.use({ seedProfile: 'last-spot' });

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

async function bounded(promise, message) {
  const abort = new AbortController();
  try {
    return await Promise.race([
      promise,
      delay(10_000, undefined, { signal: abort.signal }).then(() => { throw new Error(message); }),
    ]);
  } finally {
    abort.abort();
  }
}

async function refresh(actor, { focus = false } = {}) {
  const response = actor.page.waitForResponse(response => new URL(response.url()).pathname === '/slots/fragment');
  if (focus) {
    await actor.page.evaluate(() => window.dispatchEvent(new Event('focus')));
  } else {
    await actor.page.getByRole('button', { name: 'Refresh slots' }).click();
  }
  expect((await response).ok()).toBe(true);
  await expect(actor.page.locator('#slots-content')).not.toHaveClass(/htmx-settling/);
  await expect(actor.page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
}

async function expectRoster(actor, rows) {
  await expect(actor.card.getByTestId('player-row').locator('.member-name')).toHaveText(
    rows.filter(row => row.is_waitlist === 0).map(row => row.name),
  );
  await expect(actor.card.getByTestId('waitlist-row').locator('.member-name')).toHaveText(
    rows.filter(row => row.is_waitlist === 1).map(row => row.name),
  );
}

test('two players contest the last spot, cancellation promotes the waiter, and restart keeps the result', async ({ app }, testInfo) => {
  const alice = await app.newActor('Scenario Alice');
  const bob = await app.newActor('Scenario Bob');
  expect(alice.context).not.toBe(bob.context);
  const actors = [alice, bob];
  const release = deferred();
  const arrivals = actors.map(() => deferred());
  const signupRequests = [0, 0];
  const handlers = actors.map(() => deferred());

  for (const [index, actor] of actors.entries()) {
    await expect(actor.card.getByTestId('booking-status')).toHaveText('Not booked');
    await expect(actor.card.locator('.availability')).toContainText('14/15');
    await actor.card.locator('.roster > summary').click();
    await expect(actor.card.getByTestId('player-row')).toHaveCount(14);
    await actor.page.route(`**/slots/${scenarioSlotId}/signup`, async route => {
      try {
        expect(route.request().method()).toBe('POST');
        signupRequests[index] += 1;
        arrivals[index].resolve();
        await release.promise;
        // Forward the original request. Do not mock any server response.
        await route.continue();
      } finally {
        handlers[index].resolve();
      }
    });
  }

  try {
    await Promise.all(actors.map(actor => actor.card.getByRole('button', { name: 'Sign Up', exact: true }).click()));
    await Promise.all(arrivals.map((arrival, index) =>
      bounded(arrival.promise, `${actors[index].name} did not submit a signup POST`)));
    for (const actor of actors) {
      await expect(actor.card).toHaveAttribute('aria-busy', 'true');
      await expect(actor.card.getByRole('button', { name: 'Sign Up', exact: true })).toBeDisabled();
      await expect(actor.card.getByTestId('booking-status')).toHaveText('Not booked');
    }
    // Neither request reached the server before both browsers had submitted.
    expect(await app.query('SELECT COUNT(*) AS count FROM bookings WHERE slot_id = ?', [scenarioSlotId]))
      .toEqual([{ count: 14 }]);
  } finally {
    release.resolve();
    try {
      await Promise.all(handlers.map((handler, index) => signupRequests[index]
        ? bounded(handler.promise, `${actors[index].name} signup handler did not finish`) : Promise.resolve()));
    } finally {
      await Promise.all(actors.map(actor => actor.page.unroute(`**/slots/${scenarioSlotId}/signup`)));
    }
  }

  await expect.poll(async () => Promise.all(actors.map(actor =>
    actor.card.getByTestId('booking-status').textContent()))).toEqual(
    expect.arrayContaining(['Confirmed 🏀', 'Waitlist #1']),
  );
  expect(signupRequests).toEqual([1, 1]);
  const winner = (await alice.card.getByTestId('booking-status').textContent()) === 'Confirmed 🏀' ? alice : bob;
  const waiter = winner === alice ? bob : alice;
  await expect(waiter.card.locator('[data-feedback]')).toContainText('Waitlist #1');
  const roster = () => app.query(
    `SELECT users.name, bookings.position, bookings.is_waitlist
     FROM bookings JOIN users ON users.id = bookings.user_id
     WHERE slot_id = ? ORDER BY is_waitlist, position`,
    [scenarioSlotId],
  );
  const contested = await roster();
  expect(contested.filter(row => row.is_waitlist === 0)).toHaveLength(15);
  expect(contested.filter(row => row.is_waitlist === 1)).toEqual([
    { name: waiter.name, position: 1, is_waitlist: 1 },
  ]);

  // Refresh both real views. They must agree about the player and waitlist.
  for (const actor of actors) {
    await expect(actor.page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
    await refresh(actor);
    await expectRoster(actor, contested);
  }

  winner.page.once('dialog', dialog => dialog.accept());
  await winner.card.getByRole('button', { name: 'Cancel booking' }).click();
  await expect(winner.card.getByTestId('booking-status')).toHaveText('Not booked');
  await refresh(waiter, { focus: true });
  await expect(waiter.card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  await expect(waiter.card.getByTestId('player-row').filter({ hasText: waiter.name })).toHaveClass('current-player');
  await expect(waiter.card.getByTestId('waitlist-row')).toHaveCount(0);
  const promoted = await roster();
  expect(promoted).toHaveLength(15);
  expect(promoted.every(row => row.is_waitlist === 0)).toBe(true);
  expect(promoted.some(row => row.name === winner.name)).toBe(false);
  expect(promoted.find(row => row.name === waiter.name)).toEqual({
    name: waiter.name, position: 15, is_waitlist: 0,
  });
  await expectRoster(waiter, promoted);
  await expectRoster(winner, promoted);

  await app.restart();
  for (const actor of actors) {
    await actor.page.reload();
    await expect(actor.page.locator('.identity-name')).toHaveText(actor.name);
    await expect(actor.card.getByTestId('booking-status')).toHaveText(actor === waiter ? 'Confirmed 🏀' : 'Not booked');
    await actor.card.locator('.roster > summary').click();
    await expectRoster(actor, promoted);
  }
  expect(await roster()).toEqual(promoted);

  // Cover the full-game action as well as the stale "Sign Up" race.
  await winner.card.getByRole('button', { name: 'Join Waitlist', exact: true }).click();
  await expect(winner.card.getByTestId('booking-status')).toHaveText('Waitlist #1');
  await expect(winner.card.locator('[data-feedback]')).toContainText('Waitlist #1');
  const rejoined = [...promoted, { name: winner.name, position: 1, is_waitlist: 1 }];
  expect(await roster()).toEqual(rejoined);
  await expectRoster(winner, rejoined);
  await refresh(waiter);
  await expectRoster(waiter, rejoined);
  await testInfo.attach('booking-transitions.json', {
    body: Buffer.from(JSON.stringify({ winner: winner.name, waiter: waiter.name, contested, promoted, rejoined }, null, 2)),
    contentType: 'application/json',
  });
});
