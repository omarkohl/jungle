# Plan: Project Basics

Bootstrap jungle (`jgl`) as a cross-platform Rust CLI with strict quality gates, TDD workflow, config management, and `jgl fetch`.

## 1. Project scaffold

```
Cargo.toml
rustfmt.toml
deny.toml
justfile
src/
  main.rs        # ~10 lines: parse args, call lib, handle exit code
  lib.rs         # pub fn run(args) -> Result<()>
  cli.rs         # clap derive definitions
  config.rs      # config loading, path expansion, persistence
  commands/
    mod.rs
    add.rs
    fetch.rs
tests/
  cli.rs         # assert_cmd integration tests
  fixtures/      # sample config files for tests
```

`main.rs` returns `ExitCode` (not `Result`), so error output is controlled — no `Error: ...` from Debug formatting.

`lib.rs` + `main.rs` split is mandatory: integration tests and assert_cmd can only exercise the public lib API. All logic lives in the lib.

## 2. Cargo.toml

```toml
[package]
name = "jungle"
version = "0.1.0"
edition = "2021"
rust-version = "1.80"
license = "MIT"
description = "Multi-repo manager for jujutsu (jj)"

[[bin]]
name = "jgl"
path = "src/main.rs"

[dependencies]
anyhow = "1"
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
etcetera = "0.8"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"

[profile.release]
lto = "thin"
strip = true
codegen-units = 1
panic = "abort"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
must_use_candidate = "allow"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
dbg_macro = "deny"
todo = "deny"
unimplemented = "deny"
```

Notes:
- `anyhow` for application-level errors with `.context()` on every `?`.
- `thiserror` omitted for now — no library consumers yet. Add when/if we expose a public API.
- `etcetera` for platform-correct config dirs (XDG on Linux, native on macOS/Windows).
- `rayon` deferred until `jgl fetch` parallelism is implemented (keep deps minimal).
- `expect_used` / `unwrap_used` denied — forces proper error propagation everywhere.

## 3. Formatting

`rustfmt.toml` at project root:

```toml
edition = "2021"
```

Stick to defaults. The useful non-default options (`imports_granularity`, `group_imports`) are still unstable and require nightly — not worth it.

## 4. Linting

All lint configuration lives in `Cargo.toml` `[lints]` table (see above). No `clippy.toml` unless we need parameter overrides later.

CI runs: `cargo clippy --all-targets -- -D warnings`

This promotes all `warn`-level clippy lints (including pedantic/nursery) to errors in CI, while keeping them as warnings locally during development.

## 5. Dependency auditing

`deny.toml` at project root:

```toml
[advisories]
vulnerability = "deny"
unmaintained = "warn"

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0"]

[bans]
multiple-versions = "warn"
```

CI runs `cargo deny check` via `EmbarkStudios/cargo-deny-action@v2`.

## 6. Testing strategy (TDD)

Three layers:

**Unit tests** — in-module `#[cfg(test)]` blocks. Test config parsing, path expansion, argument validation. These are the TDD inner loop.

**Integration tests** — `tests/` directory. Test public lib API: load a fixture config, run a command handler, assert results.

**CLI tests** — `assert_cmd` + `predicates` in `tests/cli.rs`. Test the actual binary end-to-end: argument parsing, error messages, exit codes.

TDD workflow:
1. Write a failing test for the next piece of behavior
2. Implement the minimum code to pass
3. Refactor
4. `cargo nextest run` for the inner loop (install: `cargo install cargo-nextest`)

Dev tooling:
- `cargo nextest run` — parallel test runner, better output than `cargo test`
- `bacon test` or `cargo watch -x test` — re-run tests on file save

Test fixtures live in `tests/fixtures/` — sample `config.toml` files with known content.

`tempfile` crate for tests that write config: create a temp dir, write config there, point the code at it. No test pollution.

## 7. Config management

Config file location: resolved via `etcetera` — `~/.config/jungle/config.toml` on Linux (XDG), platform-native elsewhere.

```toml
[[repos]]
path = "~/projects/foo"

[[repos]]
path = "/home/omar/personal/bar"
```

Tilde is stored as-is in the config file — it's more portable and human-readable. `~` is expanded to the home directory only at the point of use (e.g., when spawning `jj`). Absolute paths are stored as-is.

Implementation:
- `config.rs` owns `Config` struct (serde Serialize + Deserialize)
- `Config::load(path)` and `Config::save(path)` — read/write TOML
- `Config::add_repo(path)` — validate existence (after expanding `~`), check for duplicates, store original form
- `Config::resolve_path(path)` — expand `~` at call site, used by commands before shelling out
- Config dir created on first write if it doesn't exist (`std::fs::create_dir_all`)

