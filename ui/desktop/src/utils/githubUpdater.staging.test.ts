import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const userData = vi.hoisted(() => ({ dir: '' }));

vi.mock('electron', () => ({ app: { getPath: () => userData.dir } }));
vi.mock('./logger', () => ({
  default: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));

import { GitHubUpdater, pruneCache } from './githubUpdater';

const archiveBytes = Buffer.from([0x50, 0x4b, 0x03, 0x04]);

function stubDownload() {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(new Uint8Array(archiveBytes), {
          status: 200,
          headers: { 'content-length': String(archiveBytes.length) },
        })
    )
  );
}

async function writeArchive(dir: string, name: string, ageSeconds: number): Promise<void> {
  const archivePath = path.join(dir, name);
  await fs.writeFile(archivePath, archiveBytes);
  const time = new Date(Date.now() - ageSeconds * 1000);
  await fs.utimes(archivePath, time, time);
}

beforeEach(async () => {
  userData.dir = await fs.mkdtemp(
    path.join(await fs.realpath(os.tmpdir()), 'goose-updater-userdata-')
  );
});

afterEach(async () => {
  vi.unstubAllGlobals();
  await fs.rm(userData.dir, { recursive: true, force: true });
});

describe('GitHubUpdater download cache', () => {
  it.skipIf(process.platform === 'win32')(
    'writes the archive into the update cache without following an entry already there',
    async () => {
      const cacheDir = path.join(userData.dir, 'update-cache');
      const victimPath = path.join(userData.dir, 'victim.txt');
      const archivePath = path.join(cacheDir, 'Goose-9.9.9.zip');
      await fs.mkdir(cacheDir);
      await fs.writeFile(victimPath, 'do not overwrite');
      await fs.symlink(victimPath, archivePath);
      await fs.symlink(victimPath, `${archivePath}.partial`);
      stubDownload();

      const result = await new GitHubUpdater().downloadUpdate(
        'https://example.invalid/Goose.zip',
        '9.9.9'
      );

      expect(result).toEqual({ success: true, downloadPath: archivePath });
      expect(await fs.readFile(victimPath, 'utf8')).toBe('do not overwrite');
      const archiveStat = await fs.lstat(archivePath);
      expect(archiveStat.isFile()).toBe(true);
      expect(archiveStat.mode & 0o777).toBe(0o600);
      expect(await fs.readFile(archivePath)).toEqual(archiveBytes);
      expect(await fs.readdir(cacheDir)).toEqual(['Goose-9.9.9.zip']);
    }
  );

  it('keeps only the two newest cached archives once a download completes', async () => {
    const cacheDir = path.join(userData.dir, 'update-cache');
    await fs.mkdir(cacheDir);
    await writeArchive(cacheDir, 'Goose-1.0.0.zip', 300);
    await writeArchive(cacheDir, 'Goose-1.1.0.zip', 200);
    await writeArchive(cacheDir, 'Goose-1.2.0.zip', 100);
    stubDownload();

    const result = await new GitHubUpdater().downloadUpdate(
      'https://example.invalid/Goose.zip',
      '1.3.0'
    );

    expect(result.success).toBe(true);
    expect((await fs.readdir(cacheDir)).sort()).toEqual(['Goose-1.2.0.zip', 'Goose-1.3.0.zip']);
  });
});

describe('pruneCache', () => {
  it('removes all but the newest archives and leaves other files alone', async () => {
    await writeArchive(userData.dir, 'Goose-1.0.0.zip', 300);
    await writeArchive(userData.dir, 'Goose-1.1.0.zip', 200);
    await writeArchive(userData.dir, 'Goose-1.2.0.zip', 100);
    await fs.writeFile(path.join(userData.dir, 'notes.txt'), 'keep');

    await pruneCache(userData.dir);

    expect((await fs.readdir(userData.dir)).sort()).toEqual([
      'Goose-1.1.0.zip',
      'Goose-1.2.0.zip',
      'notes.txt',
    ]);
  });
});
