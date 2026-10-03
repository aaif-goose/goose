import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const wrapperPath = path.join(path.dirname(fileURLToPath(import.meta.url)), 'bin', 'npx.cmd');
const system32 = path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32');

// Stands in for npm's npx.cmd: runs the node.exe next to it, like the real one does.
const fakeNpx = '@ECHO OFF\r\n"%~dp0node.exe" "%FAKE_NPX_REPORT%" %*\r\n';

const reportSource = `
const chunks = [];
process.stdin.on('data', (chunk) => chunks.push(chunk));
process.stdin.on('end', () => {
  console.log(JSON.stringify({
    nodeDir: require('path').dirname(process.execPath),
    firstPathEntry: process.env.PATH.split(';')[0],
    args: process.argv.slice(2),
    stdin: Buffer.concat(chunks).toString(),
  }));
  process.exit(Number(process.env.FAKE_NPX_EXIT));
});
`;

type ChildReport = {
  nodeDir: string;
  firstPathEntry: string;
  args: string[];
  stdin: string;
};

describe.skipIf(process.platform !== 'win32')('Windows npx wrapper', () => {
  let rootDir: string;
  let systemNodeDir: string;
  let nodeWithoutNpxDir: string;
  let failingNodeDir: string;
  let portableNodeDir: string;
  let emptyNodeDir: string;

  function makeDir(name: string) {
    const dir = path.join(rootDir, name);
    fs.mkdirSync(dir);
    return dir;
  }

  function runWrapper(options: {
    pathDirs: string[];
    args: string[];
    goosePortableDir?: string;
    exitCode?: number;
  }) {
    const args = options.args.map((arg) => (/[\s^&|<>]/.test(arg) ? `"${arg}"` : arg));
    // Same shape Rust's std::process::Command uses to start a .cmd file.
    const result = spawnSync(
      'cmd.exe',
      [`/e:ON /v:OFF /d /c ""${wrapperPath}" ${args.join(' ')}"`],
      {
        windowsVerbatimArguments: true,
        encoding: 'utf8',
        input: 'first line\nsecond line\n',
        env: {
          SystemRoot: process.env.SystemRoot,
          ComSpec: process.env.ComSpec,
          PATHEXT: process.env.PATHEXT,
          TEMP: rootDir,
          TMP: rootDir,
          GOOSE_NODE_DIR: options.goosePortableDir ?? emptyNodeDir,
          FAKE_NPX_REPORT: path.join(rootDir, 'report.js'),
          FAKE_NPX_EXIT: String(options.exitCode ?? 0),
          // PowerShell is not on PATH, so the download step fails at once instead of using the network.
          PATH: [...options.pathDirs, system32].join(';'),
        },
      }
    );

    expect(result.error).toBeUndefined();
    return result;
  }

  beforeAll(() => {
    rootDir = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'goose npx wrapper ')));
    fs.writeFileSync(path.join(rootDir, 'report.js'), reportSource);

    systemNodeDir = makeDir('system node');
    fs.copyFileSync(process.execPath, path.join(systemNodeDir, 'node.exe'));
    fs.writeFileSync(path.join(systemNodeDir, 'npx.cmd'), fakeNpx);

    nodeWithoutNpxDir = makeDir('node without npx');
    fs.linkSync(path.join(systemNodeDir, 'node.exe'), path.join(nodeWithoutNpxDir, 'node.exe'));

    // A real executable that is not Node, so the version check fails.
    failingNodeDir = makeDir('failing node');
    fs.copyFileSync(path.join(system32, 'where.exe'), path.join(failingNodeDir, 'node.exe'));
    fs.writeFileSync(path.join(failingNodeDir, 'npx.cmd'), fakeNpx);

    portableNodeDir = makeDir('portable node');
    fs.linkSync(path.join(systemNodeDir, 'node.exe'), path.join(portableNodeDir, 'node.exe'));
    fs.writeFileSync(path.join(portableNodeDir, 'npx.cmd'), fakeNpx);
    const portableVersion = /SET "NODE_VERSION=([\d.]+)"/.exec(
      fs.readFileSync(wrapperPath, 'utf8')
    );
    fs.writeFileSync(path.join(portableNodeDir, `node-v${portableVersion?.[1]}.installed`), '');

    emptyNodeDir = makeDir('empty node');
  }, 60_000);

  afterAll(() => {
    if (rootDir) {
      fs.rmSync(rootDir, { recursive: true, force: true });
    }
  });

  it('uses the first usable Node.js on PATH without downloading', () => {
    const args = ['-y', '@scope/pkg@^1.2.0', 'arg with spaces'];
    const result = runWrapper({
      pathDirs: [nodeWithoutNpxDir, failingNodeDir, systemNodeDir],
      args,
    });

    expect(result.stderr).not.toContain('Downloading');
    expect(result.status).toBe(0);
    const child = JSON.parse(result.stdout) as ChildReport;
    expect(child.nodeDir).toBe(systemNodeDir);
    expect(path.resolve(child.firstPathEntry)).toBe(systemNodeDir);
    expect(child.args).toEqual(args);
    expect(child.stdin).toBe('first line\nsecond line\n');
  });

  it('propagates the npx exit status', () => {
    const result = runWrapper({ pathDirs: [systemNodeDir], args: ['-y', 'pkg'], exitCode: 37 });

    expect(result.status).toBe(37);
    expect((JSON.parse(result.stdout) as ChildReport).nodeDir).toBe(systemNodeDir);
  });

  it('keeps the Goose portable Node.js first when it is installed', () => {
    const result = runWrapper({
      pathDirs: [systemNodeDir],
      args: ['-y', 'pkg'],
      goosePortableDir: portableNodeDir,
      exitCode: 5,
    });

    expect(result.status).toBe(5);
    expect((JSON.parse(result.stdout) as ChildReport).nodeDir).toBe(portableNodeDir);
  });

  it.each<[string, () => string[]]>([
    ['no node.exe', () => []],
    ['node.exe without npx.cmd next to it', () => [nodeWithoutNpxDir]],
    ['node.exe that fails the version check', () => [failingNodeDir]],
  ])('falls back to the download when PATH has %s', (_name, pathDirs) => {
    const result = runWrapper({ pathDirs: pathDirs(), args: ['-y', 'pkg'] });

    expect(result.stdout).toBe('');
    expect(result.stderr).toContain('Downloading portable Node.js');
    expect(result.stderr).toContain('Failed to download Node.js');
    expect(result.status).toBe(1);
  });
});
