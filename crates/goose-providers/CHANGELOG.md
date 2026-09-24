# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.10](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.9...gdk-v0.1.0-alpha.10) - 2026-09-24

### Added

- *(providers)* add Z.AI Coding Plan with streaming tool calls ([#12205](https://github.com/aaif-goose/goose/pull/12205))
- *(sdk)* allow OpenAI provider to accept a custom base URL ([#11868](https://github.com/aaif-goose/goose/pull/11868)) ([#11967](https://github.com/aaif-goose/goose/pull/11967))
- add Live voice conversations to the desktop app ([#12093](https://github.com/aaif-goose/goose/pull/12093))
- *(providers)* add EUrouter as a declarative provider ([#11619](https://github.com/aaif-goose/goose/pull/11619))
- *(acp)* scope Toolshim to custom providers ([#11414](https://github.com/aaif-goose/goose/pull/11414))

### Fixed

- *(gdk)* resolve vision support from the canonical catalog so tool result images reach the model ([#12440](https://github.com/aaif-goose/goose/pull/12440))
- *(providers)* rename deepseek-v4-flash to deepseek-flash ([#12269](https://github.com/aaif-goose/goose/pull/12269))
- *(providers)* classify many-image dimension limit errors as context length-exceeded ([#12208](https://github.com/aaif-goose/goose/pull/12208))
- *(providers)* drive pre-key model lists from the canonical registry ([#11475](https://github.com/aaif-goose/goose/pull/11475))
- fix Databricks GLM-5.3 and Kimi K3 reasoning effort ([#12079](https://github.com/aaif-goose/goose/pull/12079))
- *(anthropic)* preserved-thinking compliance for the provider layer ([#11836](https://github.com/aaif-goose/goose/pull/11836))
- send session ID header for OpenCode Go ([#11944](https://github.com/aaif-goose/goose/pull/11944))

### Other

- Add a decisions provider crate, with impls for openrouter and jev ([#12418](https://github.com/aaif-goose/goose/pull/12418))
- gpt-live API support ([#12011](https://github.com/aaif-goose/goose/pull/12011))

## [0.1.0-alpha.9](https://github.com/aaif-goose/goose/compare/gdk-v0.1.0-alpha.8...gdk-v0.1.0-alpha.9) - 2026-09-08

### Fixed

- *(security)* require HTTPS for Snowflake ([#11745](https://github.com/aaif-goose/goose/pull/11745))

### Other

- Support GPT-6 Astra models ([#11869](https://github.com/aaif-goose/goose/pull/11869))
