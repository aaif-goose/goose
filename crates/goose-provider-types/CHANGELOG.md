# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
