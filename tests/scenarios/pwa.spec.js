import { test, expect } from './fixtures.js';

test.use({ serviceWorkers: 'allow' });

test('install guidance is dismissible and hidden in standalone mode', async ({ page }) => {
  await page.goto('/');
  const guidance = page.locator('#install-guidance');
  await expect(guidance).toBeVisible();
  await guidance.getByRole('button', { name: 'Continue in browser' }).click();
  await expect(guidance).toBeHidden();
  await page.reload();
  await expect(guidance).toBeHidden();
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'standalone', { value: true, configurable: true });
    sessionStorage.removeItem('hoopline-install-dismissed');
  });
  await page.reload();
  await expect(guidance).toBeHidden();
});

test('explicit alert setup requests permission and confirms registration', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.context.addInitScript(() => {
    const notification = { permission: 'default', requestPermission: async () => {
      window.permissionRequests = (window.permissionRequests || 0) + 1;
      notification.permission = 'granted';
      return 'granted';
    } };
    Object.defineProperty(window, 'Notification', { value: notification, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    const registration = { pushManager: {
      getSubscription: async () => null,
      subscribe: async () => ({ toJSON: () => ({ endpoint: 'stub' }) }),
    } };
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
    });
  });
  let state = 'unregistered';
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state, public_key: 'AQID' },
  }));
  let subscriptions = 0;
  await actor.page.route('**/push/subscribe', route => {
    subscriptions++;
    return route.fulfill({ json: { available: true, state: 'registered', public_key: 'AQID' } });
  });
  await actor.page.reload();
  await expect(actor.page.getByRole('button', { name: 'Enable promotion alerts' })).toBeVisible();
  expect(await actor.page.evaluate(() => window.permissionRequests || 0)).toBe(0);
  await actor.page.getByRole('button', { name: 'Enable promotion alerts' }).click();
  await expect(actor.page.locator('#alert-setup-status')).toHaveText('Promotion alerts are on for this device.');
  expect(subscriptions).toBe(1);
  expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(1);
  state = 'expired';
  await actor.page.evaluate(() => window.HooplinePush.refreshStatus());
  await expect(actor.page.getByRole('button', { name: 'Enable promotion alerts' })).toBeVisible();
  await expect(actor.page.locator('#alert-setup-status')).toBeEmpty();
});

test('stalled alert setup releases booking controls and ignores late preparation', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state: 'unregistered', public_key: 'AQID' },
  }));
  await actor.page.reload();
  await actor.page.evaluate(() => {
    Object.defineProperty(window, 'Notification', { value: { permission: 'granted' }, configurable: true });
    Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
    window.HooplinePush.prepare = () => new Promise(resolve => { window.finishPreparation = resolve; });
  });
  await actor.page.evaluate(() => window.HooplinePush.refreshStatus());
  await actor.page.clock.install();
  let subscriptions = 0;
  await actor.page.route('**/push/subscribe', route => { subscriptions++; return route.abort(); });
  await actor.page.getByRole('button', { name: 'Enable promotion alerts' }).click();
  await expect(actor.page.locator('#refresh-slots')).toBeDisabled();
  await actor.page.clock.fastForward(10001);
  await expect(actor.page.locator('#alert-setup-status')).toHaveText('Alerts could not be enabled. Try again.');
  await expect(actor.page.locator('#refresh-slots')).toBeEnabled();
  await actor.page.evaluate(() => window.finishPreparation('{"endpoint":"stub"}'));
  expect(subscriptions).toBe(0);
});

test('alert setup respects an explicitly disabled device', async ({ app }) => {
  const actor = await app.newActor('Scenario Alice');
  await actor.page.route('**/push/status', route => route.fulfill({
    json: { available: true, state: 'disabled', public_key: 'AQID' },
  }));
  await actor.page.reload();
  await expect(actor.page.locator('#alert-setup')).toBeHidden();
});

test.describe('joining a full game', () => {
  test.use({ seedProfile: 'last-spot' });

  for (const permission of ['granted', 'denied']) {
    test(`requests permission and saves the waitlist booking when ${permission}`, async ({ app }) => {
      const player = await app.newActor('Scenario Bob');
      await player.card.getByRole('button', { name: 'Sign Up', exact: true }).click();
      await expect(player.card.getByTestId('booking-status')).toHaveText('Confirmed 🏀');
      const actor = await app.newActor('Scenario Alice');
      await actor.context.addInitScript(result => {
        const registration = {
          active: {},
          pushManager: {
            getSubscription: async () => null,
            subscribe: async () => ({ toJSON: () => ({ endpoint: 'stub' }) }),
          },
        };
        const notification = { permission: 'default', requestPermission: () => {
          window.permissionRequests = (window.permissionRequests || 0) + 1;
          notification.permission = result;
          return Promise.resolve(result);
        } };
        Object.defineProperty(window, 'Notification', { value: notification, configurable: true });
        Object.defineProperty(window, 'PushManager', { value: class {}, configurable: true });
        Object.defineProperty(navigator, 'serviceWorker', {
          value: { register: async () => registration, ready: Promise.resolve(registration) }, configurable: true,
        });
      }, permission);
      await actor.page.route('**/push/status', route => route.fulfill({
        json: { available: true, state: 'unregistered', public_key: 'AQID' },
      }));
      await actor.page.reload();
      await expect(actor.page.locator('#push-controls')).toBeHidden();
      await expect(actor.page.getByRole('heading', { name: 'Waitlist promotion alerts' })).toBeVisible();
      await actor.card.getByRole('button', { name: 'Join Waitlist' }).click();
      await expect(actor.card.getByTestId('booking-status')).toContainText('Waitlist');
      expect(await actor.page.evaluate(() => window.permissionRequests)).toBe(1);
      if (permission === 'denied') {
        await expect(actor.card.locator('[data-feedback]')).toContainText('Alerts could not be enabled');
        await expect(actor.card.locator('[data-feedback]')).toBeVisible();
      }
    });
  }
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
