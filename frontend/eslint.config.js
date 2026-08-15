import js from '@eslint/js'
import svelte from 'eslint-plugin-svelte'
import globals from 'globals'
import tseslint from 'typescript-eslint'

export default tseslint.config(
  { ignores: ['dist', 'node_modules', 'coverage'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    files: ['**/*.svelte.ts'],
    languageOptions: { parser: tseslint.parser },
  },
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: ['.svelte'],
      },
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
    files: ['**/*.test.ts', 'e2e/**/*.ts', 'scripts/**/*.mjs', 'playwright*.config.ts', 'vitest.config.ts', 'vite*.config.ts'],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    files: ['src/lib/domain/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['**/api', '**/api.*', '**/state/**', '**/components/**'],
              message: 'Domain code must stay framework- and transport-independent.',
            },
          ],
        },
      ],
    },
  },
)