TDD sequence for config:
1. Test: deserialize a known TOML string into Config
2. Test: serialize Config back to TOML
3. Test: round-trip load/save with tempfile
4. Test: `resolve_path` expands `~` to home dir
5. Test: adding a duplicate path is rejected
6. Test: adding a non-existent path produces an error

## 8. `jgl add <path>`

```
jgl add ~/projects/foo
```

1. Expand `~` to home directory (for validation only — not stored expanded)
2. Verify the path exists on disk — hard error if not
3. Verify the path contains a `.jj` directory — hard error if not
4. Load existing config (or create default if none exists)
5. Check for duplicates (compare resolved forms to catch `~/foo` vs `/home/user/foo`)
6. Append original form (with `~` if given) to `[[repos]]` and save

TDD sequence:
1. Test: adding a valid jj repo path updates config
2. Test: adding a non-existent path fails with clear error
3. Test: adding a path without `.jj` fails with clear error
4. Test: adding a duplicate fails — both `~/foo` and `/home/user/foo` forms detected
5. CLI test: `jgl add <path>` succeeds and config file is updated

## 9. `jgl fetch`

Runs `jj git fetch` in each registered repo. MVP: sequential execution first, parallel later.

Implementation:
1. Load config
2. For each repo path: spawn `jj git fetch` as a child process in that directory
3. Collect results (success/failure per repo)
4. Print summary

Use `std::process::Command` — no need for tokio/async. When we add parallelism, `rayon` or `std::thread::scope` is sufficient.

TDD sequence:
1. Test: `fetch_repo(path)` calls `jj git fetch` in the right directory (mock the command or use a test helper)
2. Test: fetch reports success/failure per repo
3. Test: fetch with empty config produces a helpful message
4. CLI test: `jgl fetch` with a valid config (requires a real jj repo in a temp dir for true integration testing — decide if this is in scope for CI or only local)

Testing challenge: `jgl fetch` shells out to `jj`. Options:
- **Real integration test**: create a temp jj repo with a git remote, run fetch. Slow, requires `jj` installed, but tests the real thing.
- **Command abstraction**: trait that wraps process spawning, inject a fake in tests. More testable but adds abstraction weight.
- Start with the command abstraction for unit tests, add a real integration test gated behind `#[ignore]` or a feature flag.

## 10. Cross-platform CI

`.github/workflows/ci.yml`:

```yaml
name: CI
on: [push, pull_request]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo nextest run
      - uses: EmbarkStudios/cargo-deny-action@v2

  build:
    needs: check
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - name: Install musl tools
        if: matrix.target == 'x86_64-unknown-linux-musl'
        run: sudo apt-get install -y musl-tools
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: jgl-${{ matrix.target }}
          path: |
            target/${{ matrix.target }}/release/jgl
            target/${{ matrix.target }}/release/jgl.exe
```

Notes:
- `dtolnay/rust-toolchain` not `actions-rs` (unmaintained).
- `Swatinem/rust-cache` for dependency caching.
- musl for static Linux binary (no glibc dependency).
- macOS Intel dropped — Apple Silicon only. Users on Intel Macs can use Rosetta or `cargo install`.
- `nextest` in CI requires installation — add `cargo install cargo-nextest` or use `taiki-e/install-action@nextest`.

## 11. Release / deployment

Two installation methods:

**`cargo install jungle`** — works once published to crates.io. Zero setup.

**Prebuilt binaries** — use `cargo-dist` to generate a GitHub Actions release workflow:
```bash
cargo install cargo-dist
cargo dist init   # generates release CI config
```

This creates GitHub Releases with binaries + shell/PowerShell install scripts on each tag push.

Defer `cargo-dist` setup until after MVP works. For now, the CI build job uploads artifacts that can be downloaded manually.

## 12. Local dev setup

`justfile` at project root:

```just
# Run all checks (mirrors CI)
check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo nextest run

# Format code
fmt:
    cargo fmt

# Run tests in watch mode
dev:
    bacon test
```

Developers run `just check` before committing.

## Implementation order

1. Scaffold: `Cargo.toml`, `rustfmt.toml`, `deny.toml`, `justfile`, empty `src/main.rs` + `src/lib.rs`
2. CLI skeleton: `cli.rs` with clap, `main.rs` calls `lib::run()`, test `--version` and `--help` with assert_cmd
3. Config: `config.rs` with load/save/add, full unit test coverage, tilde expansion
4. `jgl add`: wire command to config, CLI integration test
5. `jgl fetch`: command abstraction, unit tests, CLI integration test
6. CI: `.github/workflows/ci.yml`
7. Release: `cargo-dist` init

Each step follows TDD: write failing test first, then implement.

