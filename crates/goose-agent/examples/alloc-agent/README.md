# Allocating embedded agent example

This independent workspace verifies the actual alloc-only dependency graph.
Its provider holds non-`Send` state, creates an OpenAI request, and parses fake
streamed network frames using Goose's existing format helpers. A local tool reads
a simulated temperature sensor. The application supplies approval, timestamps,
unique IDs, event handling, and session effects.

Run from the repository root:

```sh
cargo test --manifest-path crates/goose-agent/examples/alloc-agent/Cargo.toml --locked
cargo check --manifest-path crates/goose-agent/examples/alloc-agent/Cargo.toml --target thumbv6m-none-eabi --locked
cargo build --manifest-path crates/goose-agent/examples/alloc-agent/Cargo.toml --release --target thumbv7em-none-eabi --features bare-metal --locked
```

Install those Rust targets with `rustup target add` first. The last command produces
`target/thumbv7em-none-eabi/release/alloc-agent` inside this example directory.
It includes a reset entry, linker script, panic handler, and a 128 KiB bump heap.
The example's linker map assumes 1 MiB flash at `0x08000000` and 256 KiB RAM at
`0x20000000`; it is a link-validation fixture, not a board-support package.

For real hardware, replace the fake `Network`, fixed-clock `Host`, simulated tool,
allocator, memory map/startup, and spin executor with your board's implementations.
IDs must remain unique across reboots sharing stored conversations. An asynchronous
network stack must make progress on your executor and release resources when a
cancelled future is dropped. Supply correctly decoded frames to the chosen parser.
The example uses no credentials and makes no external network calls.
