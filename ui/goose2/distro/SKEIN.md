# Skein distro bundle notes

This directory is the standard goose2 distro bundle; the upstream contract is
documented in [README.md](README.md). What follows is *Skein-specific* — what
we use the bundle for in this distribution, and what we deliberately do not.

## What Skein puts here

- `distro.json`
  - `appVersion: "skein-<phase tag>"` — identifies builds.
  - `featureToggles.costTracking: true` — kept on for testers who run their
    own LLMs and want to see token usage during multi-conversation tests.
- `config.yaml`
  - Phase 0: empty by design. See the file's header comment for the rationale.
- `bin/`
  - Phase 0: not used.

## What Skein deliberately does *not* put here

- **Recipe definitions.** Recipes live at the repo top level under `recipes/`,
  not in the distro bundle. They are versioned, security-scanned, and shipped
  to users as data, not bundled into the desktop app.
- **eval-bench config.** eval-bench reads recipe-local artifacts and writes
  to `~/.skein/eval-bench.sqlite`. The distro bundle is shell-level policy;
  eval-bench is product behaviour.
- **Locked-down provider allowlists.** We resist this until we have a real
  reason. Skein is testing-first; testers pick the providers they need.

## When to add to this bundle

Three triggers, in priority order:

1. A field-observed footgun — every Skein user has been surprised by the
   same default in the same direction. Encode the desired default here
   *with a comment that names the observation*.
2. A compliance or security baseline that a Skein installation is required
   to honour by some clear policy.
3. A toggle that several Skein users have asked for but is too narrow to
   merge upstream into goose proper.

If a candidate addition does not fit one of those three, it does not belong
in the distro bundle.
