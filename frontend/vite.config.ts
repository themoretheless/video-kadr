import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// In dev, proxy API and media requests to the Rust backend so the browser talks
// to a single origin (no CORS dance).
export default defineConfig({
  plugins: [vue()],
  server: {
    port: 5173,
    proxy: {
      // Use 127.0.0.1 (not "localhost"): the backend binds to IPv4, while
      // "localhost" can resolve to IPv6 ::1 first and make the proxy 500.
      '/api': 'http://127.0.0.1:8080',
      '/files': 'http://127.0.0.1:8080',
    },
  },
})
