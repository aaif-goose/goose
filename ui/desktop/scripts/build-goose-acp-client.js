#!/usr/bin/env node
/**
 * Both postinstall and start-gui need a current ACP client. Share a build stamp so
 * an unchanged launch only builds once. Packaging always forces a rebuild because
 * the list of build inputs below is maintained manually.
 */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawnSync } = require('child_process');

const desktopRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopRoot, '..', '..');
const acpClientRoot = path.join(repoRoot, 'ui', 'goose-acp-client');
const builtEntry = path.join(acpClientRoot, 'dist', 'index.js');
const stampPath = path.join(
  desktopRoot,
  'node_modules',
  '.cache',
  'goose-acp-client-build-stamp.json'
);

const force = process.argv.includes('--force') || process.env.GOOSE_ACP_CLIENT_FORCE_BUILD === '1';

// Everything the built ACP client is a function of. The two JSON schemas are what generate-schema.ts
// reads; the lockfile is in here so a bump to one of the ACP client's own dependencies invalidates too.
const inputFiles = [
  path.join(repoRoot, 'crates', 'goose', 'acp-schema.json'),
  path.join(repoRoot, 'crates', 'goose', 'acp-meta.json'),
  path.join(acpClientRoot, 'generate-schema.ts'),
  path.join(acpClientRoot, 'tsconfig.json'),
  path.join(acpClientRoot, 'package.json'),
  path.join(repoRoot, 'ui', 'pnpm-lock.yaml'),
];

// src/generated is written by `generate`, not read as a source: its bytes are a function of the
// two schemas, generate-schema.ts and the generator version pinned in the lockfile, all of which
// are hashed above. Leaving it out is what lets the same hash be taken before and after the build
// and compared — see the mid-build check at the bottom.
const generatedDir = path.join(acpClientRoot, 'src', 'generated');

function statOrNull(file) {
  try {
    return fs.statSync(file);
  } catch {
    return null; // broken symlink
  }
}

function collect(dir, found = []) {
  if (!fs.existsSync(dir)) return found;
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries.sort((a, b) => (a.name < b.name ? -1 : 1))) {
    // Skip dotfiles. Finder writes a .DS_Store into any directory you browse and then keeps
    // mutating it (window position, sort order), which would report "inputs changed" on a tree
    // where nothing the build reads has moved — the exact rebuild this gate exists to avoid.
    if (entry.name.startsWith('.')) continue;

    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) collect(full, found);
    else if (entry.isFile()) found.push(full);
    // A symlinked source file must still invalidate the stamp. Linked *directories* are not
    // followed: recursing through one that points at an ancestor would run until the stack blows,
    // and there are none here to justify carrying a seen-set for.
    else if (entry.isSymbolicLink() && statOrNull(full)?.isFile()) found.push(full);
  }
  return found;
}

function hashInputs() {
  const sources = collect(path.join(acpClientRoot, 'src')).filter(
    (file) => !file.startsWith(generatedDir + path.sep)
  );
  const files = [...inputFiles, ...sources];
  const hash = crypto.createHash('sha256');
  for (const file of files) {
    // Path first, so a rename registers even when the bytes are identical. Length-prefixing both
    // fields keeps the boundary unambiguous, so no combination of path and contents can produce
    // the digest of a different set of files.
    const relative = path.relative(repoRoot, file).split(path.sep).join('/');
    const contents = fs.existsSync(file) ? fs.readFileSync(file) : null;
    hash.update(`${relative.length}:${relative}:${contents === null ? -1 : contents.length}:`);
    if (contents !== null) hash.update(contents);
  }
  return hash.digest('hex');
}

// Taken once, before the build, and used twice: to decide whether to build at all, and as the
// baseline the post-build hash is compared against.
const inputsBefore = hashInputs();

function stalenessReason() {
  if (force) return '--force';
  if (!fs.existsSync(builtEntry)) return 'no previous build';
  let stamp;
  try {
    stamp = JSON.parse(fs.readFileSync(stampPath, 'utf8'));
  } catch {
    return 'no usable build stamp';
  }
  return stamp.hash === inputsBefore ? null : 'inputs changed';
}

function buildAcpClient() {
  const args = ['--filter', '@aaif/goose-acp-client', 'run', 'build'];
  const options = { stdio: 'inherit', cwd: desktopRoot };
  // Invoking pnpm's JS entry point directly preserves paths containing spaces on Windows.
  const execPath = process.env.npm_execpath;
  let result;
  if (execPath && /\.[cm]?js$/.test(execPath) && fs.existsSync(execPath)) {
    result = spawnSync(process.execPath, [execPath, ...args], options);
  } else {
    // Windows needs a shell for pnpm.cmd; the command and arguments here are fixed.
    result = spawnSync('pnpm', args, { ...options, shell: process.platform === 'win32' });
  }
  if (result.error) {
    if (result.error.code === 'ENOENT') {
      console.error(
        'Could not run pnpm. Run this through pnpm (`pnpm run build-goose-acp-client`), ' +
          'or put the toolchain on PATH first.'
      );
      process.exit(1);
    }
    throw result.error;
  }
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const reason = stalenessReason();
if (!reason) {
  console.log('goose ACP client is up to date — skipping rebuild (--force to rebuild anyway)');
  process.exit(0);
}

console.log(`Building goose ACP client (${reason}) ...`);

// Drop the stamp first, so that anything other than a clean run through the write below leaves no
// stamp at all. A failed build exits the process, and tsc emits even when it reports
// errors (ui/goose-acp-client/tsconfig.json does not set noEmitOnError), so a failed or interrupted build ends
// with dist partly overwritten. Keeping the previous stamp over that would let a revert to the
// inputs it describes skip the rebuild indefinitely.
fs.rmSync(stampPath, { force: true });

buildAcpClient();

// An input saved while the build was running was compiled from its older bytes, so dist may not
// match the tree. The stamp is already gone, which is the whole fix on a launch: nothing describes
// the newer bytes, so the next one rebuilds instead of skipping over an edit dist never saw.
//
// A forced build cannot end there. --force is what `package`, `make` and scripts/build-windows.ps1
// use precisely because an artifact must be built from the current inputs, and we have just found
// that it might not be. Exiting 0 would let the `&&` chain package that ACP client anyway, which is the
// failure the force path exists to prevent. Fail instead of retrying: a rebuild would race the
// same editor, and a packaging run that starts while the tree is still moving is worth stopping
// rather than papering over.
if (hashInputs() !== inputsBefore) {
  if (force) {
    console.error(
      'goose ACP client inputs changed while the build was running, so ui/goose-acp-client/dist may not match them. ' +
        'Nothing was stamped and no artifact should be built from this. Re-run once the tree ' +
        'has settled.'
    );
    process.exit(1);
  }
  console.log(
    'goose ACP client inputs changed during the build — not stamping; next launch rebuilds'
  );
  process.exit(0);
}

fs.mkdirSync(path.dirname(stampPath), { recursive: true });
fs.writeFileSync(stampPath, `${JSON.stringify({ hash: inputsBefore }, null, 2)}\n`);
