import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Guards against a second copy of any @radix-ui package entering the tree.
 *
 * Radix primitives keep cross-cutting state in MODULE-LEVEL variables, not in
 * React state. The worst offender is DismissableLayer, which owns the
 * `pointer-events: none` that goes on <body> while a modal is open, plus the
 * Sets tracking which layers are currently open, plus the saved "original"
 * body value to restore on close — all module globals.
 *
 * Two copies of that module means two independent bookkeepers writing to one
 * shared document.body, each blind to the other. Copy A opens a layer and sets
 * body to `none`; copy B then opens one, sees its OWN layer Set is empty, and
 * saves the value it currently reads — `none` — as "the original". From then on
 * copy B restores `none` on every close. The style is stranded, every click in
 * the app is silently swallowed, and nothing short of a reload recovers it: the
 * window looks completely normal, the renderer is idle, and the JS console
 * still works, so it does not present as a hang.
 *
 * That is what #11762 reported. `@radix-ui/themes` (declared but never
 * imported) pulled in the `radix-ui` umbrella package, whose older pinned
 * primitives got hoisted to the top level, where the app's *undeclared*
 * imports of react-dropdown-menu and react-tooltip resolved to them — while
 * the declared react-dialog used the modern tree. Two dismissable-layer
 * copies, both bundled into the renderer.
 *
 * Radix pins its internal deps to EXACT versions, so any dependency that
 * bundles its own primitives reintroduces this instantly and invisibly. This
 * test reads the lockfile rather than node_modules so it fails in CI on the
 * commit that introduces the skew, not months later on someone's frozen UI.
 */

function findLockfile(): string {
  let dir = process.cwd();
  for (let i = 0; i < 6; i++) {
    const candidate = join(dir, 'pnpm-lock.yaml');
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(`pnpm-lock.yaml not found at or above ${process.cwd()}`);
}

function radixVersionsByPackage(): Map<string, Set<string>> {
  const lockfile = readFileSync(findLockfile(), 'utf8');
  const versions = new Map<string, Set<string>>();

  for (const [, name, version] of lockfile.matchAll(/@radix-ui\/([a-z0-9-]+)@(\d+\.\d+\.\d+)/g)) {
    const seen = versions.get(name) ?? new Set<string>();
    seen.add(version);
    versions.set(name, seen);
  }

  return versions;
}

describe('@radix-ui dependency deduplication', () => {
  it('resolves every @radix-ui package to exactly one version', () => {
    const duplicated = [...radixVersionsByPackage()]
      .filter(([, versions]) => versions.size > 1)
      .map(([name, versions]) => `@radix-ui/${name}: ${[...versions].sort().join(', ')}`)
      .sort();

    // Reported as a list so a failure names every offender at once, and points
    // at the fix: add a pnpm override in ui/pnpm-workspace.yaml, or declare the
    // package directly in desktop/package.json so it stops being satisfied by
    // whatever version some other dependency happened to hoist.
    expect(duplicated).toEqual([]);
  });

  it('resolves react-dismissable-layer to a single version at or above 1.1.19', () => {
    const versions = radixVersionsByPackage().get('react-dismissable-layer');
    expect(versions, 'react-dismissable-layer missing from the lockfile').toBeDefined();
    expect([...versions!]).toHaveLength(1);

    // 1.1.11 and earlier delete the layer from the tracking Set in a *different*
    // useEffect than the one restoring the body style, and gate the restore on
    // `size === 1` read before that delete lands. 1.1.19 made the two atomic in
    // a single cleanup, which is what actually fixes the stranding.
    const [major, minor, patch] = [...versions!][0].split('.').map(Number);
    expect(major * 1_000_000 + minor * 1_000 + patch).toBeGreaterThanOrEqual(1_001_019);
  });

  it('does not pull in the radix-ui umbrella package, which bundles its own primitives', () => {
    const lockfile = readFileSync(findLockfile(), 'utf8');
    // A bare `radix-ui@x.y.z` key (as opposed to the scoped @radix-ui/* ones)
    // is the umbrella re-export. It pins a full set of primitives at its own
    // exact versions, so adding it back guarantees a duplicate tree.
    expect(lockfile).not.toMatch(/^ {2}radix-ui@\d/m);
  });
});
