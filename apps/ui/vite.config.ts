import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const apiTarget = process.env.BALD_API_URL ?? 'http://127.0.0.1:7333';
const devToken = process.env.BALD_UI_TOKEN ?? '';

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    {
      name: 'bald-session-token',
      transformIndexHtml(html) {
        return devToken ? html.replaceAll('__BALD_TOKEN__', devToken) : html;
      },
    },
  ],
  build: {
    target: 'es2022',
    sourcemap: false,
  },
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': {
        target: apiTarget,
        changeOrigin: false,
        headers: devToken ? { 'x-bald-token': devToken } : undefined,
      },
    },
  },
});
