# jungle

> it's a jj-ungle out there

— [Randy Newman](https://en.wikipedia.org/wiki/It%27s_a_Jungle_Out_There_(song)), *almost*

A multi-repo manager for [jujutsu (jj)](https://github.com/jj-vcs/jj). Register repos once, get a unified status dashboard, and run jj commands across all of them. Named after the jungle — a nod to jj, and a natural metaphor for repositories growing wild together.

> [!NOTE]
> jungle is in early development. Only `jgl add` and `jgl fetch` are implemented.

## Install

```sh
cargo install --path .
```

Or build manually:

```sh
cargo build --release
# binary at target/release/jgl
```

## Usage

```sh
jgl add <path>    # register a jj repository
jgl fetch         # run `jj git fetch` in all registered repos
```

Config is stored at `~/.config/jungle/config.toml` (Linux/XDG) or the platform equivalent.

## Features

**Repo registry** — register repos individually or in groups:

```
jgl add <path>          # register a repo
jgl add <path> -g work  # register into a group
jgl remove <path>
jgl list
```

**Status dashboard** — one-line-per-repo overview:

```
jgl status

NAME       BRANCH/BOOKMARK   AHEAD  BEHIND  DIRTY  STATUS
foo        main              0      2       no     ok
bar        feat-xyz          1      0       yes    conflict
```

**Fetch all** — `jj git fetch` across all repos in parallel:

```
jgl fetch
```

**Exec** — run any jj subcommand across repos:

```
jgl exec log -r 'trunk()'
jgl exec -g work git push
```

## Tech stack

Rust, single binary, no runtime deps. See [docs/design.md](docs/design.md) for architecture and [docs/plan-basics.md](docs/plan-basics.md) for the implementation plan.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).
