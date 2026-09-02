import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './storybook-e2e',
  outputDir: './test-results/storybook',
  snapshotDir: './storybook-e2e/__screenshots__',
  snapshotPathTemplate: '{snapshotDir}/{testFilePath}/{arg}{ext}',
  use: { baseURL: 'http://127.0.0.1:6006', ...devices['Desktop Chrome'] },
  webServer: {
    command: 'npm run storybook -- --ci --port 6006',
    url: 'http://127.0.0.1:6006',
    reuseExistingServer: false,
    timeout: 120_000,
  },
})
