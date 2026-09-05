import { defineConfig } from 'vite';
import path from 'path';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  // Vite options tailored for Tauri development.
  clearScreen: false,
  server: {
    port: 13269,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  optimizeDeps: {
    include: ['@/material-web.ts'],
  },
  build: {
    target: process.env.TAURI_PLATFORM == 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_DEBUG ? 'oxc' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes('node_modules')) {
            if (id.includes('@m3e/react')) return 'vendor-m3e';
            if (id.includes('@mdi/js')) return 'vendor-mdi';
            if (id.includes('@material/material-color-utilities')) return 'vendor-color';
            if (id.includes('react') || id.includes('react-dom') || id.includes('react-router') || id.includes('zustand')) return 'vendor-react';
            if (id.includes('idb')) return 'vendor-idb';
            if (id.includes('@tauri-apps')) return 'vendor-tauri';
          }
        },
      },
    },
  },
});
