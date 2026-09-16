const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { macUpdateRequirements } = require('./mac-update-requirements');

for (const [appMinimum, deploymentTarget, macOS, darwin] of [
  ['12.0', '12.0', '12.0.0', '21.0.0'],
  ['13.0', '12.0', '13.0.0', '22.0.0'],
  ['12.0', '13.0', '13.0.0', '22.0.0'],
  ['26.0', '13.0', '26.0.0', '25.0.0'],
]) {
  test(`combines Electron ${appMinimum} and backend ${deploymentTarget} requirements`, () => {
    assert.deepEqual(macUpdateRequirements(appMinimum, deploymentTarget), {
      minimumMacOSVersion: macOS,
      minimumSystemVersion: darwin,
    });
  });
}

test('rejects unknown mappings and minor-release minima instead of publishing an unsafe floor', () => {
  for (const minimum of [undefined, '', 'invalid', '13.1', '13.0.1', '16.0']) {
    assert.throws(() => macUpdateRequirements(minimum, '12.0'));
  }
});

test('publishes the stricter architecture minimum in both manifests and preserves ZIP checksums', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-manifest-test-'));
  try {
    for (const [name, minimum] of [
      ['Goose.zip', '12.0'],
      ['Goose_intel_mac.zip', '13.0'],
    ]) {
      fs.writeFileSync(path.join(directory, name), `fixture for ${name}`);
      fs.writeFileSync(
        path.join(directory, `${name}.macos.json`),
        JSON.stringify(macUpdateRequirements(minimum, '12.0'))
      );
    }
    execFileSync(process.execPath, [
      path.join(__dirname, 'generate-mac-update-manifest.js'),
      '--version',
      'v1.51.0',
      '--directory',
      directory,
    ]);
    const manifest = fs.readFileSync(path.join(directory, 'latest-mac.yml'), 'utf8');
    assert.match(manifest, /minimumSystemVersion: "22.0.0"/);
    assert.match(manifest, /version: "1.51.0"/);
    for (const name of ['Goose-darwin-arm64.zip', 'Goose-darwin-x64.zip']) {
      const hash = crypto
        .createHash('sha512')
        .update(fs.readFileSync(path.join(directory, name)))
        .digest('base64');
      assert.ok(manifest.includes(`sha512: "${hash}"`));
    }
    assert.deepEqual(
      JSON.parse(fs.readFileSync(path.join(directory, 'mac-update-requirements.json'))),
      {
        version: '1.51.0',
        minimumMacOSVersion: '13.0.0',
        minimumSystemVersion: '22.0.0',
      }
    );
    fs.unlinkSync(path.join(directory, 'Goose.zip.macos.json'));
    assert.throws(() =>
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
      )
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('electron-updater rejects macOS 12 and accepts macOS 13 with the generated minimum', async (t) => {
  const { AppUpdater } = require('electron-updater/out/AppUpdater');
  const updater = new AppUpdater(undefined, { version: '1.50.0' });
  updater.logger = null;
  const requirements = macUpdateRequirements('13.0', '12.0');
  t.mock.method(os, 'release', () => '21.6.0');
  assert.equal(await updater.isUpdateSupported(requirements), false);
  os.release.mock.mockImplementation(() => '22.0.0');
  assert.equal(await updater.isUpdateSupported(requirements), true);
});

test('reads the packaged app minimum with plutil', { skip: process.platform !== 'darwin' }, () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-plist-test-'));
  try {
    const appPath = path.join(directory, 'Goose.app');
    fs.mkdirSync(path.join(appPath, 'Contents'), { recursive: true });
    fs.writeFileSync(
      path.join(appPath, 'Contents', 'Info.plist'),
      '<?xml version="1.0"?><plist version="1.0"><dict><key>LSMinimumSystemVersion</key><string>13.0</string></dict></plist>'
    );
    const output = path.join(directory, 'Goose.zip.macos.json');
    execFileSync(
      process.execPath,
      [path.join(__dirname, 'mac-update-requirements.js'), appPath, output],
      { env: { ...process.env, MACOSX_DEPLOYMENT_TARGET: '12.0' } }
    );
    assert.deepEqual(JSON.parse(fs.readFileSync(output)), {
      minimumMacOSVersion: '13.0.0',
      minimumSystemVersion: '22.0.0',
    });
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});
