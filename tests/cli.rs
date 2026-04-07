#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn jgl() -> Command {
    Command::cargo_bin("jgl").unwrap()
}

#[test]
fn version_flag() {
    jgl()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"jgl \d+\.\d+\.\d+").unwrap());
}

#[test]
fn help_flag() {
    jgl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("jj"));
}

#[test]
fn add_nonexistent_path_fails() {
    jgl()
        .args(["add", "/nonexistent/jungle-test-path-xyz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn add_path_without_jj_fails() {
    let tmp = TempDir::new().unwrap();
    jgl()
        .args(["add", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a jj repository"));
}

#[test]
fn add_valid_repo_updates_config() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".jj")).unwrap();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo.to_str().unwrap()).unwrap();

    let config = jungle::config::Config::load(&config_path).unwrap();
    assert_eq!(config.repos.len(), 1);
}

#[test]
fn fetch_with_no_config_succeeds() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    jungle::commands::fetch::run(
        &config_path,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap();
}
