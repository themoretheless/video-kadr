import { defineConfig, devices } from '@playwright/test'

const runFirefox =
  Boolean(process.env.CI) ||
  process.platform !== 'darwin' ||
  process.env.PLAYWRIGHT_FIREFOX === '1'

function workspacePort(seed: string): number {
  let hash = 2_166_136_261
  for (const character of seed) {
    hash ^= character.charCodeAt(0)
    hash = Math.imul(hash, 16_777_619)
  }
  return 42_000 + ((hash >>> 0) % 10_000)
}

const port = Number(process.env.PLAYWRIGHT_PORT ?? workspacePort(process.cwd()))
if (!Number.isInteger(port) || port < 1 || port > 65_535) {
  throw new Error('PLAYWRIGHT_PORT must be an integer from 1 to 65535')
}
const baseURL = `http://127.0.0.1:${port}`

export default defineConfig({
  testDir: './e2e',
  // ffmpeg.wasm and browser quota/OPFS tests intentionally exercise global
  // per-origin resources; one local worker avoids synthetic cross-test OOM.
  fullyParallel: false,
  workers: process.env.CI ? 2 : 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL,
    trace: 'retain-on-failure',
  },
  webServer: {
    command: `./node_modules/.bin/vite --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    ...(runFirefox ? [{ name: 'firefox', use: { ...devices['Desktop Firefox'] } }] : []),
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
})
