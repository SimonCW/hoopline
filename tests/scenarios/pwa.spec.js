import { test, expect, scenarioPushOrigin, scenarioPushPublicKey } from './fixtures.js';

test.use({ serviceWorkers: 'allow' });

test('the welcome page does not ask users to install or enable alerts', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Your name' })).toBeVisible();
  await expect(page.getByRole('heading', { name: /install Hoopline|promotion alerts/i })).toHaveCount(0);
  await expect(page.getByText('On iPhone or iPad,', { exact: false })).toBeHidden();
});

async function stubPush(actor, { state = 'unregistered', permission = 'default', real = false, stall = false } = {}) {
  await actor.context.addInitScript(({ initialPermission, stall, publicKey }) => {
    window.permissionRequests = 0;
    const notification = { permission: initialPermission, requestPermission: () => {
      window.permissionRequests++;
      return new Promise(resolve => {
        window.answerPermission = answer => {
          notification.permission = answer;
          resolve(answer);
        };
      });
    } };
    Object.defineProperty(window, 'Notification', { value: notification, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    const registration = { pushManager: {
      getSubscription: async () => null,
      subscribe: async () => {
        if (stall) await new Promise(resolve => { window.finishSubscription = resolve; });
        return { toJSON: () => ({
          endpoint: 'https://fcm.googleapis.com/scenario-device',
          keys: { p256dh: publicKey, auth: 'AAAAAAAAAAAAAAAAAAAAAA' },
        }) };
      },
    } };
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  }, { initialPermission: permission, stall, publicKey: scenarioPushPublicKey });
  const subscriptions = [];
  // The scenario listener uses HTTP. Forward the configured HTTPS Origin to
  // the real handler so it can validate these local test writes.
  const forward = async route => route.fulfill({ response: await route.fetch({
    headers: { ...await route.request().allHeaders(), origin: scenarioPushOrigin },
  }) });
  if (real) {
    await actor.page.route('**/slots/*/signup', forward);
  } else {
    await actor.page.route('**/push/status', route => route.fulfill({
      json: { available: true, state, public_key: 'AQID' },
    }));
  }
  await actor.page.route(/\/push\/subscribe(\?.*)?$/, route => {
    subscriptions.push(new URL(route.request().url()).search);
    if (real) return forward(route);
    state = 'registered';
    return route.fulfill({ json: { available: true, state: 'registered', public_key: 'AQID' } });
  });
  await actor.page.reload();
  await actor.page.evaluate(() => window.HooplinePush.refreshStatus());
  return subscriptions;
}

test.describe('joining a full game', () => {
  test.use({ seedProfile: 'last-spot', pushEnabled: true });

  async function fillGame(app) {
    const player = await app.newActor('Scenario Bob');
    await player.card.getByRole('button', { name: 'Sign Up', exact: true }).click();
    await expect(player.card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    return player;
  }

  test('shows the waitlist alert controls and denied permission help in German', async ({ app }) => {
    await fillGame(app);
    const actor = await app.newActor('Scenario Alice');
    await actor.page.getByRole('button', { name: 'Deutsch' }).click();
    await stubPush(actor);
    await expect(actor.card.locator('[data-alert-hint]')).toHaveText('Erlaube Benachrichtigungen, um vom Nachrücken zu erfahren.');
    await actor.card.getByRole('button', { name: 'Auf die Warteliste' }).click();
    await expect(actor.card.getByTestId('booking-status')).toHaveText('Warteliste: Platz 1');
    await actor.page.evaluate(() => window.answerPermission('denied'));
    await actor.card.locator('[data-waitlist-alerts] summary').click();
    await expect(actor.card.locator('[data-alert-help]')).toHaveText('Erlaube Benachrichtigungen in den Einstellungen deines Browsers oder Geräts.');
  });

  for (const permission of ['granted', 'denied']) {
    test(`saves the booking before the user answers ${permission}`, async ({ app }) => {
      await fillGame(app);
      const actor = await app.newActor('Scenario Alice');
      const subscriptions = await stubPush(actor);
      const bell = actor.card.locator('[data-waitlist-alerts]');
      await expect(bell).toHaveCount(0);
      await expect(actor.card.getByText('Allow notifications to get an alert if you move up.')).toBeVisible();
      expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(0);
      await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
      await expect(actor.card.getByTestId('booking-status')).toContainText('Waitlist');
      expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(1);
      await expect(bell).toBeVisible();
      expect(subscriptions).toEqual([]);
      await actor.page.evaluate(result => window.answerPermission(result), permission);
      if (permission === 'granted') {
        await expect.poll(() => subscriptions).toEqual(['?automatic=true']);
        await expect(bell).toBeHidden();
      } else {
        await bell.locator('summary').click();
        await expect(bell).toContainText('Allow notifications in your browser or device settings.');
        await expect(bell.getByRole('button')).toBeHidden();
        expect(subscriptions).toEqual([]);
      }
    });
  }

  test('an off device does not prompt on join and the bell allows explicit enable', async ({ app }) => {
    await fillGame(app);
    const actor = await app.newActor('Scenario Alice');
    const subscriptions = await stubPush(actor, { state: 'disabled' });
    await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
    const bell = actor.card.locator('[data-waitlist-alerts]');
    await expect(bell).toBeVisible();
    expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(0);
    await bell.locator('summary').click();
    const enable = bell.getByRole('button', { name: 'Notify me if I move up' });
    await enable.focus();
    await actor.page.evaluate(() => new Promise(resolve => {
      document.body.addEventListener('htmx:afterSettle', function settled(event) {
        if (event.detail.elt.id !== 'slots-content') return;
        document.body.removeEventListener('htmx:afterSettle', settled);
        resolve();
      });
      window.dispatchEvent(new Event('focus'));
    }));
    await expect(enable).toBeFocused();
    await enable.press('Enter');
    await actor.page.evaluate(() => window.answerPermission('granted'));
    await expect.poll(() => subscriptions).toEqual(['']);
    await expect(bell).toBeHidden();
  });

  test('dismissing permission suppresses another automatic prompt after reload', async ({ app }) => {
    await fillGame(app);
    const actor = await app.newActor('Scenario Alice');
    await stubPush(actor);
    const join = actor.card.getByRole('button', { name: 'Join Waitlist' });
    await join.click();
    await expect(actor.card.getByTestId('booking-status')).toContainText('Waitlist');
    await actor.page.evaluate(() => window.answerPermission('default'));
    actor.page.once('dialog', dialog => dialog.accept());
    await actor.card.getByRole('button', { name: 'Cancel booking' }).click();
    await expect(join).toBeVisible();
    await actor.page.reload();
    await actor.page.evaluate(() => window.HooplinePush.refreshStatus());
    await join.click();
    await expect(actor.card.getByTestId('booking-status')).toContainText('Waitlist');
    expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(0);
  });

  test('a stale Join Waitlist does not enroll a confirmed player', async ({ app }) => {
    const player = await fillGame(app);
    const actor = await app.newActor('Scenario Alice');
    const subscriptions = await stubPush(actor);
    player.page.once('dialog', dialog => dialog.accept());
    await player.card.getByRole('button', { name: 'Cancel booking' }).click();
    await expect(player.card.getByRole('button', { name: 'Sign Up', exact: true })).toBeVisible();
    await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
    await expect(actor.card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
    await actor.page.evaluate(() => window.answerPermission('granted'));
    await expect(actor.card.locator('[data-waitlist-alerts]')).toHaveCount(0);
    expect(subscriptions).toEqual([]);
  });

  test('the first join can prompt before push status responds', async ({ app }) => {
    await fillGame(app);
    const actor = await app.newActor('Scenario Alice');
    await stubPush(actor);
    let releaseStatus;
    const gate = new Promise(resolve => { releaseStatus = resolve; });
    await actor.page.route('**/push/status', async route => {
      await gate;
      return route.fallback();
    });
    try {
      await actor.page.reload();
      await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
      await expect(actor.card.getByTestId('booking-status')).toContainText('Waitlist');
      expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(1);
      await actor.page.evaluate(() => window.answerPermission('denied'));
    } finally {
      releaseStatus();
    }
  });

  for (const stall of [false, true]) {
    test(`granted permission registers quietly ${stall ? 'after a slow setup' : 'with the booking'}`, async ({ app }) => {
      await fillGame(app);
      const actor = await app.newActor('Scenario Alice');
      const subscriptions = await stubPush(actor, { real: true, permission: 'granted', stall });
      await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
      await expect(actor.card.getByTestId('booking-status')).toContainText('Waitlist');
      if (stall) {
        await expect(actor.card.locator('[data-waitlist-alerts]')).toBeVisible();
        await actor.page.evaluate(() => window.finishSubscription());
      }
      await expect.poll(() => app.query('SELECT user_id FROM push_devices WHERE endpoint IS NOT NULL'))
        .toEqual([{ user_id: 1001 }]);
      await expect(actor.card.locator('[data-waitlist-alerts]')).toBeHidden();
      expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(0);
      expect(subscriptions).toEqual(stall ? ['?automatic=true'] : []);
    });
  }

  test('a stalled recovery times out without blocking booking controls', async ({ app }) => {
    await fillGame(app);
    const actor = await app.newActor('Scenario Alice');
    const subscriptions = await stubPush(actor, { state: 'disabled', permission: 'granted', stall: true });
    await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
    const bell = actor.card.locator('[data-waitlist-alerts]');
    await bell.locator('summary').click();
    await actor.page.clock.install();
    await bell.getByRole('button', { name: 'Notify me if I move up' }).click();
    await expect.poll(() => actor.page.evaluate(() => typeof window.finishSubscription)).toBe('function');
    await expect(actor.card.getByRole('button', { name: 'Cancel booking' })).toBeEnabled();
    await actor.page.clock.fastForward(10001);
    await expect(bell.getByRole('status')).toHaveText('Alerts could not be enabled. Try again.');
    await actor.page.evaluate(() => window.finishSubscription());
    expect(subscriptions).toEqual([]);
  });
});

test('the online-only PWA registers its local worker and serves its manifest', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  const result = await actor.page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready;
    const manifestResponse = await fetch('/manifest.webmanifest');
    const manifest = await manifestResponse.json();
    const caches = await window.caches.keys();
    return {
      scope: registration.scope,
      worker: registration.active.scriptURL,
      contentType: manifestResponse.headers.get('content-type'),
      cacheControl: manifestResponse.headers.get('cache-control'),
      manifest,
      caches,
    };
  });
  expect(result.scope).toBe(`${app.baseURL}/`);
  expect(result.worker).toBe(`${app.baseURL}/sw.js`);
  expect(result.contentType).toContain('application/manifest+json');
  expect(result.cacheControl).toBe('no-store');
  expect(result.manifest).toMatchObject({ id: '/', start_url: '/', scope: '/', display: 'standalone' });
  expect(result.caches).toEqual([]);
  for (const icon of result.manifest.icons) {
    const response = await actor.page.request.get(icon.src);
    expect(response.ok()).toBeTruthy();
    expect(response.headers()['content-type']).toBe('image/png');
  }
});

test('restored pages still submit one booking after granted-permission preparation', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.context.addInitScript(() => {
    const registration = {
      active: {},
      pushManager: {
        getSubscription: async () => null,
        subscribe: async () => ({ toJSON: () => ({ endpoint: 'https://fcm.googleapis.com/test', keys: { p256dh: 'stub', auth: 'stub' } }) }),
      },
    };
    Object.defineProperty(window, 'Notification', { value: { permission: 'granted' }, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  });
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state: 'unregistered', public_key: 'AQID' },
  }));
  await actor.page.reload();
  await expect(actor.page.locator('#push-controls')).toBeHidden();
  await actor.page.evaluate(() => {
    window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }));
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  });
  const card = actor.page.getByTestId('slot-card').first();
  await expect(actor.page.getByRole('button', { name: 'Refresh slots' })).toBeEnabled();
  let requests = 0;
  let body = '';
  await actor.page.route('**/slots/*/signup', async route => {
    requests += 1;
    body = route.request().postData();
    await route.continue();
  });
  await card.getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  expect(requests).toBe(1);
  expect(new URLSearchParams(body).get('push_subscription')).toContain('fcm.googleapis.com');
});

