pub mod cli;
pub mod commands;
pub mod config;

use anyhow::Result;
use clap::Parser;
use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};

use cli::{Cli, Command};

/// # Errors
/// Returns an error if argument parsing fails or the subcommand returns an error.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_path = config_path()?;

    match cli.command {
        Command::Add { path } => commands::add::run(&config_path, &path),
        Command::Fetch {
            verbose,
            rebase,
            with_conflicts,
        } => commands::fetch::run(
            &config_path,
            &commands::fetch::FetchOptions {
                verbose,
                rebase,
                with_conflicts,
            },
        ),
    }
}

fn config_path() -> Result<std::path::PathBuf> {
    let strategy = choose_app_strategy(AppStrategyArgs {
        top_level_domain: "io".into(),
        author: "jungle".into(),
        app_name: "jungle".into(),
    })
    .map_err(|e| anyhow::anyhow!("failed to determine config dir: {e}"))?;
    Ok(strategy.config_dir().join("config.toml"))
}
