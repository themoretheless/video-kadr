import js from '@eslint/js'
import globals from 'globals'
import tseslint from 'typescript-eslint'
import pluginVue from 'eslint-plugin-vue'

// Flat config: JS recommended + typescript-eslint + Vue essential. The Vue
// files use vue-eslint-parser (set by the plugin) with the TS parser for
// <script lang="ts">.
export default tseslint.config(
  { ignores: ['dist', 'node_modules', 'playwright-report', 'test-results'] },
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
    files: ['**/*.test.ts', 'e2e/**/*.ts', 'playwright.config.ts', 'scripts/**/*.mjs'],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    files: ['src/domain/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['**/api', '**/api.*', '**/store', '**/store.*', '**/components/**'],
              message: 'Domain code must stay framework- and transport-independent.',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['src/components/**/*.{ts,vue}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['**/api', '**/api.*'],
              message: 'Components use the store or a feature facade instead of the transport layer.',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['src/api.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['./store', './store.*', './components/**'],
              message: 'The transport layer cannot depend on UI state or components.',
            },
          ],
        },
      ],
    },
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
