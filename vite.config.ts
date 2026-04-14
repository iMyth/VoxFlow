import path from 'path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [tailwindcss(), react()],

  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },

  // Tauri-specific Vite options
  // NOTE: clearScreen is only needed for Tauri development to prevent
  // Vite from obscuring Rust compilation errors during `tauri dev`.
  // It should NOT be used in non-Tauri projects.
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },

  // Production build optimizations
  build: {
    // Use esbuild for faster minification (use 'terser' if you need
    // more aggressive dead-code elimination or custom terser options)
    minify: 'esbuild',
    rollupOptions: {
      output: {
        // Separate vendor chunks for better caching
        manualChunks: {
          vendor: ['react', 'react-dom'],
          'vendor-ui': ['radix-ui', 'class-variance-authority', 'clsx', 'tailwind-merge', 'lucide-react'],
          'vendor-state': ['zustand', 'zundo'],
          'vendor-i18n': ['i18next', 'react-i18next'],
        },
      },
    },
  },
}));
