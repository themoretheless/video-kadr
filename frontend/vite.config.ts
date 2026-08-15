import { fileURLToPath, URL } from 'node:url'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [svelte({
    compilerOptions: {
      discloseVersion: false,
      // Five base-36 hash characters are ample for this bounded component set
      // and keep scoped selectors out of the product byte budget's hot path.
      cssHash: ({ css, hash }) => `s-${hash(css).slice(0, 5)}`,
    },
  })],
  // These names are private implementation details of the two local recorder
  // controllers. Property mangling is deliberately allow-listed; wire fields,
  // DOM APIs, component props, and persisted document keys are never matched.
  esbuild: {
    // HTML and source files are UTF-8; avoid expanding Russian UI copy into
    // ASCII escape sequences in production JavaScript.
    charset: 'utf8',
    legalComments: 'none',
    mangleProps: /^(displayStream|userStream|displayTrack|displayEnded|countdownTimer|countdownResolve|elapsedTimer|meterTimer|startedAt|pausedAt|pausedDuration|finishRecording|cleanupMedia|cleanupAudio|releasePreview|clearCountdown|clearElapsedTimer|clearMeterTimer|beginRecording|refreshElapsed|isCurrent|inputStream|inputTrack|inputEnded|fetchAndDecode)$/,
  },
  build: {
    // The supported browser contract is the current Chromium/Firefox/WebKit
    // matrix exercised by Playwright. Avoid legacy transforms that none of
    // those engines need; aggregate lazy-chunk budgets measure this output.
    target: 'esnext',
    minify: 'terser',
    // All supported engines implement modulepreload; the legacy fetch shim is
    // pure startup overhead for the Playwright browser contract above.
    modulePreload: { polyfill: false },
    // Used by the bundle gate to distinguish startup assets from lazy editor
    // surfaces instead of summing every future route into the initial budget.
    manifest: true,
    rollupOptions: {
      output: {
        // Rollup's default helper syntax targets ES5 even when Vite emits
        // modern modules. Match the browser contract above before minifying.
        generatedCode: 'es2015',
        compact: true,
      },
    },
  },
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/files': 'http://127.0.0.1:8080',
    },
  },
})
