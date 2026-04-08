# Design

## Motivation

Working across many jj repos means repeating the same commands (`jj git fetch`, `jj status`, `jj log`) in each one, and mentally tracking which repos have unpushed work or conflicts. There's no built-in way to get a bird's-eye view across repos the way tools like [myrepos](https://myrepos.branchable.com/) or [gita](https://github.com/nosarthur/gita) do for git. jgl fills that gap for jj.

## Tech stack

**Rust.** Fits the jj ecosystem (jj itself is Rust), gives a single binary with no runtime deps, and makes parallel repo operations easy via Rayon/Tokio.

- CLI parsing: `clap`
- Parallel execution: `rayon`
- Terminal UI / table rendering: `comfy-table` or `tabled`
- Config: `~/.config/jgl/config.toml` via `serde` + `toml`
- Shell out to `jj` binary (don't link against jj internals for now)

## MVP features

### 1. Repo registry

```
jgl add <path>          # register a repo
jgl add <path> -g work  # register into a group
jgl remove <path>
jgl list
```

Config stored in `~/.config/jgl/config.toml`:

```toml
[[repos]]
path = "/home/omar/projects/foo"
groups = ["work"]

[[repos]]
path = "/home/omar/personal/bar"
groups = ["personal"]
```

### 2. Status dashboard (`jgl status` / `jgl st`)

One line per repo, columns:

```
NAME       BRANCH/BOOKMARK   AHEAD  BEHIND  DIRTY  STATUS
foo        main              0      2       no     ok
bar        feat-xyz          1      0       yes    conflict
baz        main              0      0       no     ok
```

Populated by running `jj log` + `jj status` per repo in parallel. Color-coded.

### 3. Fetch all (`jgl fetch`)

Runs `jj git fetch` across all registered repos in parallel. Shows per-repo success/failure.

### 4. Exec arbitrary command (`jgl exec <jj args...>`)

```
jgl exec log -r 'trunk()'
jgl exec git push
```

Runs the given `jj` subcommand in every repo. Output grouped by repo name. Supports `-g <group>` to target a subset.

## Out of scope for MVP

- TUI (terminal UI with live updates)
- Auto-discovery of repos under a root dir
- `jj workspace`-awareness
- Hooks / custom per-repo actions (myrepos-style)
- Non-jj repos

## Open questions

1. **Output format for `jgl exec`**: stream output interleaved (with repo prefix) or buffer and print per-repo when done?
2. **Auto-discovery**: should `jgl add ~/projects` recursively find all `.jj` dirs? Useful but risks false positives.
3. **Conflict with `jj` internals**: shelling out to the `jj` binary is simple but fragile. Is there value in linking against jj's Rust library directly later?
