import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      '$lib': path.resolve('./src/lib'),
      '$app/stores': path.resolve('./src/tests/mocks/app-stores.ts'),
      '$app/navigation': path.resolve('./src/tests/mocks/app-navigation.ts')
    }
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.{js,ts}']
  }
});
