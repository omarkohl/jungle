#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use harness::TestRepo;
use tempfile::TempDir;

// --- jgl add ---

#[test]
fn add_real_jj_repo_registers_in_config() {
    let tmp = TempDir::new().unwrap();
    let repo = TestRepo::new(tmp.path().join("repo"))
        .with_commit("initial", &[("README.md", "# Hello")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo.path().to_str().unwrap()).unwrap();

    let config = jungle::config::Config::load(&config_path).unwrap();
    assert_eq!(config.repos.len(), 1);
    assert_eq!(config.repos[0].path, repo.path().to_str().unwrap());
}

#[test]
fn add_repo_with_remote_registers_correctly() {
    let tmp = TempDir::new().unwrap();
    let repo = TestRepo::new(tmp.path().join("repo"))
        .with_remote("origin")
        .with_commit("initial", &[("README.md", "# Hello")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo.path().to_str().unwrap()).unwrap();

    let config = jungle::config::Config::load(&config_path).unwrap();
    assert_eq!(config.repos.len(), 1);
    // The remote path should exist and be a bare repo
    assert!(repo.remote_path("origin").join("HEAD").exists());
}

// --- jgl fetch ---

#[test]
fn fetch_pulls_commits_pushed_by_clone() {
    let tmp = TempDir::new().unwrap();
    let repo = TestRepo::new(tmp.path().join("repo"))
        .with_remote("origin")
        .with_commit("initial", &[("README.md", "# Hello")])
        .build();

    // Register repo in config
    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo.path().to_str().unwrap()).unwrap();

    // A second client clones, commits, and pushes
    let clone = repo.clone_as(tmp.path().join("clone"));
    clone.commit("feat: add feature", &[("src/lib.rs", "pub fn foo() {}")]);
    clone.push("origin");

    // Before fetch: repo does not see the new commit
    assert!(!repo
        .log_messages()
        .contains(&"feat: add feature".to_owned()));

    // Run jgl fetch
    jungle::commands::fetch::run(&config_path).unwrap();

    // After fetch: repo sees the new commit
    assert!(repo
        .log_messages()
        .contains(&"feat: add feature".to_owned()));
}

#[test]
fn fetch_multiple_repos_all_updated() {
    let tmp = TempDir::new().unwrap();

    let repo_a = TestRepo::new(tmp.path().join("repo_a"))
        .with_remote("origin")
        .with_commit("repo-a initial", &[("a.txt", "a")])
        .build();

    let repo_b = TestRepo::new(tmp.path().join("repo_b"))
        .with_remote("origin")
        .with_commit("repo-b initial", &[("b.txt", "b")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo_a.path().to_str().unwrap()).unwrap();
    jungle::commands::add::run(&config_path, repo_b.path().to_str().unwrap()).unwrap();

    // Push new commits from clones
    let clone_a = repo_a.clone_as(tmp.path().join("clone_a"));
    clone_a.commit("feat: from clone a", &[("new_a.txt", "x")]);
    clone_a.push("origin");

    let clone_b = repo_b.clone_as(tmp.path().join("clone_b"));
    clone_b.commit("feat: from clone b", &[("new_b.txt", "y")]);
    clone_b.push("origin");

    jungle::commands::fetch::run(&config_path).unwrap();

    assert!(repo_a
        .log_messages()
        .contains(&"feat: from clone a".to_owned()));
    assert!(repo_b
        .log_messages()
        .contains(&"feat: from clone b".to_owned()));
}

#[test]
fn fetch_fails_when_repo_is_deleted() {
    let tmp = TempDir::new().unwrap();
    let repo = TestRepo::new(tmp.path().join("repo"))
        .with_remote("origin")
        .with_commit("initial", &[("README.md", "# Hello")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo.path().to_str().unwrap()).unwrap();

    std::fs::remove_dir_all(repo.path()).unwrap();

    let err = jungle::commands::fetch::run(&config_path).unwrap_err();
    assert!(err.to_string().contains("failed"));
}

#[test]
fn fetch_fails_when_remote_is_deleted() {
    let tmp = TempDir::new().unwrap();
    let repo = TestRepo::new(tmp.path().join("repo"))
        .with_remote("origin")
        .with_commit("initial", &[("README.md", "# Hello")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo.path().to_str().unwrap()).unwrap();

    // Remove the bare remote so fetch has nowhere to pull from
    std::fs::remove_dir_all(repo.remote_path("origin")).unwrap();

    let err = jungle::commands::fetch::run(&config_path).unwrap_err();
    assert!(err.to_string().contains("failed"));
}
