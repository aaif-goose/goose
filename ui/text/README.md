# goose ACP TUI

Early stage and part of goose's broader move to ACP

https://github.com/aaif-goose/goose/issues/6642
https://github.com/aaif-goose/goose/discussions/7309

## Running

The TUI automatically launches the goose ACP server using the `goose acp` command.

### Development (from source)

When running from source, `npm start` automatically builds the Rust binary from the workspace root if needed:

```bash
cd ui/text
npm i
npm run start
```

The `dev:binary` script checks if the Rust binary needs rebuilding by comparing timestamps of:
- `target/release/goose` binary
- `Cargo.toml` and `Cargo.lock` 
- `crates/goose-cli/Cargo.toml`

If any source files are newer, it runs `cargo build --release -p goose-cli` automatically.

### Production (with prebuilt binaries)

In production, the TUI uses prebuilt binaries from the `@aaif/goose-binary-*` packages installed via `postinstall`.

### Custom server URL

To use a custom server URL instead of the built-in binary:

```bash
npm run start -- --server http://localhost:8080
```
