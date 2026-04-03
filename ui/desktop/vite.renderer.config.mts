import path from 'path';
import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';

// https://vitejs.dev/config
export default defineConfig({
  define: {
    // This replaces process.env.ALPHA with a literal at build time
    'process.env.ALPHA': JSON.stringify(process.env.ALPHA === 'true'),
    'process.env.GOOSE_TUNNEL': JSON.stringify(process.env.GOOSE_TUNNEL !== 'no' && process.env.GOOSE_TUNNEL !== 'none'),
  },

  plugins: [tailwindcss()],

  resolve: {
    alias: {
      // Force all dependencies (including hoisted ones like react-intl) to use
      // the same React instance as the app.  Without this, Vite's dep
      // pre-bundling can inline a second copy of React resolved from the
      // workspace root node_modules, which triggers the "Invalid hook call"
      // error at runtime.
      'react': path.resolve(__dirname, 'node_modules/react'),
      'react-dom': path.resolve(__dirname, 'node_modules/react-dom'),
    },
  },

  build: {
    target: 'esnext'
  },
});
