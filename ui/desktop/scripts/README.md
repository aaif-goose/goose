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

# Building the goose SDK

`build-goose-sdk.js` builds `@aaif/goose-sdk` (`ui/sdk`) and is what `postinstall` and `start-gui`
call. It skips the build when the SDK's inputs — the ACP schemas, `ui/sdk/src` (bar
`src/generated`, which the build writes), its tsconfig and package manifest, and the lockfile —
hash the same as the last successful build, so a launch that changed nothing does no work. If one
of those inputs is saved while a build is running, the stamp is dropped rather than written, so the
next launch rebuilds instead of skipping over an edit that never reached `dist`.

`package` and `make` call it with `--force` instead, and should keep doing so. The input list above
is a hand-maintained restatement of what the build reads; nothing enforces that it stays complete
as the SDK grows. Skipping a launch's rebuild on a stale list costs seconds; skipping an artifact's
costs a UI whose generated ACP dispatch disagrees with the backend schema, which `typecheck` will
not catch because it typechecks against the stale generated types. Ten seconds on a run that
already spends minutes on Rust and signing is not a trade worth making.

To rebuild anyway:

```bash
pnpm run build-goose-sdk:force   # or: node scripts/build-goose-sdk.js --force
```

`GOOSE_SDK_FORCE_BUILD=1` does the same thing, for callers that can only set an environment
variable.

# Vite directories

Nothing here clears either of them, and both omissions are deliberate.

`node_modules/.vite` is the dependency-optimizer cache. It is left alone — including after an SDK
rebuild. `vite.renderer.config.mts` excludes `@aaif/goose-sdk` from `optimizeDeps`, so the dev
server never holds a pre-bundled copy of it to go stale, and refilling that cache costs tens of
seconds per launch.

`.vite` in the package root is Electron Forge's build output, and the Vite plugin builds it with
`emptyOutDir: false` — but the plugin's own `preStart` and `prePackage` hooks `remove()` the whole
directory first, so a file that stops being emitted cannot survive into a `start`, a `package` or a
`make`. A cleanup step here would be doing nothing except adding a way for the recursive delete to
fail the run.
