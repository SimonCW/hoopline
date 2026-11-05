import { defineConfig, devices } from '@playwright/test';

const port = process.env.PLAYWRIGHT_PORT || '5051';

export default defineConfig({
  testDir: './tests/browser',
  testIgnore: '**/admin.spec.js',
  // These tests share a disposable in-memory server. Keep mutations sequential.
  workers: 1,
  forbidOnly: !!process.env.CI,
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'mobile', use: { ...devices['Pixel 7'] } },
    { name: 'desktop', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: {
    command: 'cargo run --locked',
    env: { DATABASE_URL: 'sqlite::memory:', PORT: port },
    url: `http://127.0.0.1:${port}/healthz`,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
