# Canonical Model System

Provides a unified view of model metadata (pricing, capabilities, context limits) across different LLM providers. 
Normalizes provider-specific model names (e.g., `claude-sonnet-4-5-20250929`)
to canonical IDs (e.g., `anthropic/claude-sonnet-4.5`).

## Build Canonical Models
Fetches latest model metadata from OpenRouter and validates provider mappings:
```bash
cargo run --bin build_canonical_models              # Build and check (default)
cargo run --bin build_canonical_models --no-check   # Build only, skip checker
```

This script performs two operations by default:
1. **Builds canonical models** - Fetches from OpenRouter API and updates the registry
   - Writes to: `crates/goose-provider-types/src/canonical/data/canonical_models.json`
2. **Checks model mappings** (unless `--no-check` is passed) - Tests provider mappings and tracks changes over time
   - Reports unmapped models
   - Compares with previous runs (like a lock file)
   - Shows changed/added/removed mappings
   - Writes to: `crates/goose-provider-types/src/canonical/data/canonical_mapping_report.json`

The script is currently built from `crates/goose/src/bin/build_canonical_models.rs` and writes into this crate's `src/canonical/data` directory.
