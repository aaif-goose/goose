#!/usr/bin/env node
/**
 * Builds @aaif/goose-sdk, but only when something it is built from has changed.
 *
 * The SDK build is invoked twice per dev launch — once from this package's `postinstall` and
 * again from `start-gui` — and neither call site is wrong: `postinstall` covers a fresh clone,
 * `start-gui` covers edits made since the last install. What was wrong is that both ran the full
 * generate + `tsc` unconditionally, so an unchanged tree paid for two identical rebuilds before
 * Electron even started.
 *
 * So instead of removing a call site, this hashes the SDK's actual inputs and skips the build when
 * they match the stamp written by the last successful build. The second call of a launch then
 * costs a few file reads.
 *
 * The stamp lives under node_modules, which makes it absent in exactly the cases that must build
 * anyway: a fresh clone, a `git clean`, CI. Deliberately not in ui/sdk/dist, which is the published
 * package (`files: ["dist"]`). Note this puts the stamp and the artifact it describes in different
 * persistence domains — CI caches node_modules but nothing caches ui/sdk/dist — which is why
 * the `builtEntry` check below is load-bearing rather than belt-and-braces.
 *
 * What is deliberately NOT covered is a build product that was partially deleted or hand-edited
 * while its inputs stayed put — dist/, or the generated sources under src/generated that the gate
 * treats as output rather than input. It trusts its own stamp there. Pass --force (or set
 * GOOSE_SDK_FORCE_BUILD=1) to rebuild regardless; `pnpm run build-goose-sdk:force` is that.
 *
 * An input edited *while a build is running* is covered, though: the hash is taken again afterwards
 * and the stamp dropped if it moved, so the edit is not lost to a skip on the next launch.
 *
 * `package` and `make` use that force script rather than this gate. inputFiles below is a
 * hand-maintained restatement of what the build reads, and nothing keeps it in sync as the SDK
 * grows; a launch that skips on a stale list costs seconds, an artifact that does ships a UI whose
 * generated ACP dispatch disagrees with the backend schema. Keep the gate on the launch paths only.
 */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawnSync } = require('child_process');

const desktopRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopRoot, '..', '..');
const sdkRoot = path.join(repoRoot, 'ui', 'sdk');
const builtEntry = path.join(sdkRoot, 'dist', 'index.js');
const stampPath = path.join(desktopRoot, 'node_modules', '.cache', 'goose-sdk-build-stamp.json');

const force = process.argv.includes('--force') || process.env.GOOSE_SDK_FORCE_BUILD === '1';

// Everything the built SDK is a function of. The two JSON schemas are what generate-schema.ts
// reads; the lockfile is in here so a bump to one of the SDK's own dependencies invalidates too.
const inputFiles = [
  path.join(repoRoot, 'crates', 'goose', 'acp-schema.json'),
  path.join(repoRoot, 'crates', 'goose', 'acp-meta.json'),
  path.join(sdkRoot, 'generate-schema.ts'),
  path.join(sdkRoot, 'tsconfig.json'),
  path.join(sdkRoot, 'package.json'),
  path.join(repoRoot, 'ui', 'pnpm-lock.yaml'),
];

// src/generated is written by `generate`, not read as a source: its bytes are a function of the
// two schemas, generate-schema.ts and the generator version pinned in the lockfile, all of which
// are hashed above. Leaving it out is what lets the same hash be taken before and after the build
// and compared — see the mid-build check at the bottom. It also drops the bulk of the bytes: the
// generated files are four of the eight under src, and 228K of its 244K.
const generatedDir = path.join(sdkRoot, 'src', 'generated');

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
  const sources = collect(path.join(sdkRoot, 'src')).filter(
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

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', cwd: desktopRoot, ...options });
  if (result.error) {
    if (result.error.code === 'ENOENT') {
      console.error(
        `Could not run ${command}. Run this through pnpm (\`pnpm run build-goose-sdk\`), ` +
          'or put the toolchain on PATH first.'
      );
      process.exit(1);
    }
    throw result.error;
  }
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function runPnpm(args) {
  // Prefer the pnpm that invoked us: npm_execpath is pnpm's own JS entry point, which sidesteps
  // PATH lookup and the .cmd/.exe shim question entirely.
  const execPath = process.env.npm_execpath;
  if (execPath && /\.[cm]?js$/.test(execPath) && fs.existsSync(execPath)) {
    run(process.execPath, [execPath, ...args]);
    return;
  }
  // Only this path needs a shell, and only on Windows: since the CVE-2024-27980 fix, spawning a
  // .cmd without one throws EINVAL. Do not hoist it into run() — with a shell, Node space-joins
  // the argv and lets cmd.exe re-parse it, which breaks on any path containing a space.
  //
  // Bare `pnpm`, not `pnpm.cmd`: cmd.exe resolves it through PATHEXT, so this finds the .cmd that
  // a corepack or npm -g install leaves behind *and* the bare pnpm.exe that the standalone
  // installer ships, where a hardcoded .cmd would not exist at all.
  const onWindows = process.platform === 'win32';
  run('pnpm', args, { shell: onWindows });
}

const reason = stalenessReason();
if (!reason) {
  console.log('goose SDK is up to date — skipping rebuild (--force to rebuild anyway)');
  process.exit(0);
}

console.log(`Building goose SDK (${reason}) ...`);

// Drop the stamp first, so that anything other than a clean run through the write below leaves no
// stamp at all. run() exits the process on a failed build, and tsc emits even when it reports
// errors (ui/sdk/tsconfig.json does not set noEmitOnError), so a failed or interrupted build ends
// with dist partly overwritten. Keeping the previous stamp over that would let a revert to the
// inputs it describes skip the rebuild indefinitely.
fs.rmSync(stampPath, { force: true });

runPnpm(['--filter', '@aaif/goose-sdk', 'run', 'build']);

// An input saved while the build was running was compiled from its older bytes, so dist may not
// match the tree. The stamp is already gone, which is the whole fix on a launch: nothing describes
// the newer bytes, so the next one rebuilds instead of skipping over an edit dist never saw.
//
// A forced build cannot end there. --force is what `package`, `make` and scripts/build-windows.ps1
// use precisely because an artifact must be built from the current inputs, and we have just found
// that it might not be. Exiting 0 would let the `&&` chain package that SDK anyway, which is the
// failure the force path exists to prevent. Fail instead of retrying: a rebuild would race the
// same editor, and a packaging run that starts while the tree is still moving is worth stopping
// rather than papering over.
if (hashInputs() !== inputsBefore) {
  if (force) {
    console.error(
      'goose SDK inputs changed while the build was running, so ui/sdk/dist may not match them. ' +
        'Nothing was stamped and no artifact should be built from this. Re-run once the tree ' +
        'has settled.'
    );
    process.exit(1);
  }
  console.log('goose SDK inputs changed during the build — not stamping; next launch rebuilds');
  process.exit(0);
}

fs.mkdirSync(path.dirname(stampPath), { recursive: true });
fs.writeFileSync(stampPath, `${JSON.stringify({ hash: inputsBefore }, null, 2)}\n`);
