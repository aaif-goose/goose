# Goose2 distro bundles

A Goose2 distro bundle is an optional app-specific package of configuration and policy that the Tauri shell loads at startup.

## What a distro bundle is

A distro bundle lives under `ui/goose2/distro/` in development, and is bundled into the packaged app as a Tauri resource in production.

Current supported files:

- `distro.json` — distro manifest
- `config.yaml` — optional Goose config passed to `goose serve`
- `bin/` — optional executables or helper scripts prepended to `PATH` for `goose serve`

## How it is discovered

The Tauri app resolves the distro bundle in this order:

1. `GOOSE_DISTRO_DIR`, if set
2. bundled Tauri resource dir at `resource_dir()/distro`

In development, `just dev` and `just dev-debug` automatically export `GOOSE_DISTRO_DIR` to `ui/goose2/distro` when that directory exists.

## Manifest shape

Example:

```json
{
  "appVersion": "development",
  "featureToggles": {
    "costTracking": false
  },
  "security": {
    "providerAllowlist": "databricks"
  }
}
```

### Fields

- `appVersion?: string`
  - optional app version tag supplied by the distro

- `featureToggles?: Record<string, boolean>`
  - optional UI/product flags controlled by the distro
  - currently supported:
    - `costTracking`
      - `false` hides cost UI in the token/context usage surfaces
      - omitted behaves as enabled

- `security?: { providerAllowlist?: string, extensionAllowlist?: string }`
  - optional policy controls
  - currently used:
    - `providerAllowlist`
      - comma-separated provider ids
      - limits visible model providers in Settings
      - limits visible Goose model options in the chat model picker

## Runtime effects

When a distro bundle is present, Goose2 does two kinds of things with it.

### Frontend behavior

The frontend loads `get_distro_bundle` during app startup and stores the manifest in Zustand.

Today it uses that manifest to:

- filter model providers shown in provider settings via `providerAllowlist`
- filter Goose model options shown in the chat input model picker via `providerAllowlist`
- hide cost UI when `featureToggles.costTracking === false`

### Backend / shell behavior

When the Tauri shell launches the long-lived `goose serve` process, it applies the distro bundle like this:

- prepends `distro/bin` to `PATH` when present
- adds `distro/config.yaml` to `GOOSE_ADDITIONAL_CONFIG_FILES` when present
- sets `GOOSE_DISTRO_DIR` to the resolved distro root

This is shell-level behavior, so it is implemented as Tauri-side setup rather than an ACP method.

## Development notes

- packaged apps discover distro content from bundled Tauri resources
- local development uses `GOOSE_DISTRO_DIR`
- after changing `distro.json`, restart `just dev` so startup reloads the manifest

## Scope guidance

Use distro bundles for packaged-app policy and shell-level defaults.

Good fits:

- feature flags for Goose2 UI behavior
- allowlists that constrain visible product choices
- config or helper binaries that should be present when `goose serve` starts

Avoid using distro bundles as a replacement for normal app state, user settings, or ACP-backed domain data.
