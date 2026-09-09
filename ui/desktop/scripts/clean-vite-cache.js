const fs = require('fs');
const path = require('path');

const desktopRoot = path.resolve(__dirname, '..');

const pathsToRemove = [
  path.join(desktopRoot, 'node_modules', '.vite'),
  path.join(desktopRoot, 'node_modules', '.vite-temp'),
  path.join(desktopRoot, '.vite'),
];

for (const targetPath of pathsToRemove) {
  if (!fs.existsSync(targetPath)) {
    continue;
  }

  fs.rmSync(targetPath, { recursive: true, force: true });
  console.log(`Removed ${path.relative(desktopRoot, targetPath)}`);
}
