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
- *(deps)* bump zstd to 0.14.3 for RUSTSEC advisories ([#13162](https://github.com/aaif-goose/goose/pull/13162))
- *(openrouter)* resolve variant suffixes like :free and :nitro to the base model ([#12979](https://github.com/aaif-goose/goose/pull/12979))

### Other

- *(release)* bump version to 1.57.0 (minor) ([#13161](https://github.com/aaif-goose/goose/pull/13161))

## [0.1.0-alpha.13](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.12...gdk-v0.1.0-alpha.13) - 2026-10-23

### Fixed

- *(providers)* request summarized thinking from Claude models ([#13029](https://github.com/aaif-goose/goose/pull/13029))
- fix stale models ([#13054](https://github.com/aaif-goose/goose/pull/13054))
- *(moonshot)* map provider name to moonshotai for canonical model lookup ([#12041](https://github.com/aaif-goose/goose/pull/12041))
- *(providers)* tolerate missing created_at on OpenAI Responses stream events ([#12241](https://github.com/aaif-goose/goose/pull/12241))

### Other

- *(release)* bump version to 1.56.0 (minor) ([#13012](https://github.com/aaif-goose/goose/pull/13012))
- Automatic Thinking Level Operation ([#12237](https://github.com/aaif-goose/goose/pull/12237))
- Support provider-defined cache prices ([#12944](https://github.com/aaif-goose/goose/pull/12944))
- *(release)* bump version to 1.55.0 (minor) ([#12863](https://github.com/aaif-goose/goose/pull/12863))

## [0.1.0-alpha.12](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.11...gdk-v0.1.0-alpha.12) - 2026-10-13

### Added

- surface provider x-request-id on responses and errors ([#12587](https://github.com/aaif-goose/goose/pull/12587))
- *(providers)* honor supports_vision declared on custom provider mo… ([#12769](https://github.com/aaif-goose/goose/pull/12769))
- Use live model metadata for OpenAI reasoning and prefer the Responses API ([#12568](https://github.com/aaif-goose/goose/pull/12568))
- fetch model metadata from models.dev live with bundled fallback ([#12560](https://github.com/aaif-goose/goose/pull/12560))

### Fixed

- *(cost)* apply long-context price tiers to cost estimates ([#12787](https://github.com/aaif-goose/goose/pull/12787))
- adaptive thinking for Claude Sonnet and Haiku 5.5 ([#12761](https://github.com/aaif-goose/goose/pull/12761))
- *(anthropic)* forward unknown request_params to wire, mirroring openai engine ([#12720](https://github.com/aaif-goose/goose/pull/12720))
- support hyphenated GPT 6.1 Sol effort aliases ([#12689](https://github.com/aaif-goose/goose/pull/12689))
- *(cost)* price Databricks GLM aliases from the zhipuai catalog ([#12673](https://github.com/aaif-goose/goose/pull/12673))
- *(bedrock)* preserve the service total when folding cache input tokens ([#12586](https://github.com/aaif-goose/goose/pull/12586))
- session naming/"none" reasoning effort fails for gpt-6.1-sol ([#12605](https://github.com/aaif-goose/goose/pull/12605))
- *(providers)* read cache write tokens from Responses API usage ([#12509](https://github.com/aaif-goose/goose/pull/12509))
- keep tool results consecutive when OpenAI-compatible tool images are emitted ([#12233](https://github.com/aaif-goose/goose/pull/12233))

### Other

- Delete the legacy agent loop ([#12760](https://github.com/aaif-goose/goose/pull/12760))
- use typed structs for openai message formatting ([#12725](https://github.com/aaif-goose/goose/pull/12725))
- Refresh the model catalog on the blocking pool ([#12727](https://github.com/aaif-goose/goose/pull/12727))
- *(release)* bump version to 1.54.0 (minor) ([#12701](https://github.com/aaif-goose/goose/pull/12701))
- upgrade rust to 1.99.0 ([#12714](https://github.com/aaif-goose/goose/pull/12714))
- *(release)* bump version to 1.53.0 (minor) ([#12565](https://github.com/aaif-goose/goose/pull/12565))
- Run the GDK agent loop on wasm32 ([#12569](https://github.com/aaif-goose/goose/pull/12569))
- Add Meta Muse Code provider ([#11765](https://github.com/aaif-goose/goose/pull/11765))

## [0.1.0-alpha.11](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.10...gdk-v0.1.0-alpha.11) - 2026-09-28

### Added

- *(gdk)* add model discovery to the Provider API ([#12497](https://github.com/aaif-goose/goose/pull/12497))

### Fixed

- GDK tools with no args still advertise empty params input schema ([#12527](https://github.com/aaif-goose/goose/pull/12527))
- *(costs)* estimate through canonical provider aliases ([#11916](https://github.com/aaif-goose/goose/pull/11916))

## [0.1.0-alpha.10](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.9...gdk-v0.1.0-alpha.10) - 2026-09-24

### Fixed

- *(gdk)* resolve vision support from the canonical catalog so tool result images reach the model ([#12440](https://github.com/aaif-goose/goose/pull/12440))
- *(providers)* set strict:false on chat-completions tools ([#12272](https://github.com/aaif-goose/goose/pull/12272))
- *(providers)* show CoreWeave in bundled metadata ([#12149](https://github.com/aaif-goose/goose/pull/12149))
- *(providers)* drive pre-key model lists from the canonical registry ([#11475](https://github.com/aaif-goose/goose/pull/11475))
- fix Databricks GLM-5.3 and Kimi K3 reasoning effort ([#12079](https://github.com/aaif-goose/goose/pull/12079))
- *(anthropic)* preserved-thinking compliance for the provider layer ([#11836](https://github.com/aaif-goose/goose/pull/11836))
- stop mapping Astra Off to reasoning.effort none ([#11976](https://github.com/aaif-goose/goose/pull/11976))
- *(google)* pair functionResponse names with the preceding request ([#11888](https://github.com/aaif-goose/goose/pull/11888))

### Other

- support Opus 5.5, GPT-6-{sol,luna} ([#12447](https://github.com/aaif-goose/goose/pull/12447))
- *(release)* bump version to 1.52.0 (minor) ([#12421](https://github.com/aaif-goose/goose/pull/12421))
- compress canonical_models.json ([#12385](https://github.com/aaif-goose/goose/pull/12385))
- *(release)* bump version to 1.51.0 (minor) ([#12071](https://github.com/aaif-goose/goose/pull/12071))

## [0.1.0-alpha.9](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.8...gdk-v0.1.0-alpha.9) - 2026-09-08

### Other

- *(release)* bump version to 1.50.0 (minor) ([#11907](https://github.com/aaif-goose/goose/pull/11907))
- Support GPT-6 Astra models ([#11869](https://github.com/aaif-goose/goose/pull/11869))
