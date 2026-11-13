import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/scenarios',
  fullyParallel: true,
  workers: 4,
  forbidOnly: !!process.env.CI,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  retries: 0,
  outputDir: 'test-results/scenarios',
  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'] } },
    { name: 'mobile', use: { ...devices['Pixel 7'] } },
  ],
  // Each test starts its own production binary. Do not use a shared webServer.
});
