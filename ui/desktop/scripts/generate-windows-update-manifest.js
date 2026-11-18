#!/usr/bin/env node

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

// Copies set up by the Windows installer request this manifest (the channel set in
// src/utils/autoUpdater.ts). ZIP copies request latest.yml, which is never published for
// Windows, so they stay on the swap updater.
const MANIFEST_NAME = 'installer.yml';
const INSTALLER_NAME = 'Goose-win32-x64-setup.exe';

function usage() {
  console.error(
    'Usage: node scripts/generate-windows-update-manifest.js --version <version> [--directory <path>]'
  );
}

function parseArgs(argv) {
  const args = {
    directory: process.cwd(),
    version: '',
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--version') {
      args.version = argv[++i] || '';
    } else if (arg === '--directory') {
      args.directory = argv[++i] || '';
    } else {
      usage();
      process.exit(1);
    }
  }

  if (!args.version || !args.directory) {
    usage();
    process.exit(1);
  }

  args.version = args.version.replace(/^v/, '');
  args.directory = path.resolve(args.directory);
  return args;
}

function sha512(filePath) {
  const hash = crypto.createHash('sha512');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('base64');
}

function yamlString(value) {
  return JSON.stringify(value);
}

function writeManifest({ directory, version }) {
  const installerPath = path.join(directory, INSTALLER_NAME);
  if (!fs.existsSync(installerPath)) {
    throw new Error(`Missing required file: ${installerPath}`);
  }

  const digest = sha512(installerPath);
  const manifest = [
    `version: ${yamlString(version)}`,
    'files:',
    `  - url: ${yamlString(INSTALLER_NAME)}`,
    `    sha512: ${yamlString(digest)}`,
    `    size: ${fs.statSync(installerPath).size}`,
    `path: ${yamlString(INSTALLER_NAME)}`,
    `sha512: ${yamlString(digest)}`,
    `releaseDate: ${yamlString(new Date().toISOString())}`,
    '',
  ].join('\n');

  fs.writeFileSync(path.join(directory, MANIFEST_NAME), manifest);
}

try {
  writeManifest(parseArgs(process.argv.slice(2)));
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
}
