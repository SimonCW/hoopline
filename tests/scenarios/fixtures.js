import { test as base, expect } from '@playwright/test';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
import { onListening } from './listening.js';

const exec = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const scenarioNow = '2030-01-07T12:00:00Z';
export const scenarioSlotId = 1001;

export const test = base.extend({
  seedProfile: ['legacy', { option: true }],
  now: [scenarioNow, { option: true }],
  serviceWorkers: ['block', { option: true }],
  baseURL: async ({ app }, use) => { await use(app.baseURL); },
  _networkIsolation: [async ({ context, app }, use) => {
    await app.blockNetwork(context);
    await use();
  }, { auto: true }],
  app: async ({ browser, contextOptions, viewport, isMobile, hasTouch, deviceScaleFactor, userAgent, seedProfile, now }, use, testInfo) => {
    const directory = await mkdtemp(join(process.env.TMPDIR || tmpdir(), 'hoopline-scenario-'));
    const databasePath = join(directory, 'hoopline.sqlite');
    let port = 0;
    let baseURL;
    const logs = [];
    const contexts = [];
    const externalRequests = [];
    let running;
    let generation = 0;
    let currentNow = now;

    async function start() {
      generation += 1;
      logs.push(`\n--- app start ${generation} (port=${port}, now=${currentNow}) ---\n`);
      const child = spawn(join(root, 'target/debug/hoopline'), [], {
        cwd: root,
        env: {
          ...process.env,
          DATABASE_URL: `sqlite://${databasePath}`,
          PORT: String(port),
          ADMIN_PASSWORD: 'admin',
          COOKIE_SECURE: 'false',
          PREVIEW_SAMPLE: '0',
          HOOPLINE_TEST_NOW: currentNow,
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      const state = { child, ended: false, error: undefined };
      state.exit = new Promise(resolve => {
        child.once('error', error => {
          state.error = error;
          state.ended = true;
          logs.push(`spawn error: ${error.message}\n`);
          resolve();
        });
        child.once('exit', (code, signal) => {
          state.ended = true;
          logs.push(`app exit: code=${code}, signal=${signal}\n`);
          resolve();
        });
      });
      child.stdout.on('data', data => logs.push(`[stdout] ${data}`));
      onListening(child.stdout, assignedPort => {
        if (port !== 0 && assignedPort !== port) {
          state.error = new Error(`Restart bound unexpected port ${assignedPort}, expected ${port}`);
        } else {
          port = assignedPort;
          baseURL = `http://127.0.0.1:${port}`;
          state.listening = true;
        }
      });
      child.stderr.on('data', data => logs.push(`[stderr] ${data}`));
      running = state;
      const deadline = Date.now() + 15_000;
      while (Date.now() < deadline) {
        if (state.ended || state.error) {
          throw new Error(`App failed to start. Build with cargo build --locked --features scenario-tests.\n${state.error || logs.join('')}`);
        }
        try {
          if (state.listening) {
            const response = await fetch(`${baseURL}/healthz`, { signal: AbortSignal.timeout(500) });
            if (response.ok) return;
          }
        } catch {
          // The process runs real migrations before it binds its listener.
        }
        await delay(50);
      }
      throw new Error(`App startup timed out.\n${logs.join('')}`);
    }

    async function stop() {
      if (!running || running.ended) return;
      const state = running;
      state.child.kill('SIGTERM');
      const timeout = setTimeout(() => state.child.kill('SIGKILL'), 5_000);
      try {
        await state.exit;
      } finally {
        clearTimeout(timeout);
      }
    }

    async function query(sql, parameters = []) {
      const { stdout } = await exec('python3', ['-c', `
import json, sqlite3, sys
db = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
db.row_factory = sqlite3.Row
print(json.dumps([dict(row) for row in db.execute(sys.argv[2], json.loads(sys.argv[3]))]))
db.close()
`, databasePath, sql, JSON.stringify(parameters)], { timeout: 10_000 });
      return JSON.parse(stdout);
    }

    async function seed(sql, parameters = []) {
      await stop();
      try {
        await exec('python3', ['-c', `
import json, sqlite3, sys
with sqlite3.connect(sys.argv[1]) as db:
    db.execute("PRAGMA foreign_keys = ON")
    db.execute(sys.argv[2], json.loads(sys.argv[3]))
`, databasePath, sql, JSON.stringify(parameters)], { timeout: 10_000 });
      } finally {
        await start();
      }
    }

    async function newActor(name, { admin = false } = {}) {
      const context = await browser.newContext({
        ...contextOptions, viewport, isMobile, hasTouch, deviceScaleFactor, userAgent, baseURL,
        serviceWorkers: 'block',
      });
      contexts.push(context);
      await blockNetwork(context);
      const page = await context.newPage();
      await page.goto('/');
      await page.getByRole('combobox', { name: 'Your name' }).fill(name);
      await page.getByRole('option', { name, exact: true }).click();
      await page.getByRole('button', { name: 'Continue', exact: true }).click();
      await expect(page.locator('.identity-name')).toHaveText(name);
      if (admin) {
        await page.goto('/admin/login');
        await page.getByLabel('Password', { exact: true }).fill('admin');
        await page.getByRole('button', { name: 'Enter admin UI' }).click();
        await expect(page.getByRole('link', { name: 'Edit this game' }).first()).toBeVisible();
      }
      return { name, context, page, card: page.locator(`#slot-${scenarioSlotId}`) };
    }

    async function blockNetwork(context) {
      await context.route('**/*', async route => {
        if (new URL(route.request().url()).origin === baseURL) {
          await route.fallback();
        } else {
          externalRequests.push(route.request().url());
          await route.abort('blockedbyclient');
        }
      });
    }

    try {
      // The production app creates this new file and applies its real migrations.
      await start();
      await stop();
      await exec('python3', [join(root, 'tests/scenarios/seed.py'), databasePath, seedProfile], { timeout: 10_000 });
      await start();
      await use({
        baseURL, databasePath, newActor, query, seed, blockNetwork,
        restart: async (newNow = currentNow) => {
          await stop();
          currentNow = newNow;
          await start();
        },
      });
    } finally {
      const cleanupErrors = [];
      try {
        for (const [index, context] of contexts.entries()) {
          try {
            if (testInfo.status !== testInfo.expectedStatus) {
              for (const [pageIndex, page] of context.pages().entries()) {
                if (!page.isClosed()) {
                  await testInfo.attach(`actor-${index + 1}-page-${pageIndex + 1}`, {
                    body: await page.screenshot({ timeout: 2_000 }), contentType: 'image/png',
                  });
                }
              }
            }
          } catch (error) {
            cleanupErrors.push(error);
          } finally {
            try { await context.close(); } catch (error) { cleanupErrors.push(error); }
          }
        }
      } finally {
        try {
          await stop();
        } finally {
          try {
            await testInfo.attach('server.log', { body: Buffer.from(logs.join('')), contentType: 'text/plain' });
            await testInfo.attach('external-requests.json', {
              body: Buffer.from(JSON.stringify(externalRequests)), contentType: 'application/json',
            });
          } finally {
            await rm(directory, { recursive: true, force: true });
          }
        }
      }
      expect(externalRequests, 'The UI must not depend on external network requests').toEqual([]);
      if (cleanupErrors.length) throw new AggregateError(cleanupErrors, 'Actor cleanup failed');
    }
  },
});

export { expect };
