# jgl

> it's a jj-ungle out there

— [Randy Newman](https://en.wikipedia.org/wiki/It%27s_a_Jungle_Out_There_(song)), *almost*

Pronounced *jungle* /ˈdʒʌŋɡəl/. A multi-repo manager for [jujutsu (jj)](https://github.com/jj-vcs/jj). Register repos once and run jj commands across all of them. The name comes from the image of many repositories — each a tree of commits — growing wild and unmanaged on disk: a jungle. It also happens to sound like *jj*.

## Install

Download the latest binary from the [releases page](https://github.com/omarkohl/jgl/releases) and place it on your `PATH`.

Or install via cargo:

```sh
cargo install jgl
```

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

Config is stored at `~/.config/jgl/config.toml` (Linux/XDG) or the platform equivalent:

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
