const { build } = require('vite');
const { resolve } = require('path');
const fs = require('fs');

async function buildMain() {
  try {
    const outDir = resolve(__dirname, '../.vite/build');
    
    // Ensure output directory exists
    if (!fs.existsSync(outDir)) {
      fs.mkdirSync(outDir, { recursive: true });
    }

    await build({
      configFile: resolve(__dirname, '../vite.main.config.mts'),
      build: {
        outDir,
        emptyOutDir: false,
        ssr: true,
        rollupOptions: {
          input: resolve(__dirname, '../src/main.ts'),
          output: {
            format: 'cjs',
            entryFileNames: 'main.cjs'
          },
          external: [
            'electron',
            'electron-squirrel-startup',
            'path',
            'fs',
            'url',
            'child_process',
            'crypto',
            'os',
            'util'
          ]
        }
      }
    });

    console.log('Main process build complete');
  } catch (e) {
    console.error('Error building main process:', e);
    process.exit(1);
  }
}

buildMain();
