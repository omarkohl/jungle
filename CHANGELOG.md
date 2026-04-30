# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.3.0] - 2026-04-30

Version jump from v0.5.x to v1.3.0 to signal production readiness. v1.0.0 was skipped as it can paradoxically look unfinished.

### Fixed

- Prevent terminal corruption when a fetch times out — `GIT_TERMINAL_PROMPT=0` stops git from opening `/dev/tty` for credential prompts, which previously left the console in a broken state after SIGKILL

## [0.5.1] - 2026-04-17

### Fixed

- Use `map_or` instead of `map().unwrap_or()` to satisfy new clippy lint in Rust 1.95

## [0.5.0] - 2026-04-16

### Added

- Rebase runs even when fetch fails or times out — partial successes are still rebased

### Changed

- `jgl fetch` output is now an aligned table instead of a flat list
- Rebase shows `no-op` instead of `rebased` when nothing changed

## [0.4.0] - 2026-04-15

### Added

- Live progress display during `jgl fetch` — shows per-repo status in real time instead of waiting for all fetches to complete
- Idle timeout for `jj git fetch` — avoids hanging indefinitely if a fetch stalls

## [0.3.1] - 2026-04-08

### Fixed

- Updated `fastrand` dependency from yanked `2.4.0` to `2.4.1`

## [0.3.0] - 2026-04-08

### Added

- Config file defaults for `rebase` and `with_conflicts` via a `[fetch]` section in `~/.config/jgl/config.toml` — CLI flags still override
- Shell completions for bash, zsh, and fish via `jgl completions <shell>`

### Fixed

- Rebase failure no longer dumps the full jj error to output unless `--verbose` is set — failures surface as a short `(rebase failed)` suffix

### Changed

- Renamed project from `jungle` to `jgl` — the binary was already `jgl`, so keeping the project name `jungle` was confusing. `jungle` is also taken on crates.io, making `jgl` the natural choice for `cargo install jgl`.
- Config directory moved from `~/.config/jungle/` to `~/.config/jgl/`

## [0.2.0] - 2026-04-07

### Added

- `--rebase` flag on `jgl fetch` — automatically rebase local changes after a successful fetch
- `--with-conflicts` flag on `jgl fetch` — keep a conflicted rebase instead of undoing it

## [0.1.0] - 2026-04-05

### Added

- `jgl add <path>` — register a jj repository in the config
- `jgl fetch` — run `jj git fetch` across all registered repositories in parallel (up to 4 at a time)
- Per-repo `changed` / `unchanged` status output after fetch
- Automatic short labels for each repo (disambiguated by path suffix when names collide)
- `--verbose` / `-v` flag on `jgl fetch` to show full jj output per repository

[Unreleased]: https://github.com/omarkohl/jgl/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/omarkohl/jgl/compare/v0.5.1...v1.3.0
[0.5.1]: https://github.com/omarkohl/jgl/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/omarkohl/jgl/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/omarkohl/jgl/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/omarkohl/jgl/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/omarkohl/jgl/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/omarkohl/jgl/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/omarkohl/jgl/releases/tag/v0.1.0
