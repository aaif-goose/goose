import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { execFileSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { GitHubUpdater } from './githubUpdater';

vi.mock('electron', () => ({ app: { getVersion: () => '1.50.0' } }));
vi.mock('./logger', () => ({
  default: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));

const originalPlatform = Object.getOwnPropertyDescriptor(process, 'platform')!;
const originalArch = Object.getOwnPropertyDescriptor(process, 'arch')!;
const originalSystemVersion = Object.getOwnPropertyDescriptor(process, 'getSystemVersion');
const metadataUrl = 'https://example.invalid/mac-update-requirements.json';
const assets = [
  { name: 'mac-update-requirements.json', browser_download_url: metadataUrl, size: 100 },
  { name: 'Goose.zip', browser_download_url: 'https://example.invalid/Goose.zip', size: 100 },
  {
    name: 'Goose_intel_mac.zip',
    browser_download_url: 'https://example.invalid/Goose_intel_mac.zip',
    size: 100,
  },
  {
    name: 'Goose-win32-x64.zip',
    browser_download_url: 'https://example.invalid/Goose-win32-x64.zip',
    size: 100,
  },
  {
    name: 'Goose-linux-x64.zip',
    browser_download_url: 'https://example.invalid/Goose-linux-x64.zip',
    size: 100,
  },
];
const release = { tag_name: 'v1.51.0', name: 'Goose', assets };

function generatedRequirements(minimumMacOSVersion: string) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-fallback-release-test-'));
  try {
    for (const name of ['Goose.zip', 'Goose_intel_mac.zip']) {
      fs.writeFileSync(path.join(directory, name), 'archive fixture');
      fs.writeFileSync(
        path.join(directory, `${name}.macos.json`),
        JSON.stringify({ minimumMacOSVersion })
      );
    }
    execFileSync(process.execPath, [
      path.resolve('scripts/generate-mac-update-manifest.js'),
      '--version',
      'v1.51.0',
      '--directory',
      directory,
    ]);
    return JSON.parse(
      fs.readFileSync(path.join(directory, 'mac-update-requirements.json'), 'utf8')
    ) as unknown;
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

const macOS12Release = generatedRequirements('12.0.0');
const macOS13Release = generatedRequirements('13.0.0');

function mockRelease(metadata: unknown = macOS13Release, releaseAssets = assets) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) => {
      if (url === 'https://api.github.com/repos/aaif-goose/goose/releases/latest') {
        return new Response(JSON.stringify({ ...release, assets: releaseAssets }));
      }
      if (url === metadataUrl) {
        return metadata instanceof Response ? metadata : new Response(JSON.stringify(metadata));
      }
      throw new Error(`Unexpected request: ${url}`);
    })
  );
}

beforeEach(() => {
  Object.defineProperty(process, 'platform', { value: 'darwin' });
  Object.defineProperty(process, 'arch', { value: 'arm64' });
  Object.defineProperty(process, 'getSystemVersion', {
    value: vi.fn(() => '12.7.6'),
    configurable: true,
  });
});

afterEach(() => {
  Object.defineProperty(process, 'platform', originalPlatform);
  Object.defineProperty(process, 'arch', originalArch);
  if (originalSystemVersion) {
    Object.defineProperty(process, 'getSystemVersion', originalSystemVersion);
  } else {
    Reflect.deleteProperty(process, 'getSystemVersion');
  }
  vi.unstubAllGlobals();
});

describe('GitHub updater macOS compatibility', () => {
  it.each(['arm64', 'x64'])('does not offer a macOS 13 update on macOS 12 (%s)', async (arch) => {
    Object.defineProperty(process, 'arch', { value: arch });
    mockRelease();
    const result = await new GitHubUpdater().checkForUpdates();
    expect(result).toEqual({ updateAvailable: false, latestVersion: '1.51.0' });
    expect(result.downloadUrl).toBeUndefined();
  });

  it.each([
    ['arm64', '13.0', 'Goose.zip'],
    ['x64', '13.0', 'Goose_intel_mac.zip'],
    ['arm64', '26.0', 'Goose.zip'],
  ])('offers the %s download on macOS %s', async (arch, version, asset) => {
    Object.defineProperty(process, 'arch', { value: arch });
    vi.mocked(process.getSystemVersion).mockReturnValue(version);
    mockRelease();
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: true,
      downloadUrl: `https://example.invalid/${asset}`,
    });
  });

  it('still offers a macOS 12-compatible release on macOS 12', async () => {
    mockRelease(macOS12Release);
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({ updateAvailable: true });
  });

  it.each([
    {},
    { version: '1.51.0', minimumMacOSVersion: 'invalid' },
    { version: '1.50.0', minimumMacOSVersion: '12.0.0' },
  ])('rejects malformed or mismatched requirements: %j', async (metadata) => {
    mockRelease(metadata);
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: false,
      error: expect.any(String),
    });
  });

  it('does not offer an update without compatibility metadata', async () => {
    mockRelease(
      macOS13Release,
      assets.filter((asset) => asset.name !== 'mac-update-requirements.json')
    );
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: false,
      error: expect.stringContaining('compatibility information'),
    });
  });

  it('does not offer an update when the compatibility file cannot be retrieved', async () => {
    mockRelease(new Response('', { status: 503 }));
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: false,
      error: expect.any(String),
    });
  });

  it.each(['win32', 'linux'])('leaves %s updates unchanged', async (platform) => {
    Object.defineProperty(process, 'platform', { value: platform });
    Object.defineProperty(process, 'arch', { value: 'x64' });
    mockRelease(
      undefined,
      assets.filter((asset) => asset.name !== 'mac-update-requirements.json')
    );
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: true,
      downloadUrl: `https://example.invalid/Goose-${platform}-x64.zip`,
    });
  });
});
