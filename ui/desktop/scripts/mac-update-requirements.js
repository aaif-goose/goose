#!/usr/bin/env node

const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

function macUpdateRequirements(appMinimum, deploymentTarget) {
  const versions = [appMinimum, deploymentTarget].map((version) => {
    if (!/^\d+\.0(?:\.0)?$/.test(version)) {
      throw new Error(`Expected a macOS major-release minimum, got ${version}`);
    }
    return Number(version.split('.')[0]);
  });
  const major = Math.max(...versions);
  // Darwin 20–24 correspond to macOS 11–15; macOS switched to year-based names at 26.
  const darwinMajor = major >= 26 ? major - 1 : major >= 11 && major <= 15 ? major + 9 : null;
  if (darwinMajor === null) {
    throw new Error(`Unknown Darwin version for macOS ${major}`);
  }
  return {
    minimumMacOSVersion: `${major}.0.0`,
    minimumSystemVersion: `${darwinMajor}.0.0`,
  };
}

if (require.main === module) {
  const [appPath, outputPath] = process.argv.slice(2);
  const appMinimum = execFileSync(
    '/usr/bin/plutil',
    ['-extract', 'LSMinimumSystemVersion', 'raw', path.join(appPath, 'Contents', 'Info.plist')],
    { encoding: 'utf8' }
  ).trim();
  const requirements = macUpdateRequirements(appMinimum, process.env.MACOSX_DEPLOYMENT_TARGET);
  fs.writeFileSync(outputPath, `${JSON.stringify(requirements)}\n`);
}

module.exports = { macUpdateRequirements };
