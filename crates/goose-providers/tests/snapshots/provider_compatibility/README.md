# Verified provider requests

These JSON files are historical evidence that a provider/model/scenario completed
successfully **live**, including response parsing and the scenario assertions.
Every non-list run first generates current requests offline, then compares their
HTTP methods, endpoints, and payloads with the cached successful live requests.
A matching cache verifies the current requests without sending network traffic;
a mismatching cache is stale and never counts as a pass.

The cache key is `<provider>/<hex-encoded-model>/<scenario>.json`. The JSON metadata
contains the readable model ID. Files contain request payloads, verification
metadata, and the actual assistant history needed for tool continuation, not
authentication headers, standalone responses, usage, or timings. New entries
also store a credential-free endpoint fingerprint so private endpoint changes
invalidate verification even when the displayed hostname is redacted. Complete
diagnostics remain in ignored `target/provider-compatibility-matrix/` artifacts.

For tool continuation, offline comparison reuses the stored actual assistant/tool-call
history to generate the second-turn request. A fresh live run produces new assistant
history instead. Legacy tool-continuation caches without stored assistant history
require fresh live verification; other legacy snapshots can still be compared
using their stored requests.

## Run

```bash
cargo test -p goose-providers --features rustls-tls \
  --test provider_compatibility_matrix -- --ignored --nocapture
```

- A matching verified cache yields `verified` without network requests, even
  without credentials, unless live replay or record-only mode is requested.
- A missing or stale cache triggers a fresh live attempt when credentials are
  available and record-only mode is not set. Otherwise, the current requests are
  `recorded_only` and remain unverified; a stale entry is never a pass.
- Only a fully successful live scenario publishes a verified file, atomically.
- Offline or recording-only runs, failures, interrupted replays, and unsupported
  rows do not replace previously verified files. Preserving an old file does not
  make mismatching current requests verified.

## Invalidate and replay

Remove an individual file to invalidate it, for example:

```bash
rm crates/goose-providers/tests/snapshots/provider_compatibility/openai/6770742d362d6c756e61/text.json
```

Or force a fresh live run even when the current requests match the cache:

```bash
REPLAY_VERIFIED_LIVE=1 \
GOOSE_PROVIDER_MATRIX_PROVIDERS=openai \
GOOSE_PROVIDER_MATRIX_SCENARIOS=text \
OPENAI_API_KEY="$(cat "$HOME/.openai-key")" \
cargo test -p goose-providers --features rustls-tls \
  --test provider_compatibility_matrix -- --ignored --nocapture
```

`REPLAY_VERIFIED_LIVE=1` without filters attempts every eligible row, so use filters
to control cost. It forces live verification only when credentials are available;
without them, the run is `recorded_only` and unverified.
`GOOSE_PROVIDER_MATRIX_RECORD_ONLY=1` always wins over live replay and prevents
sending requests. Current requests are still generated offline first, but the run
is `recorded_only` and unverified even when they match the cache.

Use `GOOSE_PROVIDER_MATRIX_LIST=1` to preview selected rows. Provider, model, and
scenario filters accept exact, comma-separated names. Set
`GOOSE_PROVIDER_MATRIX_SNAPSHOT_DIR` to an isolated directory when experimenting
with cache behavior; `GOOSE_PROVIDER_MATRIX_OUTPUT_DIR` changes only run artifacts.

Review newly created or replaced files before committing. A matching cache verifies
that the current requests match historically accepted requests; it does not
establish that the remote service still behaves the same way today.
