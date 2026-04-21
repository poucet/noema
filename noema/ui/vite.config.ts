import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

// Tauri expects a fixed port; `strictPort` prevents Vite from silently
// falling back to another port when 5173 is busy.
export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    // Tauri ships a webview; skip the extra compat layer for broad browser support.
    target: 'esnext',
  },
});
