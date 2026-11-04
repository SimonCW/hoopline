import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/browser',
  testMatch: '**/admin.spec.js',
  workers: 1,
  forbidOnly: !!process.env.CI,
  use: { baseURL: 'http://127.0.0.1:5052', trace: 'retain-on-failure' },
  projects: [
    { name: 'mobile', use: { ...devices['Pixel 7'] } },
    { name: 'desktop', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: {
    command: 'cargo run --locked',
    env: { DATABASE_URL: 'sqlite::memory:', PORT: '5052', ADMIN_PASSWORD: 'admin' },
    url: 'http://127.0.0.1:5052/healthz',
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
