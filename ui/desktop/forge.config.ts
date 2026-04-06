const { FusesPlugin } = require('@electron-forge/plugin-fuses');
const { FuseV1Options, FuseVersion } = require('@electron/fuses');
const { resolve } = require('path');

let cfg = {
  asar: true,
  extraResource: ['src/bin', 'src/images'],
  icon: 'src/images/icon',
  // Windows specific configuration
  win32: {
    icon: 'src/images/icon.ico',
    certificateFile: process.env.WINDOWS_CERTIFICATE_FILE,
    signingRole: process.env.WINDOW_SIGNING_ROLE,
    rfc3161TimeStampServer: 'http://timestamp.digicert.com',
    signWithParams: '/fd sha256 /tr http://timestamp.digicert.com /td sha256',
  },
  // Protocol registration
  protocols: [
    {
      name: 'GooseProtocol',
      schemes: ['goose'],
    },
  ],
  // macOS Info.plist extensions for drag-and-drop support
  extendInfo: {
    // Document types for drag-and-drop support onto dock icon
    CFBundleDocumentTypes: [
      {
        CFBundleTypeName: 'Folders',
        CFBundleTypeRole: 'Viewer',
        LSHandlerRank: 'Alternate',
        LSItemContentTypes: ['public.directory', 'public.folder'],
      },
    ],
    // Usage descriptions for macOS TCC (Transparency, Consent, and Control)
    NSCalendarsUsageDescription:
      'Goose needs access to your calendars to help manage and query calendar events.',
    NSRemindersUsageDescription:
      'Goose needs access to your reminders to help manage and query reminders.',
  },
};

// macOS code signing and notarization via Electron Forge
// Activated when APPLE_TEAM_ID is set (CI signing builds)
if (process.env.APPLE_TEAM_ID) {
  cfg.osxSign = {
    keychain: process.env.KEYCHAIN_PATH || undefined,
    entitlements: 'entitlements.plist',
    'entitlements-inherit': 'entitlements.plist',
  };
  cfg.osxNotarize = {
    appleId: process.env.APPLE_ID,
    appleIdPassword: process.env.APPLE_ID_PASSWORD,
    teamId: process.env.APPLE_TEAM_ID,
  };
}

module.exports = {
  packagerConfig: cfg,
  rebuildConfig: {},
  publishers: [
    {
      name: '@electron-forge/publisher-github',
      config: {
        repository: {
          owner: process.env.GITHUB_OWNER || 'block',
          name: process.env.GITHUB_REPO || 'goose',
        },
        prerelease: false,
        draft: true,
      },
    },
  ],
  makers: [
    {
      name: '@electron-forge/maker-zip',
      platforms: ['darwin', 'win32', 'linux'],
      config: {
        arch: process.env.ELECTRON_ARCH === 'x64' ? ['x64'] : ['arm64'],
        options: {
          icon: 'src/images/icon.ico',
        },
      },
    },
    {
      name: '@electron-forge/maker-deb',
      config: {
        name: 'Goose',
        bin: 'Goose',
        maintainer: 'Block, Inc.',
        homepage: 'https://block.github.io/goose/',
        categories: ['Development'],
        desktopTemplate: './forge.deb.desktop',
        options: {
          icon: 'src/images/icon.png',
          prefix: '/opt',
        },
      },
    },
    {
      name: '@electron-forge/maker-rpm',
      config: {
        name: 'Goose',
        bin: 'Goose',
        maintainer: 'Block, Inc.',
        homepage: 'https://block.github.io/goose/',
        categories: ['Development'],
        desktopTemplate: './forge.rpm.desktop',
        options: {
          icon: 'src/images/icon.png',
          prefix: '/opt',
          fpm: ['--rpm-rpmbuild-define', '_build_id_links none'],
        },
      },
    },
    {
      name: '@electron-forge/maker-flatpak',
      config: {
        options: {
          id: 'io.github.block.Goose',
          categories: ['Development'],
          icon: {
            scalable: 'src/images/icon.svg',
            '512x512': 'src/images/icon-512.png',
          },
          homepage: 'https://block.github.io/goose/',
          runtimeVersion: '25.08',
          baseVersion: '25.08',
          bin: 'Goose',
          modules: [
            {
              name: 'libbz2-shim',
              buildsystem: 'simple',
              'build-commands': [
                // Create the lib directory in the app bundle
                'mkdir -p /app/lib',
                // Point to the actual library in the 25.08 runtime
                // We use a wildcard to handle multi-arch paths (x86_64-linux-gnu, etc)
                'ln -s $(find /usr/lib -name "libbz2.so.1" | head -n 1) /app/lib/libbz2.so.1.0',
              ],
            },
          ],
          finishArgs: [
            '--share=ipc',
            '--socket=x11',
            '--socket=wayland',
            '--device=dri',
            '--share=network',
            '--filesystem=home',
            '--talk-name=org.freedesktop.Notifications',
            '--socket=session-bus',
            '--socket=system-bus',
            // This ensures the app looks in our shim folder first
            '--env=LD_LIBRARY_PATH=/app/lib',
          ],
        },
      },
    },
  ],
  plugins: [
    {
      name: '@electron-forge/plugin-vite',
      config: {
        build: [
          {
            entry: 'src/main.ts',
            config: 'vite.main.config.mts',
          },
          {
            entry: 'src/preload.ts',
            config: 'vite.preload.config.mts',
          },
        ],
        renderer: [
          {
            name: 'main_window',
            config: 'vite.renderer.config.mts',
          },
        ],
      },
    },
    // Fuses are used to enable/disable various Electron functionality
    // at package time, before code signing the application
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};
