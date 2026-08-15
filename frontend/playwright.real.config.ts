import { defineConfig, devices } from '@playwright/test'

const baseURL = process.env.REAL_COMPOSITION_BASE_URL
if (!baseURL) throw new Error('REAL_COMPOSITION_BASE_URL is required')

export default defineConfig({
  testDir: './e2e',
  testMatch: 'real-composition.spec.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: 'list',
  timeout: 90_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL,
    acceptDownloads: true,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium-real', use: { ...devices['Desktop Chrome'] } }],
})
