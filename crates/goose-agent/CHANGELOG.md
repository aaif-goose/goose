# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.14](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.13...gdk-v0.1.0-alpha.14) - 2026-10-30

### Added

- context window report and viewer ([#10462](https://github.com/aaif-goose/goose/pull/10462))

### Fixed

- *(providers)* replay OpenAI Responses reasoning across tool turns ([#13222](https://github.com/aaif-goose/goose/pull/13222))

## [0.1.0-alpha.13](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.12...gdk-v0.1.0-alpha.13) - 2026-10-23

### Added

- *(subagent)* resume failed or cancelled delegates ([#13015](https://github.com/aaif-goose/goose/pull/13015))

### Fixed

- *(compaction)* context freeze preserved message must stay user-visible ([#12692](https://github.com/aaif-goose/goose/pull/12692))

### Other

- Automatic Thinking Level Operation ([#12237](https://github.com/aaif-goose/goose/pull/12237))
- Limit concurrent subagents with GOOSE_MAX_CONCURRENT_SUBAGENTS ([#12946](https://github.com/aaif-goose/goose/pull/12946))
- move skills discovery into GDK ([#12935](https://github.com/aaif-goose/goose/pull/12935))
- run foreground subagents in the GDK ([#12792](https://github.com/aaif-goose/goose/pull/12792))
- agent loop: make tool operations own tool call checks ([#12865](https://github.com/aaif-goose/goose/pull/12865))

## [0.1.0-alpha.12](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.11...gdk-v0.1.0-alpha.12) - 2026-10-13

### Added

- surface provider x-request-id on responses and errors ([#12587](https://github.com/aaif-goose/goose/pull/12587))

### Other

- not infer or steer while a tool approval is pending ([#12808](https://github.com/aaif-goose/goose/pull/12808))
- changed yield_to_customer to next enum ([#12789](https://github.com/aaif-goose/goose/pull/12789))
- rename to finalize_cancellation and make Emitter sync ([#12786](https://github.com/aaif-goose/goose/pull/12786))
- agent loop: make Stop end the run promptly and save what it produced ([#12765](https://github.com/aaif-goose/goose/pull/12765))
- Delete the legacy agent loop ([#12760](https://github.com/aaif-goose/goose/pull/12760))
- Resolve extensions into a per-inference lease ([#12161](https://github.com/aaif-goose/goose/pull/12161))
- Run the GDK agent loop on wasm32 ([#12569](https://github.com/aaif-goose/goose/pull/12569))

## [0.1.0-alpha.11](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.10...gdk-v0.1.0-alpha.11) - 2026-09-28

### Other

- don't emit warning when ending on a successful tool call ([#12468](https://github.com/aaif-goose/goose/pull/12468))

## [0.1.0-alpha.10](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.9...gdk-v0.1.0-alpha.10) - 2026-09-24

### Fixed

- *(agents)* keep streamed thinking ahead of text and tool calls ([#11837](https://github.com/aaif-goose/goose/pull/11837))
- *(anthropic)* preserved-thinking compliance for the provider layer ([#11836](https://github.com/aaif-goose/goose/pull/11836))

## [0.1.0-alpha.9](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.8...gdk-v0.1.0-alpha.9) - 2026-09-08

### Other

- update Cargo.toml dependencies
