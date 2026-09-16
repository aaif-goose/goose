const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { AppUpdater } = require('electron-updater/out/AppUpdater');
const { parseUpdateInfo } = require('electron-updater/out/providers/Provider');

function workspace(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-manifest-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  return directory;
}

function recordBundle(directory, name, appMinimum, deploymentTarget) {
  const appPath = path.join(directory, `${name}.app`);
  fs.mkdirSync(path.join(appPath, 'Contents'), { recursive: true });
  fs.writeFileSync(
    path.join(appPath, 'Contents', 'Info.plist'),
    `<?xml version="1.0"?><plist version="1.0"><dict><key>LSMinimumSystemVersion</key><string>${appMinimum}</string></dict></plist>`
  );
  fs.writeFileSync(path.join(directory, name), 'archive fixture');
  execFileSync(
    process.execPath,
    [
      path.join(__dirname, 'mac-update-requirements.js'),
      appPath,
      path.join(directory, `${name}.macos.json`),
    ],
    { env: { ...process.env, MACOSX_DEPLOYMENT_TARGET: deploymentTarget }, stdio: 'pipe' }
  );
}

function generateManifest(directory) {
  execFileSync(
    process.execPath,
    [
      path.join(__dirname, 'generate-mac-update-manifest.js'),
      '--version',
      'v1.51.0',
      '--directory',
      directory,
    ],
    { stdio: 'pipe' }
  );
}

for (const {
  name,
  appMinimum,
  backendMinimum,
  intelMinimum,
  minimumMacOS,
  blockedDarwin,
  allowedDarwin,
} of [
  {
    name: 'still allows macOS 12 for a compatible release',
    appMinimum: '12.0',
    backendMinimum: '12.0',
    intelMinimum: '12.0',
    minimumMacOS: '12.0.0',
    blockedDarwin: '20.6.0',
    allowedDarwin: '21.0.0',
  },
  {
    name: 'blocks macOS 12 when Electron requires macOS 13',
    appMinimum: '13.0',
    backendMinimum: '12.0',
    intelMinimum: '13.0',
    minimumMacOS: '13.0.0',
    blockedDarwin: '21.6.0',
    allowedDarwin: '22.0.0',
  },
  {
    name: 'blocks macOS 12 when the backend requires macOS 13',
    appMinimum: '12.0',
    backendMinimum: '13.0',
    intelMinimum: '12.0',
    minimumMacOS: '13.0.0',
    blockedDarwin: '21.6.0',
    allowedDarwin: '22.0.0',
  },
  {
    name: 'honors the stricter Intel bundle requirement',
    appMinimum: '12.0',
    backendMinimum: '12.0',
    intelMinimum: '13.0',
    minimumMacOS: '13.0.0',
    blockedDarwin: '21.6.0',
    allowedDarwin: '22.0.0',
  },
  {
    name: 'handles the macOS 26 naming change',
    appMinimum: '26.0',
    backendMinimum: '13.0',
    intelMinimum: '26.0',
    minimumMacOS: '26.0.0',
    blockedDarwin: '24.6.0',
    allowedDarwin: '25.0.0',
  },
]) {
  test(name, { skip: process.platform !== 'darwin' }, async (t) => {
    const directory = workspace(t);
    recordBundle(directory, 'Goose.zip', appMinimum, backendMinimum);
    recordBundle(directory, 'Goose_intel_mac.zip', intelMinimum, backendMinimum);
    generateManifest(directory);

    const updateInfo = parseUpdateInfo(
      fs.readFileSync(path.join(directory, 'latest-mac.yml'), 'utf8'),
      'latest-mac.yml',
      new URL('https://example.invalid/latest-mac.yml')
    );
    const updater = new AppUpdater(undefined, { version: '1.50.0' });
    updater.logger = null;
    t.mock.method(os, 'release', () => blockedDarwin);
    assert.equal(await updater.isUpdateSupported(updateInfo), false);
    os.release.mock.mockImplementation(() => allowedDarwin);
    assert.equal(await updater.isUpdateSupported(updateInfo), true);

    const fallbackMetadata = JSON.parse(
      fs.readFileSync(path.join(directory, 'mac-update-requirements.json'))
    );
    assert.equal(fallbackMetadata.version, updateInfo.version);
    assert.equal(fallbackMetadata.minimumMacOSVersion, minimumMacOS);
  });
}

test('does not publish a manifest if one architecture has no compatibility metadata', (t) => {
  const directory = workspace(t);
  fs.writeFileSync(
    path.join(directory, 'Goose.zip.macos.json'),
    JSON.stringify({ minimumMacOSVersion: '12.0.0' })
  );
  for (const name of ['Goose.zip', 'Goose_intel_mac.zip']) {
    fs.writeFileSync(path.join(directory, name), 'archive fixture');
  }
  assert.throws(() => generateManifest(directory));
  assert.equal(fs.existsSync(path.join(directory, 'latest-mac.yml')), false);
  assert.equal(fs.existsSync(path.join(directory, 'mac-update-requirements.json')), false);
});

test(
  'stops packaging rather than silently rounding down unsupported OS minima',
  { skip: process.platform !== 'darwin' },
  (t) => {
    for (const minimum of ['13.1', '13.0.1', '16.0']) {
      const directory = workspace(t);
      assert.throws(() => recordBundle(directory, 'Goose.zip', minimum, '12.0'));
      assert.equal(fs.existsSync(path.join(directory, 'Goose.zip.macos.json')), false);
    }
  }
);
