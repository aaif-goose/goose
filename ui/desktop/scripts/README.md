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

