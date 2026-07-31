import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  resolve: {
    conditions: mode === 'test' ? ['browser'] : [],
    alias: {
      '$lib': path.resolve('./src/lib'),
      '$app/stores': path.resolve('./src/tests/mocks/app-stores.ts'),
      '$app/navigation': path.resolve('./src/tests/mocks/app-navigation.ts')
    }
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.{test,spec}.{js,ts}']
  }
}));
