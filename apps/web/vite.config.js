import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    host: '0.0.0.0',
    proxy: {
      // Local API for direct-Vite access (e.g. Tailscale IP): the app calls
      // relative `/api/*`, which Traefik proxies in the :80 flow but Vite
      // does not — without this, direct :5173 access 404s every API call.
      '/api': 'http://localhost:3000'
    },
    hmr: {
      protocol: 'ws',
      host: 'localhost',
      port: 5173
    }
  }
});
