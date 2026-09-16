import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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

function mockRelease(metadata: unknown = { version: '1.51.0', minimumMacOSVersion: '13.0.0' }) {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(new Response(JSON.stringify(release)))
    .mockResolvedValueOnce(new Response(JSON.stringify(metadata)));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
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
    const fetchMock = mockRelease();
    const result = await new GitHubUpdater().checkForUpdates();
    expect(result).toEqual({ updateAvailable: false, latestVersion: '1.51.0' });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenLastCalledWith(metadataUrl, expect.any(Object));
  });

  it.each(['13.0', '13.6.1', '14.0', '26.0'])(
    'offers a compatible update on macOS %s',
    async (version) => {
      vi.mocked(process.getSystemVersion).mockReturnValue(version);
      mockRelease();
      expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
        updateAvailable: true,
        downloadUrl: 'https://example.invalid/Goose.zip',
      });
    }
  );

  it('still offers a macOS 12-compatible release on macOS 12', async () => {
    mockRelease({ version: '1.51.0', minimumMacOSVersion: '12.0.0' });
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
    const fetchMock = mockRelease();
    fetchMock
      .mockReset()
      .mockResolvedValueOnce(new Response(JSON.stringify({ ...release, assets: assets.slice(1) })));
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: false,
      error: expect.stringContaining('compatibility information'),
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it.each([404, 500])('does not offer an update when metadata returns HTTP %s', async (status) => {
    const fetchMock = mockRelease();
    fetchMock
      .mockReset()
      .mockResolvedValueOnce(new Response(JSON.stringify(release)))
      .mockResolvedValueOnce(new Response('', { status }));
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({
      updateAvailable: false,
      error: expect.any(String),
    });
  });

  it.each(['win32', 'linux'])('leaves %s updates unchanged', async (platform) => {
    Object.defineProperty(process, 'platform', { value: platform });
    Object.defineProperty(process, 'arch', { value: 'x64' });
    const fetchMock = mockRelease();
    expect(await new GitHubUpdater().checkForUpdates()).toMatchObject({ updateAvailable: true });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
