#!/usr/bin/env node
/**
 * Cross-platform i18n check script.
 * Extracts messages to a temp file and compares against the committed file
 * to ensure src/i18n/messages/en.json is up to date.
 */
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

const projectDir = path.join(__dirname, '..');
const formatjs = require.resolve('@formatjs/cli/bin/formatjs');
const enFile = path.join(projectDir, 'src', 'i18n', 'messages', 'en.json');
const tmpFile = path.join(os.tmpdir(), 'en.i18n-check.json');

execFileSync(
  process.execPath,
  [
    formatjs,
    'extract',
    'src/**/*.{ts,tsx}',
    '--ignore',
    '**/*.d.ts',
    '--out-file',
    tmpFile,
    '--flatten',
  ],
  { stdio: 'inherit', cwd: projectDir }
);

const committed = fs.readFileSync(enFile, 'utf8');
const extracted = fs.readFileSync(tmpFile, 'utf8');

try {
  fs.unlinkSync(tmpFile);
} catch (_) {
  // ignore cleanup errors
}

if (JSON.stringify(JSON.parse(committed)) !== JSON.stringify(JSON.parse(extracted))) {
  console.error(
    'Error: src/i18n/messages/en.json is out of date. Run pnpm i18n:extract to update it.'
  );
  process.exit(1);
}
