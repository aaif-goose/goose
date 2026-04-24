# Recipe Schema Tracking

Automated pipeline for detecting and documenting Recipe schema and validation rule changes between goose releases.

## Overview

This automation keeps the [Recipe Reference Guide](https://goose-docs.ai/docs/guides/recipes/recipe-reference) synchronized with code changes by:

1. **Extracting** schema and validation rules from source code (deterministic)
2. **Detecting** changes between versions (deterministic diff)
3. **Synthesizing** human-readable change documentation (AI-powered)
4. **Updating** the Core Recipe Schema, Field Specifications, and Validation Rule sections in the Recipe Reference Guide (AI-powered)

The automation runs automatically on new releases via GitHub Actions, or can be run manually for testing.

## Quick Start

### Automated (GitHub Actions)

The automation runs automatically when a new release is published. See [TESTING.md](./TESTING.md) for testing instructions.

### Manual (Local Testing)

```bash
# Run the complete pipeline
./scripts/run-pipeline.sh v1.14.0 v1.15.0

# Or run individual steps:
# 1. Extract validation structures
./scripts/extract-validation-structure.sh v1.14.0 > output/old-validation-structure.json
./scripts/extract-validation-structure.sh v1.15.0 > output/new-validation-structure.json

# 2. Extract schemas
./scripts/extract-schema.sh v1.15.0 > output/new-schema.json

# 3. Detect changes
./scripts/diff-validation-structures.sh output/old-validation-structure.json \
                                        output/new-validation-structure.json \
                                        > output/validation-changes.json

# 4. Generate human-readable change documentation
cd output && goose run --recipe ../recipes/synthesize-validation-changes.yaml

# 5. Update recipe-reference.md
export RECIPE_REF_PATH=/path/to/recipe-reference.md
goose run --recipe ../recipes/update-recipe-reference.yaml
```

## Architecture

### Modular Pipeline Design

The automation uses a **hybrid approach**: deterministic shell scripts for data extraction/diffing, AI recipes for analysis and documentation updates.

```
┌─────────────────────────────────────────────────────────────────┐
│                    EXTRACTION (Deterministic)                    │
├─────────────────────────────────────────────────────────────────┤
│  extract-schema.sh              extract-validation-structure.sh  │
│  ↓                              ↓                                │
│  new-schema.json                new-validation-structure.json    │
└─────────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                    DIFFING (Deterministic)                       │
├─────────────────────────────────────────────────────────────────┤
│  diff-validation-structures.sh                                   │
│  ↓                                                               │
│  validation-changes.json                                         │
└─────────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                    SYNTHESIS (AI-Powered)                        │
├─────────────────────────────────────────────────────────────────┤
│  synthesize-validation-changes.yaml                              │
│  ↓                                                               │
│  validation-changes.md (human-readable)                          │
└─────────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                    UPDATE (AI-Powered)                           │
├─────────────────────────────────────────────────────────────────┤
│  update-recipe-reference.yaml                                    │
│  ↓                                                               │
│  recipe-reference.md (updated) + update-summary.md               │
└─────────────────────────────────────────────────────────────────┘
```

### Why This Design?

**Scripts handle deterministic tasks:**
- Version-specific code extraction using `git show`
- JSON schema parsing and comparison
- No interpretation or inference - direct text extraction

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
| `old-schema.json` | `extract-schema.sh` | `synthesize-validation-changes.yaml` | Previous version OpenAPI schema |
| `new-schema.json` | `extract-schema.sh` | `synthesize-validation-changes.yaml` | Current version OpenAPI schema |
| `old-validation-structure.json` | `extract-validation-structure.sh` | `diff-validation-structures.sh` | Previous version struct fields + validation functions |
| `new-validation-structure.json` | `extract-validation-structure.sh` | `diff-validation-structures.sh` | Current version struct fields + validation functions |
| `validation-changes.json` | `diff-validation-structures.sh` | `synthesize-validation-changes.yaml` | Detected changes (structured) |
| `validation-changes.md` | `synthesize-validation-changes.yaml` | `update-recipe-reference.yaml` | Human-readable change documentation |
| `update-summary.md` | `update-recipe-reference.yaml` | Human review | Summary of documentation updates |

## Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RECIPE_REF_PATH` | No | - | Full path to `recipe-reference.md` file (overrides `GOOSE_REPO` construction) |
| `GOOSE_REPO` | No | Auto-detect | Path to goose repository root |

**Example (for local testing):**
```bash
export RECIPE_REF_PATH=/path/to/local/goose/documentation/docs/guides/recipes/recipe-reference.md
# OR
export GOOSE_REPO=/path/to/local/goose
```

### Configuration Files

#### `config/serde-attributes.json`

Defines Serde attribute meanings for parsing struct fields:

```json
{
  "skip_serializing_if": "Field is optional and skipped when value matches condition",
  "default": "Field uses default value when missing during deserialization",
  "flatten": "Field's contents are flattened into parent struct",
  "rename": "Field is serialized with a different name"
}
```

**When to update:** When new Serde attributes are introduced in Recipe struct definitions.

#### `config/known-validation-files.json`

Lists source files containing recipe validation logic:

```json
{
  "validation_files": [
    "crates/goose/src/recipe/validate_recipe.rs",
    "crates/goose/src/agents/types.rs"
  ]
}
```

**When to update:** When validation logic is added to new files or moved to different locations.

### Scope and Exclusions

#### In Scope
- Top-level Recipe struct fields (all fields in `Recipe` struct)
- Validation functions in `validate_recipe.rs`
- Field types, optionality, and default values
- Validation error messages and requirements
- Enum value changes (e.g., new input types)

#### Excluded (By Design)
- **Extension schema deep-dives**: Extensions use a dual-purpose type (`ExtensionConfig`) shared across recipes, CLI, and runtime with mismatched validation requirements. The automation documents basic structure only. Extension-specific validation is documented separately.

**Why extensions are excluded:** The `ExtensionConfig` type serves multiple contexts with different validation needs:
- **Recipe context**: Looser validation for user-authored configurations
- **CLI context**: Stricter validation for command-line arguments
- **Runtime context**: Additional validation for server connections

Attempting to document all extension validation rules in the Recipe Reference would create confusion about which rules apply when. Extension documentation is maintained separately.

## Scripts

### `extract-schema.sh`

Extracts OpenAPI schema from the goose codebase.

**Usage:**
```bash
./scripts/extract-schema.sh [version] > output/new-schema.json
```

**Arguments:**
- `version` (optional): Git tag or commit to extract from (default: current working directory)

**Output:** JSON schema with field descriptions, types, and constraints

**Example:**
```bash
# Extract from current code
./scripts/extract-schema.sh > output/new-schema.json

# Extract from specific version
./scripts/extract-schema.sh v1.15.0 > output/old-schema.json
```

### `extract-validation-structure.sh`

Extracts Recipe struct fields and validation functions from source code.

**Usage:**
```bash
./scripts/extract-validation-structure.sh [version] > output/new-validation-structure.json
```

**Arguments:**
- `version` (optional): Git tag or commit to extract from (default: current working directory)

**Output:** JSON with struct fields (name, type, optionality, comments) and validation functions (signature, error messages)

**Example:**
```bash
# Extract from current code
./scripts/extract-validation-structure.sh > output/new-validation-structure.json

# Extract from v1.15.0
./scripts/extract-validation-structure.sh v1.15.0 > output/old-validation-structure.json
```

### `diff-validation-structures.sh`

Compares two validation structure files and outputs detected changes.

**Usage:**
```bash
./scripts/diff-validation-structures.sh <old-file> <new-file> > output/validation-changes.json
```

**Arguments:**
- `old-file`: Path to old validation structure JSON
- `new-file`: Path to new validation structure JSON

**Output:** JSON with categorized changes:
- `struct_fields.added`: New fields
- `struct_fields.removed`: Deleted fields
- `struct_fields.type_changed`: Type modifications
- `struct_fields.comment_changed`: Comment updates
- `validation_functions.added`: New validation rules
- `validation_functions.removed`: Deleted validation rules

**Example:**
```bash
./scripts/diff-validation-structures.sh \
  output/old-validation-structure.json \
  output/new-validation-structure.json \
  > output/validation-changes.json
```

## Recipes

### `synthesize-validation-changes.yaml`

Analyzes detected changes and generates human-readable documentation.

**Inputs:**
- `output/validation-changes.json` - Detected changes from diff script
- `output/old-schema.json` - Previous version schema (for descriptions)
- `output/new-schema.json` - Current version schema (for descriptions)

**Output:**
- `output/validation-changes.md` - Human-readable change documentation with:
  - Breaking changes with migration guidance
  - Non-breaking changes with usage examples
  - Validation rule additions/removals/modifications
  - Migration checklist

**Usage:**
```bash
cd output
goose run --recipe ../recipes/synthesize-validation-changes.yaml
```

**What it does:**
- Compares old and new schemas to detect enum changes and required field changes
- Analyzes struct field changes (additions, removals, type changes)
- Explains validation rule changes with examples
- Generates migration guidance for breaking changes
- Creates actionable checklist for recipe authors

### `update-recipe-reference.yaml`

Updates the Recipe Reference Guide based on synthesized changes.

**Inputs:**
- `output/validation-changes.md` - Change documentation from synthesis recipe
- `recipe-reference.md` - Target documentation file (path from `RECIPE_REF_PATH` or `GOOSE_REPO` env var)

**Outputs:**
- Updated `recipe-reference.md` with changes applied
- `output/update-summary.md` - Summary of changes for review

**Usage:**
```bash
export RECIPE_REF_PATH=/path/to/recipe-reference.md
goose run --recipe recipes/update-recipe-reference.yaml
```

**What it does:**
- Updates Core Recipe Schema table (field additions/removals/type changes)
- Adds/removes/updates Field Specification sections for complex fields
- Updates Validation Rules section with new/modified/removed rules
- Updates enum lists in Field Specifications (e.g., input types)
- Generates summary of all changes for review

**Target sections:**
1. **Core Recipe Schema table** - Field-level changes
2. **Field Specifications sections** - Detailed documentation for complex fields
3. **Validation Rules section** - Validation function changes

## Directory Structure

```
recipe-schema-tracking/
├── README.md                           # This file
├── TESTING.md                          # Testing guide for GitHub Actions workflow
├── .gitignore                          # Excludes output/ directory
├── config/                             # Configuration files
│   ├── serde-attributes.json           # Serde attribute definitions
│   ├── known-validation-files.json     # Validation source files
│   ├── extraction-output-schema.json   # Schema for extraction output
│   └── validation-output-schema.json   # Schema for validation output
├── scripts/                            # Shell scripts (deterministic)
│   ├── extract-schema.sh               # Extract OpenAPI schema
│   ├── extract-validation-structure.sh # Extract struct fields + validation
│   ├── diff-validation-structures.sh   # Compare structures
│   └── run-pipeline.sh                 # End-to-end pipeline runner
├── recipes/                            # AI recipes
│   ├── synthesize-validation-changes.yaml # Generate change docs
│   └── update-recipe-reference.yaml    # Update documentation
└── output/                             # Generated files (gitignored)
    ├── old-schema.json                 # Previous version schema
    ├── new-schema.json                 # Current version schema
    ├── old-validation-structure.json   # Previous version structure
    ├── new-validation-structure.json   # Current version structure
    ├── validation-changes.json         # Detected changes (structured)
    ├── validation-changes.md           # Change documentation (human-readable)
    ├── update-summary.md               # Documentation update summary
    └── pipeline.log                    # Pipeline execution log
```

## GitHub Actions Workflow

The automation runs via `.github/workflows/docs-update-recipe-ref.yml`:

- **Trigger**: Automatically on new releases, or manually for testing
- **Process**: Extracts schemas, detects changes, updates documentation
- **Output**: Creates a PR with updated `recipe-reference.md` if changes detected
- **Testing**: See [TESTING.md](./TESTING.md) for detailed testing instructions

## What Gets Tracked

### Struct Fields (6 structs)
- `Recipe` - Top-level recipe structure
- `Author` - Recipe author information  
- `Settings` - Recipe settings (model, provider, etc.)
- `Response` - Structured output schema
- `SubRecipe` - Sub-recipe definitions
- `RecipeParameter` - Parameter definitions

### Changes Detected
- ✅ Fields added/removed
- ✅ Field type changes (e.g., `Option<T>` → `T`)
- ✅ Comment changes (inline documentation)
- ✅ Validation functions added/removed/modified
- ✅ Error messages changed
- ✅ Enum value changes

## Maintenance

When modifying the automation:

1. **Test locally first**: Run `./scripts/run-pipeline.sh` with test versions
2. **Verify outputs**: Check generated files against source code
3. **Update configuration**: If validation files move or new attributes added
4. **Test in fork**: Use GitHub Actions workflow with dry-run mode
5. **Document changes**: Update this README with design decisions

## Related Documentation

- [TESTING.md](./TESTING.md) - How to test the GitHub Actions workflow
- [Automation Overview](../README.md) - All automation projects
- [Recipe Reference Guide](../../docs/guides/recipes/recipe-reference.md) - Target documentation
