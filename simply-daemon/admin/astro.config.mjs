// @ts-check
import { defineConfig } from 'astro/config';

import svelte from '@astrojs/svelte';

import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  // Static output — built files served by the daemon
  output: 'static',

  // The daemon serves these at /admin/*
  base: '/',

  build: {
    // Output to dist/ — daemon embeds or serves from here
    assets: '_assets',
  },

  vite: {
    build: {
      cssCodeSplit: false,
    },
    plugins: [tailwindcss()],
    server: {
      proxy: {
        '/api': 'http://127.0.0.1:9800',
        '/admin/api': 'http://127.0.0.1:9800',
        '/auth': 'http://127.0.0.1:9800',
        '/ws': { target: 'ws://127.0.0.1:9800', ws: true },
      },
    },
  },

  integrations: [svelte()],
});