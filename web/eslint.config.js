// PT: Lint de TypeScript e Svelte. `svelte/no-at-html-tags` impede HTML vindo de dados.
// EN: TypeScript and Svelte lint. `svelte/no-at-html-tags` forbids HTML coming from data.
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
  { ignores: ['dist/', 'node_modules/', 'src/lib/wasm/pkg/'] },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs.recommended,
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: ts.parser } },
  },
  {
    rules: {
      'svelte/no-at-html-tags': 'error',
      'no-restricted-properties': [
        'error',
        { object: 'element', property: 'innerHTML', message: 'Use textContent.' },
      ],
    },
  },
);
