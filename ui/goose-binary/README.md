# Native Binary Packages for goose

This directory contains the npm package scaffolding for distributing the
`goose` Rust binary as platform-specific npm packages.

## Packages

| Package | Platform |
|---------|----------|
| `@aaif/goose-binary-darwin-arm64` | macOS Apple Silicon |
| `@aaif/goose-binary-darwin-x64` | macOS Intel |
| `@aaif/goose-binary-linux-arm64` | Linux ARM64 |
| `@aaif/goose-binary-linux-x64` | Linux x64 |
| `@aaif/goose-binary-win32-x64` | Windows x64 |

## Building

From the repository root:

```bash
# Build for current platform only
cd ui/acp
npm run build:native

# Build for all platforms (requires cross-compilation toolchains)
npm run build:native:all

# Build for specific platform(s)
npx tsx scripts/build-native.ts darwin-arm64 linux-x64
```

The built binaries are placed into `ui/goose-binary/goose-binary-{platform}/bin/`.
These directories are git-ignored.

## Publishing

Publishing is handled by GitHub Actions. See `.github/workflows/publish-npm.yml`.

For manual publishing:

```bash
# From repository root
./ui/scripts/publish.sh --real
```

This will publish all native packages along with `@aaif/goose-acp` and `@aaif/goose`.

## Usage

These packages are installed as optional dependencies by `@aaif/goose` (the TUI).
The appropriate package for the user's platform is automatically selected during
installation.

See `ui/text/scripts/postinstall.mjs` for how the binary path is resolved.
