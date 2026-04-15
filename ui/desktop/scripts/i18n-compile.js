#!/usr/bin/env node
/**
 * Cross-platform i18n compile script.
 * Compiles all JSON message files in src/i18n/messages/ using formatjs.
 */
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const projectDir = path.join(__dirname, '..');
const formatjs = require.resolve('@formatjs/cli/bin/formatjs');
const messagesDir = path.join(projectDir, 'src', 'i18n', 'messages');
const compiledDir = path.join(projectDir, 'src', 'i18n', 'compiled');

fs.mkdirSync(compiledDir, { recursive: true });

const files = fs.readdirSync(messagesDir).filter((f) => f.endsWith('.json'));

for (const file of files) {
  const locale = path.basename(file, '.json');
  const inFile = path.join(messagesDir, file).split(path.sep).join('/');
  const outFile = path.join(compiledDir, `${locale}.json`);
  execFileSync(process.execPath, [formatjs, 'compile', inFile, '--out-file', outFile], {
    stdio: 'inherit',
    cwd: projectDir,
  });
}
