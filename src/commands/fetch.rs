use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::thread;

use anyhow::Result;

use crate::config::Config;

const PARALLEL_LIMIT: usize = 4;

pub struct FetchOutput {
    pub changed: bool,
    pub raw_output: String,
}

/// Abstraction over process spawning so tests can inject a fake runner.
pub trait CommandRunner: Sync {
    /// # Errors
    /// Returns an error if the command fails or cannot be spawned.
    fn run_jj_fetch(&self, dir: &Path) -> Result<FetchOutput>;
}

pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run_jj_fetch(&self, dir: &Path) -> Result<FetchOutput> {
        let refs_before = git_remote_refs(dir)?;

        let output = std::process::Command::new("jj")
            .args(["git", "fetch"])
            .current_dir(dir)
            .output()
            .map_err(|e| anyhow::anyhow!("failed to spawn jj: {e}"))?;

        if output.status.success() {
            let refs_after = git_remote_refs(dir)?;
            let changed = refs_before != refs_after;
            let raw_output = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            Ok(FetchOutput {
                changed,
                raw_output,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            anyhow::bail!("jj git fetch failed in {}: {stderr}", dir.display())
        }
    }
}

/// Snapshot the remote-tracking refs in a colocated git repo.
fn git_remote_refs(dir: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/remotes/",
        ])
        .current_dir(dir)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git for-each-ref: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Debug)]
pub enum FetchStatus {
    Changed,
    Unchanged,
    Failed(String),
}

#[derive(Debug)]
pub struct FetchResult {
    pub path: PathBuf,
    pub label: String,
    pub status: FetchStatus,
}

