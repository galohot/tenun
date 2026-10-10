import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Two entries: the app (served by the Tenun server) and the viewer (inlined
// into exported HTML files, see src/viewer).
export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  build:
    mode === 'viewer'
      ? {
          outDir: 'dist-viewer',
          emptyOutDir: true,
          cssCodeSplit: false,
          assetsInlineLimit: 100_000_000,
          rollupOptions: {
            input: 'viewer.html',
            output: { inlineDynamicImports: true, entryFileNames: 'viewer.js', assetFileNames: 'viewer.[ext]' },
          },
        }
      : { outDir: 'dist', emptyOutDir: true, chunkSizeWarningLimit: 1200 },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:4310',
        ws: true,
        // The server only answers to its own Host and Origin.
        configure: (proxy) => {
          proxy.on('proxyReq', (req) => {
            req.setHeader('host', '127.0.0.1:4310');
            req.removeHeader('origin');
          });
          proxy.on('proxyReqWs', (req) => {
            req.setHeader('host', '127.0.0.1:4310');
            req.removeHeader('origin');
          });
        },
      },
    },
  },
}));
