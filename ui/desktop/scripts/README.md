# Goosey

Put `goosey` in your $PATH if you want to launch via:

```
goosey .
```

This will open goose GUI from any path you specify

# Unregister Deeplink Protocols (macos only)

`unregister-deeplink-protocols.js` is a script to unregister the deeplink protocol used by goose like `goose://`.
This is handy when you want to test deeplinks with the development version of Goose.

# Usage

To unregister the deeplink protocols, run the following command in your terminal:
Then launch Goose again and your deeplinks should work from the latest launched goose application as it is registered on startup.

```bash
node scripts/unregister-deeplink-protocols.js
```

# Building the Goose ACP client

`build-goose-acp-client.js` builds `@aaif/goose-acp-client` (`ui/goose-acp-client`).
Both `postinstall` and `start-gui` call it. It skips unchanged builds by hashing the
ACP schemas, generator, handwritten sources, TypeScript configuration, package
manifest, and workspace lockfile. Generated sources are build outputs and are excluded.

A failed build or an input change during the build leaves no stamp, so the next
launch rebuilds. The stamp lives in `node_modules/.cache`; a missing `dist/index.js`
also triggers a build.

`package`, `make`, and `scripts/build-windows.ps1` always force a rebuild. The input
list is maintained manually, so packaging must not rely on it to detect every change.
A forced build fails if its inputs change while it runs.

To force a development rebuild:

```bash
pnpm run build-goose-acp-client:force
```

`GOOSE_ACP_CLIENT_FORCE_BUILD=1` has the same effect. Use either option if generated
sources or files in `dist` were edited or partially deleted.

# Vite directories

`node_modules/.vite` is Vite's dependency cache and survives client builds.
`vite.renderer.config.mts` excludes `@aaif/goose-acp-client` from `optimizeDeps`, so
Vite reads the client build directly.

The package-root `.vite` directory contains Electron Forge's build output. The Vite
plugin clears it in its `preStart` and `prePackage` hooks, so a separate cleanup step
is unnecessary.
