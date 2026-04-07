# CLI Command Tracking

Automated pipeline for detecting and documenting CLI command changes between goose releases.

## Overview

This automation keeps the [CLI Commands Guide](https://goose-docs.ai/docs/guides/goose-cli-commands) synchronized with code changes by:

1. **Extracting** CLI structure from goose binary using `--help` output (deterministic)
2. **Detecting** changes between versions (deterministic diff)
3. **Synthesizing** human-readable change documentation (AI-powered)
4. **Updating** the CLI Commands Guide (AI-powered)

The automation runs automatically on new releases via GitHub Actions, or can be run manually for testing.

## Quick Start

### Automated (GitHub Actions)

The automation runs automatically when a new release is published. See [TESTING.md](./TESTING.md) for testing instructions.

### Manual (Local Testing)

```bash
# Set the goose repository path
export GOOSE_REPO=/path/to/goose

# Run the complete pipeline with auto-detected versions
./scripts/run-pipeline.sh

# Or specify versions explicitly
./scripts/run-pipeline.sh v1.17.0 v1.19.0

# Or run individual steps:
# 1. Extract CLI structures
./scripts/extract-cli-structure.sh v1.17.0 > output/old-cli-structure.json
./scripts/extract-cli-structure.sh v1.19.0 > output/new-cli-structure.json

# 2. Detect changes
python3 scripts/diff-cli-structures.py output/old-cli-structure.json \
                                       output/new-cli-structure.json \
                                       > output/cli-changes.json

# 3. Generate human-readable change documentation
cd output && goose run --recipe ../recipes/synthesize-cli-changes.yaml

# 4. Update goose-cli-commands.md
cd output && goose run --recipe ../recipes/update-cli-commands.yaml
```

### Version Detection

The pipeline automatically detects versions when not specified:
- **Old version**: Second-most-recent release tag (via `gh release list`)
- **New version**: Most recent release tag, or `RELEASE_TAG` env var (for CI)
- **Fallback**: Uses git tags if `gh` CLI not available

To test unreleased changes, explicitly pass `HEAD`:
```bash
./scripts/run-pipeline.sh v1.19.0 HEAD
```

## Architecture

### Modular Pipeline Design

The automation uses a **hybrid approach**: deterministic scripts for data extraction/diffing, AI recipes for analysis and documentation updates.

```
┌─────────────────────────────────────────────────────────────────┐
│                    EXTRACTION (Deterministic)                    │
├─────────────────────────────────────────────────────────────────┤
│  extract-cli-structure.sh → extract-cli-structure.py             │
│  ↓                                                               │
│  cli-structure.json (commands, options, subcommands, aliases)    │
└─────────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                    DIFFING (Deterministic)                       │
├─────────────────────────────────────────────────────────────────┤
│  diff-cli-structures.py                                          │
│  ↓                                                               │
│  cli-changes.json (added, removed, modified commands/options)    │
└─────────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                    SYNTHESIS (AI-Powered)                        │
├─────────────────────────────────────────────────────────────────┤
│  synthesize-cli-changes.yaml                                     │
│  ↓                                                               │
│  cli-changes.md (human-readable)                                 │
└─────────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                    UPDATE (AI-Powered)                           │
├─────────────────────────────────────────────────────────────────┤
│  update-cli-commands.yaml                                        │
│  ↓                                                               │
│  goose-cli-commands.md (updated) + update-summary.md             │
└─────────────────────────────────────────────────────────────────┘
```

### Why This Design?

**Scripts handle deterministic tasks:**
- Building goose from specific git tags
- Running `--help` commands and parsing output
- JSON structure comparison
- No interpretation or inference - direct extraction

**AI recipes handle synthesis and updates:**
- Analyzing changes and explaining implications
- Generating migration guidance and examples
- Updating documentation with proper formatting and context

**Benefits:**
- **Reliability**: Extraction is deterministic and reproducible
- **Testability**: Each stage has clear inputs/outputs
- **Maintainability**: Easy to update individual components
- **Transparency**: Intermediate files can be inspected

### Data Flow

All stages communicate via JSON/Markdown files in the `output/` directory:

| File | Producer | Consumer | Purpose |
|------|----------|----------|---------|
| `old-cli-structure.json` | `extract-cli-structure.sh` | `diff-cli-structures.py` | Previous version CLI structure |
| `new-cli-structure.json` | `extract-cli-structure.sh` | `diff-cli-structures.py` | Current version CLI structure |
| `cli-changes.json` | `diff-cli-structures.py` | `synthesize-cli-changes.yaml` | Detected changes (structured) |
| `cli-changes.md` | `synthesize-cli-changes.yaml` | `update-cli-commands.yaml` | Human-readable change documentation |
| `update-summary.md` | `update-cli-commands.yaml` | Human review | Summary of documentation updates |

## Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `GOOSE_REPO` | Yes (local) | - | Path to goose repository root |
| `CLI_COMMANDS_PATH` | No | `$GOOSE_REPO/documentation/docs/guides/goose-cli-commands.md` | Full path to target doc file |
| `RELEASE_TAG` | No | - | Used by GitHub Actions to specify the new version |

**Example:**
```bash
export GOOSE_REPO=/Users/you/goose
# CLI_COMMANDS_PATH is auto-constructed from GOOSE_REPO
```

### Skipped Commands

Some commands are intentionally excluded from extraction and documentation tracking. These are configured in `config/skip-commands.json`:

```json
{
  "description": "Commands to skip during extraction (not documented intentionally)",
  "skip_commands": [
    {
      "name": "term",
      "reason": "Terminal integration documented via @goose/@g aliases"
    }
  ]
}
```

To add or remove skipped commands, edit the config file - no code changes required.

## Scripts

### `extract-cli-structure.sh`

Builds goose from a specific git tag and extracts CLI structure using `--help` output.

**Usage:**
```bash
./scripts/extract-cli-structure.sh [version] > output/cli-structure.json
```

**Arguments:**
- `version` (optional): Git tag or commit to extract from (default: HEAD)

**Output:** JSON with complete command tree including options, subcommands, aliases

**Example:**
```bash
# Extract from current code
./scripts/extract-cli-structure.sh HEAD > output/new-cli-structure.json

# Extract from specific version
./scripts/extract-cli-structure.sh v1.15.0 > output/old-cli-structure.json
```

### `diff-cli-structures.py`

Compares two CLI structure files and outputs detected changes.

**Usage:**
```bash
python3 scripts/diff-cli-structures.py <old-file> <new-file> > output/cli-changes.json
```

**Arguments:**
- `old-file`: Path to old CLI structure JSON
- `new-file`: Path to new CLI structure JSON

**Output:** JSON with categorized changes:
- `commands.added`: New commands
- `commands.removed`: Deleted commands
- `commands.modified`: Changed commands (options, description, aliases)
- `breaking_changes`: Categorized breaking changes

**Example:**
```bash
python3 scripts/diff-cli-structures.py \
  output/old-cli-structure.json \
  output/new-cli-structure.json \
  > output/cli-changes.json
```

## Recipes

### `synthesize-cli-changes.yaml`

Analyzes detected changes and generates human-readable documentation.

**Inputs:**
- `output/cli-changes.json` - Detected changes from diff script
- `output/old-cli-structure.json` - Previous version structure
- `output/new-cli-structure.json` - Current version structure

**Output:**
- `output/cli-changes.md` - Human-readable change documentation with:
  - Breaking changes with migration guidance
  - New commands with usage examples
  - Modified commands with details
  - Non-breaking changes summary

**Usage:**
```bash
cd output
goose run --recipe ../recipes/synthesize-cli-changes.yaml
```

### `update-cli-commands.yaml`

Updates the CLI Commands Guide based on synthesized changes.

**Inputs:**
- `output/cli-changes.md` - Change documentation from synthesis recipe
- `goose-cli-commands.md` - Target documentation file (path from `CLI_COMMANDS_PATH` or `GOOSE_REPO` env var)

**Outputs:**
- Updated `goose-cli-commands.md` with changes applied
- `output/update-summary.md` - Summary of changes for review

**Usage:**
```bash
export CLI_COMMANDS_PATH=/path/to/goose-cli-commands.md
cd output
goose run --recipe ../recipes/update-cli-commands.yaml
```

## Directory Structure

```
cli-command-tracking/
├── README.md                           # This file
├── TESTING.md                          # Testing guide for GitHub Actions workflow
├── .gitignore                          # Excludes output/ directory
├── config/                             # Configuration files
│   └── skip-commands.json              # Commands to exclude from tracking
├── scripts/                            # Extraction and diff scripts
│   ├── extract-cli-structure.sh        # Wrapper that builds goose and runs Python
│   ├── extract-cli-structure.py        # Python script to parse --help output
│   ├── diff-cli-structures.py          # Compare structures and detect changes
│   └── run-pipeline.sh                 # End-to-end pipeline runner
├── recipes/                            # AI recipes
│   ├── synthesize-cli-changes.yaml     # Generate change docs
│   └── update-cli-commands.yaml        # Update documentation
├── .github/workflows/                  # GitHub Actions workflow
│   └── docs-update-cli-ref.yml         # Workflow definition
└── output/                             # Generated files (gitignored)
    ├── old-cli-structure.json          # Previous version structure
    ├── new-cli-structure.json          # Current version structure
    ├── cli-changes.json                # Detected changes (structured)
    ├── cli-changes.md                  # Change documentation (human-readable)
    ├── update-summary.md               # Documentation update summary
    └── pipeline.log                    # Pipeline execution log
```

## GitHub Actions Workflow

The automation runs via `.github/workflows/docs-update-cli-ref.yml`:

- **Trigger**: Automatically on new releases, or manually for testing
- **Process**: Builds goose for both versions, extracts CLI structures, detects changes, updates documentation
- **Output**: Creates a PR with updated `goose-cli-commands.md` if changes detected
- **Testing**: See [TESTING.md](./TESTING.md) for detailed testing instructions

## What Gets Tracked

### Commands
- ✅ Commands added/removed
- ✅ Command descriptions changed
- ✅ Command aliases added/removed
- ✅ Subcommands added/removed

### Options
- ✅ Options added/removed
- ✅ Option help text changed
- ✅ Default values changed
- ✅ Possible values changed (enums)
- ✅ Short/long flags changed

### Breaking Changes (Auto-Categorized)
- Command removed (high severity)
- Option removed (high severity)
- Option renamed (high severity)
- Default value changed (medium severity)
- Enum values removed (high severity)
- Alias removed (medium severity)

## Maintenance

When modifying the automation:

1. **Test locally first**: Run `./scripts/run-pipeline.sh` with test versions
2. **Verify outputs**: Check generated files against actual CLI changes
3. **Test in fork**: Use GitHub Actions workflow with dry-run mode
4. **Document changes**: Update this README with design decisions

## Related Documentation

- [TESTING.md](./TESTING.md) - How to test the GitHub Actions workflow
- [Automation Overview](../README.md) - All automation projects
- [CLI Commands Guide](../../docs/guides/goose-cli-commands.md) - Target documentation
