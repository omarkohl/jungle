# jungle

> it's a jj-ungle out there

— [Randy Newman](https://en.wikipedia.org/wiki/It%27s_a_Jungle_Out_There_(song)), *almost*

A multi-repo manager for [jujutsu (jj)](https://github.com/jj-vcs/jj). Register repos once, get a unified status dashboard, and run jj commands across all of them. Named after the jungle — a nod to jj, and a natural metaphor for repositories growing wild together.

## Install

Download the latest binary from the [releases page](https://github.com/omarkohl/jungle/releases) and place it on your `PATH`.

Or build from source:

```sh
cargo build --release
# binary at target/release/jgl
```

## Usage

```sh
jgl add <path>    # register a jj repository
jgl fetch         # run `jj git fetch` in all registered repos
```

Config is stored at `~/.config/jungle/config.toml` (Linux/XDG) or the platform equivalent:

```toml
[[repos]]
path = "~/projects/foo"

[[repos]]
path = "~/projects/bar"

[fetch]
rebase = true          # rebase onto trunk() after each fetch (default: false)
with_conflicts = false # allow rebase even if it introduces conflicts (default: false)
```

CLI flags override config: `--rebase`/`--no-rebase` and `--with-conflicts`/`--without-conflicts`.

## Shell completions

```sh
# bash
source <(jgl completions bash)

# zsh
source <(jgl completions zsh)

# fish
jgl completions fish | source
```

To persist, add the `source` line to your shell's rc file (e.g. `~/.bashrc`, `~/.zshrc`).

## Tech stack

Rust, single binary, no runtime deps.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).
