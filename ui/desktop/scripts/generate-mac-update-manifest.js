#!/usr/bin/env node

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { macUpdateRequirements } = require('./mac-update-requirements');

function usage() {
  console.error(
    'Usage: node scripts/generate-mac-update-manifest.js --version <version> [--directory <path>]'
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

function ensureFile(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Missing required file: ${filePath}`);
  }
}

function copyIfDifferent(source, target) {
  ensureFile(source);
  if (path.resolve(source) === path.resolve(target)) {
    return;
  }
  fs.copyFileSync(source, target);
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
  const files = [
    {
      sourceName: 'Goose.zip',
      updateName: 'Goose-darwin-arm64.zip',
    },
    {
      sourceName: 'Goose_intel_mac.zip',
      updateName: 'Goose-darwin-x64.zip',
    },
  ];

  const requirements = files.map(({ sourceName }) => {
    const metadata = JSON.parse(
      fs.readFileSync(path.join(directory, `${sourceName}.macos.json`), 'utf8')
    );
    return macUpdateRequirements(metadata.minimumMacOSVersion, metadata.minimumMacOSVersion);
  });
  const minimum = requirements.reduce((highest, current) =>
    Number(current.minimumSystemVersion.split('.')[0]) >
    Number(highest.minimumSystemVersion.split('.')[0])
      ? current
      : highest
  );

  fs.writeFileSync(
    path.join(directory, 'mac-update-requirements.json'),
    `${JSON.stringify({ version, ...minimum })}\n`
  );

  const entries = files.map(({ sourceName, updateName }) => {
    const sourcePath = path.join(directory, sourceName);
    const updatePath = path.join(directory, updateName);
    copyIfDifferent(sourcePath, updatePath);

    const stats = fs.statSync(updatePath);
    return {
      url: updateName,
      sha512: sha512(updatePath),
      size: stats.size,
    };
  });

  const manifest = [
    `version: ${yamlString(version)}`,
    `minimumSystemVersion: ${yamlString(minimum.minimumSystemVersion)}`,
    'files:',
    ...entries.flatMap((entry) => [
      `  - url: ${yamlString(entry.url)}`,
      `    sha512: ${yamlString(entry.sha512)}`,
      `    size: ${entry.size}`,
    ]),
    `path: ${yamlString(entries[0].url)}`,
    `sha512: ${yamlString(entries[0].sha512)}`,
    `releaseDate: ${yamlString(new Date().toISOString())}`,
    '',
  ].join('\n');

  fs.writeFileSync(path.join(directory, 'latest-mac.yml'), manifest);
}

try {
  writeManifest(parseArgs(process.argv.slice(2)));
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
}