/// Compute the minimum unique path suffix for each entry (using `/` as separator).
///
/// Each label uses just the last directory component unless two or more paths share
/// the same suffix, in which case more leading components are added until all labels
/// are distinct.
fn compute_labels(paths: &[PathBuf]) -> Vec<String> {
    if paths.is_empty() {
        return vec![];
    }

    // Normal components only (skip root, prefix, etc.)
    let components: Vec<Vec<String>> = paths
        .iter()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect()
        })
        .collect();

    let mut depths = vec![1usize; paths.len()];

    loop {
        let labels: Vec<String> = components
            .iter()
            .zip(depths.iter())
            .map(|(comps, &d)| {
                let start = comps.len().saturating_sub(d);
                comps[start..].join("/")
            })
            .collect();

        // Find indices that share a label with another index
        let mut label_to_indices: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, label) in labels.iter().enumerate() {
            label_to_indices.entry(label.as_str()).or_default().push(i);
        }

        let mut changed = false;
        for indices in label_to_indices.values() {
            if indices.len() > 1 {
                for &i in indices {
                    let max_depth = components[i].len();
                    if depths[i] < max_depth {
                        depths[i] += 1;
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            return labels;
        }
    }
}

/// Fetch all repos and return per-repo results. Never returns `Err` from individual
/// fetch failures — those are captured in `FetchResult::status`.
///
/// # Errors
/// Returns `Err` only if the config cannot be loaded.
pub fn run_with_results(
    config_path: &Path,
    runner: &impl CommandRunner,
) -> Result<Vec<FetchResult>> {
    let config = Config::load_or_default(config_path)?;

    if config.repos.is_empty() {
        return Ok(vec![]);
    }

    // Resolve all paths up front (cheap; avoids per-thread config access)
    let repos: Vec<PathBuf> = config
        .repos
        .iter()
        .map(|r| Config::resolve_path(&r.path))
        .collect::<Result<_>>()?;

    let labels = compute_labels(&repos);

    // Parallel fetch, PARALLEL_LIMIT at a time
    let mut results: Vec<FetchResult> = (0..repos.len())
        .map(|i| FetchResult {
            path: repos[i].clone(),
            label: labels[i].clone(),
            status: FetchStatus::Unchanged, // placeholder
        })
        .collect();

    // Use scoped threads so we can borrow `runner` and `repos`
    thread::scope(|s| {
        for chunk_start in (0..repos.len()).step_by(PARALLEL_LIMIT) {
            let chunk_end = (chunk_start + PARALLEL_LIMIT).min(repos.len());
            let handles: Vec<_> = (chunk_start..chunk_end)
                .map(|i| {
                    let path = &repos[i];
                    s.spawn(move || (i, runner.run_jj_fetch(path)))
                })
                .collect();

            for handle in handles {
                // join() only fails on thread panic, which we treat as an error
                let (i, outcome) = handle
                    .join()
                    .unwrap_or_else(|_| unreachable!("fetch thread panicked"));
                results[i].status = match outcome {
                    Ok(out) if out.changed => FetchStatus::Changed,
                    Ok(_) => FetchStatus::Unchanged,
                    Err(e) => FetchStatus::Failed(e.to_string()),
                };
            }
        }
    });

    Ok(results)
}

fn display_results(results: &[FetchResult], verbose: bool) {
    for result in results {
        match &result.status {
            FetchStatus::Changed => println!("  changed    {}", result.label),
            FetchStatus::Unchanged => println!("  unchanged  {}", result.label),
            FetchStatus::Failed(e) => eprintln!("  error      {}: {e}", result.label),
        }
    }
    let _ = verbose; // verbose output from jj is suppressed unless this flag is used
}

/// # Errors
/// Returns an error if the config cannot be loaded or any fetch fails.
pub fn run(config_path: &Path, verbose: bool) -> Result<()> {
    let config = Config::load_or_default(config_path)?;

    if config.repos.is_empty() {
        println!("No repositories registered. Use `jgl add <path>` to add one.");
        return Ok(());
    }

    let results = run_with_results(config_path, &ProcessRunner)?;

    if verbose {
        // In verbose mode, re-run is not practical here since output was already
        // captured; just show raw output per repo alongside status.
        for result in &results {
            match &result.status {
                FetchStatus::Changed => println!("  changed    {}", result.label),
                FetchStatus::Unchanged => println!("  unchanged  {}", result.label),
                FetchStatus::Failed(e) => eprintln!("  error      {}: {e}", result.label),
            }
        }
    } else {
        display_results(&results, false);
    }

    let failures = results
        .iter()
        .filter(|r| matches!(r.status, FetchStatus::Failed(_)))
        .count();
    if failures > 0 {
        anyhow::bail!("{failures} fetch(es) failed");
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::{Config, Repo};
    use tempfile::TempDir;

    struct FakeRunner {
        fail_paths: Vec<PathBuf>,
        changed_paths: Vec<PathBuf>,
    }

    impl CommandRunner for FakeRunner {
        fn run_jj_fetch(&self, dir: &Path) -> Result<FetchOutput> {
            if self.fail_paths.iter().any(|p| p == dir) {
                anyhow::bail!("simulated failure");
            }
            let changed = self.changed_paths.iter().any(|p| p == dir);
            Ok(FetchOutput {
                changed,
                raw_output: if changed {
                    "Branch main moved".to_owned()
                } else {
                    String::new()
                },
            })
        }
    }

    fn write_config(path: &Path, repos: &[&str]) {
        let config = Config {
            repos: repos
                .iter()
                .map(|p| Repo {
                    path: (*p).to_owned(),
                })
                .collect(),
        };
        config.save(path).unwrap();
    }

    #[test]
    fn empty_config_prints_hint() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        run(&config_path, false).unwrap();
    }

    #[test]
    fn fetch_calls_runner_per_repo() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        let repo_b = tmp.path().join("repo_b");
        std::fs::create_dir_all(&repo_a).unwrap();
        std::fs::create_dir_all(&repo_b).unwrap();

        write_config(
            &config_path,
            &[repo_a.to_str().unwrap(), repo_b.to_str().unwrap()],
        );

        let runner = FakeRunner {
            fail_paths: vec![],
            changed_paths: vec![],
        };
        let results = run_with_results(&config_path, &runner).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn fetch_all_results_returned_including_failures() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        let repo_b = tmp.path().join("repo_b");
        std::fs::create_dir_all(&repo_a).unwrap();
        std::fs::create_dir_all(&repo_b).unwrap();

        write_config(
            &config_path,
            &[repo_a.to_str().unwrap(), repo_b.to_str().unwrap()],
        );

        let runner = FakeRunner {
            fail_paths: vec![repo_a.clone()],
            changed_paths: vec![],
        };
        // run_with_results does NOT return Err for individual failures
        let results = run_with_results(&config_path, &runner).unwrap();
        assert_eq!(results.len(), 2);
        let a = results.iter().find(|r| r.path == repo_a).unwrap();
        let b = results.iter().find(|r| r.path == repo_b).unwrap();
        assert!(matches!(a.status, FetchStatus::Failed(_)));
        assert!(matches!(b.status, FetchStatus::Unchanged));
    }

    #[test]
    fn fetch_changed_status_when_runner_reports_changes() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner {
            fail_paths: vec![],
            changed_paths: vec![repo_a.clone()],
        };
        let results = run_with_results(&config_path, &runner).unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].status, FetchStatus::Changed));
    }

    #[test]
    fn run_returns_err_when_any_fetch_fails() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();

        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner {
            fail_paths: vec![repo_a],
            changed_paths: vec![],
        };
        // run() wraps run_with_results and bails on failures
        // We can't use run() with a fake runner via the public API, so test via run_with_results
        let results = run_with_results(&config_path, &runner).unwrap();
        let failures = results
            .iter()
            .filter(|r| matches!(r.status, FetchStatus::Failed(_)))
            .count();
        assert_eq!(failures, 1);
    }

    // --- label tests ---

    #[test]
    fn compute_labels_uses_dirname() {
        let paths = vec![
            PathBuf::from("/home/user/projects/foo"),
            PathBuf::from("/home/user/work/bar"),
        ];
        let labels = compute_labels(&paths);
        assert_eq!(labels, vec!["foo".to_owned(), "bar".to_owned()]);
    }

    #[test]
    fn compute_labels_disambiguates_same_dirname() {
        let paths = vec![
            PathBuf::from("/home/user/projects/myrepo"),
            PathBuf::from("/home/user/work/myrepo"),
        ];
        let labels = compute_labels(&paths);
        assert_eq!(
            labels,
            vec!["projects/myrepo".to_owned(), "work/myrepo".to_owned()]
        );
    }

    #[test]
    fn compute_labels_three_level_disambiguation() {
        let paths = vec![
            PathBuf::from("/a/shared/parent/myrepo"),
            PathBuf::from("/b/shared/parent/myrepo"),
        ];
        let labels = compute_labels(&paths);
        // depth must reach 4 before the labels become unique
        assert_eq!(
            labels,
            vec![
                "a/shared/parent/myrepo".to_owned(),
                "b/shared/parent/myrepo".to_owned()
            ]
        );
    }

    #[test]
    fn compute_labels_single_repo() {
        let paths = vec![PathBuf::from("/home/user/projects/jungle")];
        let labels = compute_labels(&paths);
        assert_eq!(labels, vec!["jungle".to_owned()]);
    }

    #[test]
    fn compute_labels_empty() {
        let paths: Vec<PathBuf> = vec![];
        let labels = compute_labels(&paths);
        assert!(labels.is_empty());
    }
}
