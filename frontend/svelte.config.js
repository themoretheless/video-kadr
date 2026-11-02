import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

export default {
  // Keep stylesheet preprocessing on the same JS config-loader path as focused tests.
  preprocess: vitePreprocess({ style: { configLoader: 'runner' } }),
}
