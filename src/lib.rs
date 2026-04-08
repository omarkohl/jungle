pub mod cli;
pub mod commands;
pub mod config;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};

use cli::{Cli, Command};

fn resolve_flag(cli_true: bool, cli_false: bool, config_val: Option<bool>) -> bool {
    if cli_true {
        true
    } else if cli_false {
        false
    } else {
        config_val.unwrap_or(false)
    }
}

/// # Errors
/// Returns an error if argument parsing fails or the subcommand returns an error.
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    if let Command::Completions { shell } = cli.command {
        generate(shell, &mut Cli::command(), "jgl", &mut std::io::stdout());
        return Ok(());
    }

    let config_path = config_path()?;

    match cli.command {
        Command::Completions { .. } => unreachable!(),
        Command::Add { path } => commands::add::run(&config_path, &path, &mut std::io::stdout()),
        Command::Fetch {
            verbose,
            rebase,
            no_rebase,
            with_conflicts,
            without_conflicts,
        } => {
            let config = config::Config::load_or_default(&config_path)?;
            let effective_rebase = resolve_flag(rebase, no_rebase, config.fetch.rebase);
            let effective_with_conflicts = resolve_flag(
                with_conflicts,
                without_conflicts,
                config.fetch.with_conflicts,
            );
            commands::fetch::run(
                &config_path,
                &commands::fetch::FetchOptions {
                    verbose,
                    rebase: effective_rebase,
                    with_conflicts: effective_with_conflicts,
                },
                &mut std::io::stdout(),
                &mut std::io::stderr(),
            )
        }
    }
}

fn config_path() -> Result<std::path::PathBuf> {
    let strategy = choose_app_strategy(AppStrategyArgs {
        top_level_domain: "io".into(),
        author: "jgl".into(),
        app_name: "jgl".into(),
    })
    .map_err(|e| anyhow::anyhow!("failed to determine config dir: {e}"))?;
    Ok(strategy.config_dir().join("config.toml"))
}
