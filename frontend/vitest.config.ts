import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

// Unit tests for the store's pure logic. happy-dom supplies localStorage and
// document so the preset/theme helpers run as they would in the browser.
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'happy-dom',
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test/setup.ts'],
  },
})
