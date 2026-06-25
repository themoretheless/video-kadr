import js from '@eslint/js'
import globals from 'globals'
import tseslint from 'typescript-eslint'
import pluginVue from 'eslint-plugin-vue'

// Flat config: JS recommended + typescript-eslint + Vue essential. The Vue
// files use vue-eslint-parser (set by the plugin) with the TS parser for
// <script lang="ts">.
export default tseslint.config(
  { ignores: ['dist', 'node_modules'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/essential'],
  {
    files: ['**/*.vue'],
    languageOptions: {
      parserOptions: { parser: tseslint.parser },
    },
  },
  {
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: { ...globals.browser },
    },
  },
  {
    files: ['**/*.test.ts'],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    rules: {
      // Single-word component names (App, Toasts) are fine in this app.
      'vue/multi-word-component-names': 'off',
      // Several catch blocks intentionally swallow errors with a comment.
      'no-empty': ['error', { allowEmptyCatch: true }],
    },
  },
)
