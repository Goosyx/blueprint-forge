// PT: Configuração do Vite. Em desenvolvimento, /api vai para o servidor em 127.0.0.1:8080,
//     mantendo a mesma origem para os cookies de sessão.
// EN: Vite configuration. In development, /api goes to the server at 127.0.0.1:8080,
//     keeping the same origin for session cookies.
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [svelte()],
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': 'http://127.0.0.1:8080',
    },
  },
  worker: {
    format: 'es',
  },
  build: {
    target: 'es2022',
    sourcemap: false,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
