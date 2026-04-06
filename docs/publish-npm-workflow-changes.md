# publish-npm.yml Workflow Changes

## Summary

Successfully simplified the `publish-npm.yml` workflow by removing the broken changesets action and replacing it with direct `pnpm publish` commands.

## Changes Made

### 1. Removed Changesets Action
**Problem:** The changesets action never worked - all runs failed with "There is no .changeset directory in this project" error, even though the directory existed.

**Root Cause:** The `cwd` parameter in the changesets action wasn't working correctly, and the action was checking for `.changeset` in the repo root before applying the working directory.

**Solution:** Removed changesets entirely and replaced with simple `pnpm publish -r` command.

### 2. Simplified Publishing Logic

**Before:**
```yaml
- name: Create Release Pull Request or Publish to npm
  if: inputs.dry-run != true && github.ref == 'refs/heads/main'
  uses: changesets/action@...
  with:
    publish: pnpm run release
    version: pnpm run version
    cwd: ui
```

**After:**
```yaml
- name: Publish to npm
  if: inputs.dry-run != true && github.ref == 'refs/heads/main'
  run: |
    cd ui
    pnpm publish -r --access public --no-git-checks

- name: Dry run - Show what would be published
  if: inputs.dry-run == true || github.ref != 'refs/heads/main'
  run: |
    cd ui
    # List all packages with their versions
    for pkg in acp text goose-binary/*/; do
      if [ -f "$pkg/package.json" ]; then
        name=$(jq -r '.name' "$pkg/package.json")
        version=$(jq -r '.version' "$pkg/package.json")
        echo "- $name@$version"
      fi
    done
```

### 3. Dry-Run Now Only Prevents Publish

**Before:** Dry-run would skip the entire changesets step, preventing testing of the workflow logic.

**After:** Dry-run runs all the same steps but shows what would be published instead of actually publishing. This allows full workflow testing without publishing to npm.

### 4. Temporarily Disabled Windows Build

**Reason:** Windows builds take 20+ minutes when cache misses, slowing down testing.

**Solution:** Commented out Windows from the build matrix. Will re-enable after implementing sccache optimization (see `docs/ci-optimization-windows-builds.md`).

## Testing Results

### Successful Run: 23666117886

**Duration:** ~5 minutes (all builds had cache hits)

**Packages Detected:**
- @aaif/goose-acp@0.1.0
- @aaif/goose@0.1.0
- @aaif/goose-binary-darwin-arm64@0.1.0
- @aaif/goose-binary-darwin-x64@0.1.0
- @aaif/goose-binary-linux-arm64@0.1.0
- @aaif/goose-binary-linux-x64@0.1.0
- @aaif/goose-binary-win32-x64@0.1.0

**Status:** ✅ All steps completed successfully

## How to Use

### Test the Workflow (Dry-Run)
```bash
gh workflow run publish-npm.yml --ref <branch> -f dry-run=true
```

This will:
1. Generate ACP schema
2. Build goose binaries for all platforms (except Windows, temporarily)
3. Build npm packages
4. Show what would be published (without actually publishing)

### Publish for Real (Main Branch Only)
```bash
# Merge to main, then:
gh workflow run publish-npm.yml --ref main -f dry-run=false
```

This will:
1. Run all build steps
2. Actually publish to npm with `pnpm publish -r`

**Note:** Publishing only works from the `main` branch due to security restrictions.

## Versioning Strategy

Since we removed changesets, you'll need to manage versions manually:

### Option 1: Manual Version Bumps
```bash
cd ui/acp
npm version patch  # or minor, major

cd ../text
npm version patch

# etc for each package
```

### Option 2: Use Changesets CLI Manually
```bash
cd ui
pnpm changeset add  # Create a changeset
pnpm changeset version  # Bump versions based on changesets
git commit -am "chore: version packages"
```

Then trigger the workflow to publish.

### Option 3: Add Version Bump to Workflow
Could add a step to automatically bump versions based on conventional commits or other logic.

## Future Improvements

### 1. Re-enable Windows Build with sccache
See `docs/ci-optimization-windows-builds.md` for implementation details.

**Expected improvement:** 20+ minutes → 8-10 minutes for cache misses

### 2. Add Automatic Version Bumping
Options:
- Use conventional commits to determine version bump
- Add a workflow input for version bump type (patch/minor/major)
- Integrate changesets CLI properly (without the action)

### 3. Add Release Notes Generation
Could generate release notes from git commits or changeset files.

### 4. Add npm Publish Verification
After publishing, verify packages are available on npm registry.

## Troubleshooting

### "Package already exists" Error
If you try to publish a version that already exists on npm:
```bash
# Bump the version first
cd ui/acp
npm version patch
git commit -am "chore: bump version"
git push
```

### Cache Issues
If builds are slow due to cache misses:
```bash
# Skip cache and rebuild everything
gh workflow run publish-npm.yml -f skip-cache=true
```

### Testing Without Publishing
Always use dry-run mode for testing:
```bash
gh workflow run publish-npm.yml -f dry-run=true
```

## Related Documentation

- `docs/ci-optimization-windows-builds.md` - Strategies to speed up Windows builds
- `.github/workflows/publish-npm.yml` - The workflow file
- `ui/package.json` - Workspace configuration
- `ui/.changeset/` - Changeset configuration (currently unused by workflow)