test('booking preparation holds the shared request lock', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.context.addInitScript(() => {
    const registration = {
      active: {},
      pushManager: {
        getSubscription: () => new Promise(resolve => { window.finishPreparation = () => resolve(null); }),
        subscribe: async () => ({ toJSON: () => ({ endpoint: 'stub' }) }),
      },
    };
    Object.defineProperty(window, 'Notification', { value: { permission: 'granted' }, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  });
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state: 'unregistered', public_key: 'AQID' },
  }));
  await actor.page.reload();
  await actor.page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect.poll(() => actor.page.evaluate(() => typeof window.finishPreparation)).toBe('function');
  await expect(actor.page.getByRole('button', { name: 'Log out' })).toBeDisabled();
  await expect(actor.page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true })).toBeDisabled();
  await actor.page.evaluate(() => window.finishPreparation());
  await expect(actor.page.getByRole('button', { name: 'Log out' })).toBeEnabled();
});

test('identity refresh replaces the device status and displayed member', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.context.addInitScript(() => {
    const registration = { active: {}, pushManager: { getSubscription: async () => ({}) } };
    Object.defineProperty(window, 'Notification', { value: { permission: 'granted' }, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  });
  let state = 'registered';
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state, public_key: 'AQID' },
  }));
  await actor.page.reload();
  await expect(actor.page.locator('#push-controls')).toHaveAttribute('data-user-id', '1001');
  state = 'unregistered';
  await actor.context.addCookies([{ name: 'user_id', value: '1002', url: app.baseURL }]);
  await actor.page.getByRole('button', { name: 'Refresh slots' }).click();
  await expect(actor.page.locator('.identity-name')).toHaveText('Scenario Bob');
  await expect(actor.page.locator('#push-controls')).toHaveAttribute('data-user-id', '1002');
  await expect(actor.page.locator('#push-controls')).toBeHidden();
});

