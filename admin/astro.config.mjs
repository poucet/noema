// @ts-check
import { defineConfig } from 'astro/config';

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
      // Single CSS/JS bundle for easy embedding
      cssCodeSplit: false,
    },
  },
});
