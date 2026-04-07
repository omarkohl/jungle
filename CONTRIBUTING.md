# Contributing

## Requirements

- Rust stable (≥ 1.80)
- [`just`](https://github.com/casey/just) — task runner
- [`cargo-nextest`](https://nexte.st) — test runner
- [`jj`](https://github.com/jj-vcs/jj) — required for integration tests
- [`bacon`](https://github.com/Canop/bacon) — optional, for watch mode (`just dev`)

## Setup

```sh
git clone https://github.com/omarkohl/jungle
cd jungle
cargo build
```

## Development workflow

```sh
just fmt        # format code
just check      # run all checks (mirrors CI): fmt, clippy, tests
just dev        # watch mode (requires bacon)
just integration  # run integration tests only
```

Or directly:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo nextest run
```

## Tests

- **CLI tests** (`tests/cli.rs`) — black-box tests against the compiled binary
- **Integration tests** (`tests/integration.rs`) — spin up real jj repos via the test harness

## Linting conventions

Clippy is configured in `Cargo.toml` with `pedantic` and `nursery` groups enabled. The following are hard errors:

- `unwrap_used`, `expect_used`, `panic` — use `anyhow::Result` and `?` instead
- `unsafe_code`
- `dbg_macro`, `todo`, `unimplemented`

## Commit conventions

Use [Conventional Commits](https://www.conventionalcommits.org/) with these types:

| Type | When |
|------|------|
| `feat:` | new user-facing feature |
| `fix:` | bug fix |
| `refactor:` | internal restructuring, no behavior change |
| `docs:` | documentation only |
| `tests:` | test additions or changes |
| `chore:` | maintenance (deps, config, cleanup) |
| `ci:` | CI/CD changes |
| `dev:` | dev tooling or version bumps |

Keep messages short. Focus on *why*, not *what*.

## Before submitting

Run `just check` — it mirrors the CI pipeline exactly. PRs must pass CI.