test('same-member refresh detects an expired endpoint before the next signup', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.context.addInitScript(() => {
    const old = { unsubscribe: async () => { window.unsubscribed = true; return true; } };
    const registration = {
      active: {},
      pushManager: {
        getSubscription: async () => old,
        subscribe: async () => ({ toJSON: () => ({ endpoint: 'new-endpoint' }) }),
      },
    };
    Object.defineProperty(window, 'Notification', { value: { permission: 'granted' }, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  });
  let state = 'registered';
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state, public_key: 'AQID' },
  }));
  await actor.page.reload();
  await expect(actor.page.locator('#push-controls')).toBeHidden();
  state = 'expired';
  const refreshed = actor.page.waitForResponse('**/push/status');
  await actor.page.getByRole('button', { name: 'Refresh slots' }).click();
  await refreshed;
  await actor.page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect(actor.page.getByTestId('slot-card').first().getByTestId('booking-status')).toHaveText('Confirmed 🏀');
  expect(await actor.page.evaluate(() => window.unsubscribed)).toBe(true);
});

test('restoring during preparation drains the queued roster refresh without booking', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.context.addInitScript(() => {
    const registration = {
      active: {},
      pushManager: {
        getSubscription: () => new Promise(resolve => { window.finishPreparation = () => resolve(null); }),
        subscribe: async () => ({ toJSON: () => ({ endpoint: 'stub' }) }),
      },
    };
    Object.defineProperty(window, 'Notification', { value: { permission: 'granted' }, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  });
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state: 'unregistered', public_key: 'AQID' },
  }));
  await actor.page.reload();
  await expect(actor.page.locator('#push-controls')).toBeHidden();
  let signups = 0;
  let refreshes = 0;
  await actor.page.route('**/slots/*/signup', async route => { signups += 1; await route.continue(); });
  await actor.page.route('**/slots/fragment*', async route => { refreshes += 1; await route.continue(); });
  await actor.page.getByTestId('slot-card').first().getByRole('button', { name: 'Sign Up', exact: true }).click();
  await expect.poll(() => actor.page.evaluate(() => typeof window.finishPreparation)).toBe('function');
  await actor.page.evaluate(() => {
    window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }));
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    window.finishPreparation();
  });
  await expect.poll(() => refreshes).toBe(1);
  expect(signups).toBe(0);
});
