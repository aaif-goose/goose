import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';

const launcherSourceDir = path.dirname(fileURLToPath(import.meta.url));
const tempDirs: string[] = [];

function makeLauncherHarness(launcherName: 'node' | 'npx') {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), 'goose node launchers '));
  tempDirs.push(rootDir);

  const launcherDir = path.join(rootDir, 'launcher');
  const callerDir = path.join(rootDir, 'caller directory');
  const setupDir = path.join(rootDir, 'setup directory');
  const fakeBinDir = path.join(rootDir, 'fake bin');
  fs.mkdirSync(launcherDir);
  fs.mkdirSync(callerDir);
  fs.mkdirSync(setupDir);
  fs.mkdirSync(fakeBinDir);

  const launcherPath = path.join(launcherDir, launcherName);
  fs.copyFileSync(path.join(launcherSourceDir, launcherName), launcherPath);
  fs.chmodSync(launcherPath, 0o755);

  fs.writeFileSync(
    path.join(launcherDir, 'node-setup-common.sh'),
    ['cd -- "$FAKE_SETUP_DIR"', 'export PATH="$FAKE_BIN_DIR:$PATH"', 'log() { :; }', ''].join('\n')
  );

  const childPath = path.join(fakeBinDir, launcherName);
  fs.writeFileSync(
    childPath,
    ['#!/bin/bash', 'printf "%s\\n" "$PWD"', 'exit "$FAKE_CHILD_EXIT_CODE"', ''].join('\n')
  );
  fs.chmodSync(childPath, 0o755);

  return { callerDir, fakeBinDir, launcherPath, setupDir };
}

function runLauncher(launcherName: 'node' | 'npx', exitCode: number) {
  const harness = makeLauncherHarness(launcherName);
  const result = spawnSync(harness.launcherPath, ['--fixture'], {
    cwd: harness.callerDir,
    encoding: 'utf8',
    env: {
      ...process.env,
      FAKE_BIN_DIR: harness.fakeBinDir,
      FAKE_CHILD_EXIT_CODE: String(exitCode),
      FAKE_SETUP_DIR: harness.setupDir,
    },
  });

  expect(result.error).toBeUndefined();
  return { ...harness, callerDir: fs.realpathSync(harness.callerDir), result };
}

afterEach(() => {
  while (tempDirs.length > 0) {
    fs.rmSync(tempDirs.pop()!, { recursive: true, force: true });
  }
});

describe.skipIf(process.platform === 'win32').each(['node', 'npx'] as const)(
  '%s desktop launcher',
  (launcherName) => {
    it('runs the child from the caller working directory', () => {
      const { callerDir, result } = runLauncher(launcherName, 0);

      expect(result.status).toBe(0);
      expect(result.stdout.trim()).toBe(callerDir);
    });

    it('propagates a nonzero child exit status', () => {
      const { callerDir, result } = runLauncher(launcherName, 37);

      expect(result.status).toBe(37);
      expect(result.stdout.trim()).toBe(callerDir);
    });
  }
);

type SetupLauncher = 'npx' | 'uvx' | 'jbang';

const hermitManagedTools: Record<SetupLauncher, string[]> = {
  npx: ['node', 'npx'],
  uvx: ['uv', 'uvx'],
  jbang: ['java', 'jbang'],
};

function writeExecutable(filePath: string, lines: string[]) {
  fs.writeFileSync(filePath, [...lines, ''].join('\n'));
  fs.chmodSync(filePath, 0o755);
}

// Runs the real launcher script against an already initialized hermit
// environment whose hermit and tools are stubs, so no network is needed.
function makeSetupLauncherHarness(launcherName: SetupLauncher) {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), 'goose launcher logs '));
  tempDirs.push(rootDir);

  const launcherDir = path.join(rootDir, 'launcher');
  const tmpDir = path.join(rootDir, 'tmp');
  const gooseRoot = path.join(rootDir, 'goose');
  const configDir = path.join(gooseRoot, 'config');
  const hermitBinDir = path.join(configDir, 'mcp-hermit', 'bin');
  fs.mkdirSync(launcherDir);
  fs.mkdirSync(tmpDir);
  fs.mkdirSync(hermitBinDir, { recursive: true });

  for (const file of [launcherName, 'node-setup-common.sh']) {
    fs.copyFileSync(path.join(launcherSourceDir, file), path.join(launcherDir, file));
  }
  const launcherPath = path.join(launcherDir, launcherName);
  fs.chmodSync(launcherPath, 0o755);

  fs.writeFileSync(path.join(configDir, '.mcp-hermit-cleanup-v1'), '');
  fs.writeFileSync(path.join(hermitBinDir, 'activate-hermit'), '');
  writeExecutable(path.join(hermitBinDir, 'hermit'), ['#!/bin/bash', 'exit 0']);
  for (const tool of hermitManagedTools[launcherName]) {
    writeExecutable(path.join(hermitBinDir, tool), [
      '#!/bin/bash',
      `printf 'child:${tool}:%s\\n' "$*"`,
    ]);
  }

  return { gooseRoot, launcherPath, rootDir, tmpDir };
}

function runSetupLauncher(
  harness: ReturnType<typeof makeSetupLauncherHarness>,
  tmpDir = harness.tmpDir
) {
  const env: Record<string, string | undefined> = {
    ...process.env,
    GOOSE_PATH_ROOT: harness.gooseRoot,
    HOME: harness.rootDir,
    TMPDIR: tmpDir,
  };
  for (const name of [
    'GOOSE_CONFIG_DIR',
    'GOOSE_NPM_REGISTRY',
    'GOOSE_UV_REGISTRY',
    'GOOSE_JBANG_REGISTRY',
  ]) {
    delete env[name];
  }

  const result = spawnSync(harness.launcherPath, ['--fixture'], {
    cwd: harness.rootDir,
    encoding: 'utf8',
    env,
  });

  expect(result.error).toBeUndefined();
  return result;
}

describe.skipIf(process.platform === 'win32').each(['npx', 'uvx', 'jbang'] as const)(
  '%s desktop launcher log',
  (launcherName) => {
    const childOutput = new RegExp(`child:${launcherName}:.*--fixture`);

    it('writes a separate private log for each launch', () => {
      const harness = makeSetupLauncherHarness(launcherName);

      for (const result of [runSetupLauncher(harness), runSetupLauncher(harness)]) {
        expect(result.status).toBe(0);
        expect(result.stdout).toMatch(childOutput);
      }

      const logs = fs.readdirSync(harness.tmpDir).filter((name) => name.startsWith('goose-mcp-'));
      expect(logs).toHaveLength(2);
      for (const name of logs) {
        const logPath = path.join(harness.tmpDir, name);
        expect(fs.statSync(logPath).mode & 0o777).toBe(0o600);
        expect(fs.readFileSync(logPath, 'utf8')).toContain(`Log file: ${logPath}`);
      }
    });

    it('still starts the child when the log file cannot be created', () => {
      const harness = makeSetupLauncherHarness(launcherName);

      const result = runSetupLauncher(harness, path.join(harness.rootDir, 'missing'));

      expect(result.status).toBe(0);
      expect(result.stdout).toMatch(childOutput);
      expect(`${result.stdout}${result.stderr}`).toContain('Log file: /dev/null');
    });
  }
);
