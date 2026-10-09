# Verified provider requests

These JSON files are historical evidence that a provider/model/scenario completed
successfully **live**, including response parsing and the scenario assertions.
They are not request-generation regression snapshots: the test deliberately skips
verified rows without generating or comparing today's payloads.

The cache key is `<provider>/<hex-encoded-model>/<scenario>.json`. The JSON metadata
contains the readable model ID. Files contain request payloads and verification
metadata, not authentication headers, standalone responses, usage, or timings.
A tool-continuation request includes the actual assistant/tool-call history replayed
on its second turn. Complete diagnostics remain in ignored
`target/provider-compatibility-matrix/` artifacts.

## Run

```bash
cargo test -p goose-providers --features rustls-tls \
  --test provider_compatibility_matrix -- --ignored --nocapture
```

- A valid `verified` file skips that row, even without credentials.
- A missing file runs normally: live with credentials, otherwise recording only.
- Only a fully successful live scenario publishes a verified file, atomically.
- Recording-only runs, failures, interrupted replays, and unsupported rows do not
  replace previously verified files.
- Malformed or mismatched snapshot metadata fails explicitly instead of billing
  an unexpected new request. Delete the file or force replay to repair it.

## Invalidate and replay

Remove an individual file to invalidate it, for example:

```bash
rm crates/goose-providers/tests/snapshots/provider_compatibility/openai/6770742d362d6c756e61/text.json
```

Or bypass verified files for a selected live run:

```bash
REPLAY_VERIFIED_LIVE=1 \
GOOSE_PROVIDER_MATRIX_PROVIDERS=openai \
GOOSE_PROVIDER_MATRIX_SCENARIOS=text \
OPENAI_API_KEY="$(cat "$HOME/.openai-key")" \
cargo test -p goose-providers --features rustls-tls \
  --test provider_compatibility_matrix -- --ignored --nocapture
```

`REPLAY_VERIFIED_LIVE=1` without filters attempts every eligible row, so use filters
to control cost. Missing credentials still prevent live requests, and
`GOOSE_PROVIDER_MATRIX_RECORD_ONLY=1` always prevents sending—even with replay set.
Record-only mode generates current requests rather than using cached verification.

Use `GOOSE_PROVIDER_MATRIX_LIST=1` to preview selected rows. Provider, model, and
scenario filters accept exact, comma-separated names. Set
`GOOSE_PROVIDER_MATRIX_SNAPSHOT_DIR` to an isolated directory when experimenting
with cache behavior; `GOOSE_PROVIDER_MATRIX_OUTPUT_DIR` changes only run artifacts.

Review newly created or replaced files before committing. Verification applies only
to the recorded request and model's historical acceptance; it does not establish
that the current formatter or remote service behaves the same way today.
