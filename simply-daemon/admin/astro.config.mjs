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
    // Our workspace packages are shipped as raw TypeScript source (no build
    // step). Astro's SSR externalizes workspace deps by default, which hands
    // them to Node's strict ESM resolver and breaks on directory imports like
    // `./transport`. Inlining them forces Vite to resolve and transpile.
    ssr: {
      noExternal: ['@simply/client', '@simply/entity-ui'],
    },
    plugins: [tailwindcss()],
    server: {
      watch: {
        ignored: ['**/dist/**', '**/.astro/**'],
      },
      proxy: {
        '/api': { target: 'http://127.0.0.1:9800', changeOrigin: true },
        '/admin/api': { target: 'http://127.0.0.1:9800', changeOrigin: true },
        '/auth': { target: 'http://127.0.0.1:9800', changeOrigin: true },
        '/ws': { target: 'ws://127.0.0.1:9800', ws: true },
      },
    },
  },

  integrations: [svelte()],
});