# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-05

### Added

- `jgl add <path>` — register a jj repository in the config
- `jgl fetch` — run `jj git fetch` across all registered repositories in parallel (up to 4 at a time)
- Per-repo `changed` / `unchanged` status output after fetch
- Automatic short labels for each repo (disambiguated by path suffix when names collide)
- `--verbose` / `-v` flag on `jgl fetch` to show full jj output per repository

[Unreleased]: https://github.com/omarkohl/jungle/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/omarkohl/jungle/releases/tag/v0.1.0
