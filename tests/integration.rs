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
    jungle::commands::fetch::run(
        &config_path,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap();

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

    jungle::commands::fetch::run(
        &config_path,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap();

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

    let err = jungle::commands::fetch::run(
        &config_path,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap_err();
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

    let err = jungle::commands::fetch::run(
        &config_path,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("failed"));
}

// --- new behaviour tests ---

#[test]
fn fetch_continues_after_partial_failure() {
    let tmp = TempDir::new().unwrap();

    let repo_a = TestRepo::new(tmp.path().join("repo_a"))
        .with_remote("origin")
        .with_commit("initial a", &[("a.txt", "a")])
        .build();

    let repo_b = TestRepo::new(tmp.path().join("repo_b"))
        .with_remote("origin")
        .with_commit("initial b", &[("b.txt", "b")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo_a.path().to_str().unwrap()).unwrap();
    jungle::commands::add::run(&config_path, repo_b.path().to_str().unwrap()).unwrap();

    // Push a new commit to repo_b's remote
    let clone_b = repo_b.clone_as(tmp.path().join("clone_b"));
    clone_b.commit("feat: new in b", &[("new.txt", "x")]);
    clone_b.push("origin");

    // Delete repo_a so its fetch fails
    std::fs::remove_dir_all(repo_a.path()).unwrap();

    // run() should report failure (repo_a errored)
    let err = jungle::commands::fetch::run(
        &config_path,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("failed"));

    // repo_b must still have been fetched despite repo_a failing
    assert!(
        repo_b.log_messages().contains(&"feat: new in b".to_owned()),
        "repo_b should have been fetched even though repo_a failed"
    );
}

#[test]
fn fetch_result_shows_changed_and_unchanged() {
    let tmp = TempDir::new().unwrap();

    let repo_a = TestRepo::new(tmp.path().join("repo_a"))
        .with_remote("origin")
        .with_commit("initial a", &[("a.txt", "a")])
        .build();

    let repo_b = TestRepo::new(tmp.path().join("repo_b"))
        .with_remote("origin")
        .with_commit("initial b", &[("b.txt", "b")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo_a.path().to_str().unwrap()).unwrap();
    jungle::commands::add::run(&config_path, repo_b.path().to_str().unwrap()).unwrap();

    // Push a new commit only to repo_a's remote
    let clone_a = repo_a.clone_as(tmp.path().join("clone_a"));
    clone_a.commit("feat: new in a", &[("new.txt", "x")]);
    clone_a.push("origin");

    let results = jungle::commands::fetch::run_with_results(
        &config_path,
        &jungle::commands::fetch::ProcessRunner,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap();

    let result_a = results.iter().find(|r| r.path == *repo_a.path()).unwrap();
    let result_b = results.iter().find(|r| r.path == *repo_b.path()).unwrap();

    assert!(
        matches!(
            result_a.status,
            jungle::commands::fetch::FetchStatus::Changed
        ),
        "repo_a should be Changed"
    );
    assert!(
        matches!(
            result_b.status,
            jungle::commands::fetch::FetchStatus::Unchanged
        ),
        "repo_b should be Unchanged"
    );
}

#[test]
fn fetch_labels_repos_by_dirname() {
    let tmp = TempDir::new().unwrap();

    let repo_a = TestRepo::new(tmp.path().join("my_project"))
        .with_remote("origin")
        .with_commit("initial", &[("a.txt", "a")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo_a.path().to_str().unwrap()).unwrap();

    let results = jungle::commands::fetch::run_with_results(
        &config_path,
        &jungle::commands::fetch::ProcessRunner,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap();

    assert_eq!(results[0].label, "my_project");
}

#[test]
fn fetch_disambiguates_same_dirname() {
    let tmp = TempDir::new().unwrap();

    // Two repos with the same directory name but under different parents
    let dir_a = tmp.path().join("team_a").join("myrepo");
    let dir_b = tmp.path().join("team_b").join("myrepo");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let repo_a = TestRepo::new(dir_a)
        .with_remote("origin")
        .with_commit("initial a", &[("a.txt", "a")])
        .build();

    let repo_b = TestRepo::new(dir_b)
        .with_remote("origin")
        .with_commit("initial b", &[("b.txt", "b")])
        .build();

    let config_path = tmp.path().join("config.toml");
    jungle::commands::add::run(&config_path, repo_a.path().to_str().unwrap()).unwrap();
    jungle::commands::add::run(&config_path, repo_b.path().to_str().unwrap()).unwrap();

    let results = jungle::commands::fetch::run_with_results(
        &config_path,
        &jungle::commands::fetch::ProcessRunner,
        &jungle::commands::fetch::FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
        },
    )
    .unwrap();

    let labels: Vec<&str> = results.iter().map(|r| r.label.as_str()).collect();
    assert!(
        labels.contains(&"team_a/myrepo"),
        "expected team_a/myrepo in {labels:?}"
    );
    assert!(
        labels.contains(&"team_b/myrepo"),
        "expected team_b/myrepo in {labels:?}"
    );
}
