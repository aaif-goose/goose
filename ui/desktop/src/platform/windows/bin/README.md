# Windows-Specific Runtime Files

This directory contains Windows-specific scripts that are only included during Windows builds.

## Components

### Node.js Installation

`npx.cmd` uses the first Node.js that fits, in this order:

1. Portable Node.js that Goose already downloaded to `%LOCALAPPDATA%\Goose\node`.
2. The first `node.exe` on `PATH` that is version 22 or newer and has `npx.cmd` next to it.
3. Otherwise, it downloads portable Node.js to `%LOCALAPPDATA%\Goose\node`.

### Windows Binaries

- `uv.exe` and `uvx.exe` are downloaded from the pinned Astral uv release during packaging.
- Compiled `.exe` and `.dll` files are generated or fetched during the build and are not committed.

## Build Process

Run `node scripts/prepare-platform-binaries.js` from `ui/desktop`. It copies the
Windows command wrappers to `src/bin` and downloads the pinned uv binaries.

The command wrappers in this directory are committed source files; downloaded
binaries and staged copies in `src/bin` are build artifacts.
