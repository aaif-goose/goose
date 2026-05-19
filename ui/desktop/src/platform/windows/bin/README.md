# Windows-Specific Runtime Files

This directory contains Windows-specific scripts that are only included during Windows builds.

## Components

### Node.js Installation
- `install-node.cmd` - Script to check for and install Node.js if needed
- `npx.cmd` - Wrapper script that ensures Node.js is installed and uses system npx

### Windows Binaries
- `uv.exe` and `uvx.exe` are downloaded from the pinned Astral uv release during packaging.
- Compiled `.exe` and `.dll` files are generated or fetched during the build and are not committed.

## Build Process

Windows runtime files are prepared during the build process by:
1. `prepare-windows-npm.sh` - Creates Node.js installation scripts
2. `prepare-platform-binaries.js` - Downloads pinned uv binaries and copies Windows-specific files to `src/bin`

None of these files should be committed to the repository - they are generated fresh during each Windows build.
