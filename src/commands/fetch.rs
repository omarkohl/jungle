use std::collections::{BTreeMap, HashMap};
use std::io::{IsTerminal, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::config::Config;

const PARALLEL_LIMIT: usize = 4;

/// Default idle timeout for fetch operations (in seconds).
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 10;

/// Sentinel error returned when a process is killed due to idle timeout.
#[derive(Debug)]
pub struct IdleTimeoutError;

impl std::fmt::Display for IdleTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("idle timeout exceeded")
    }
}

impl std::error::Error for IdleTimeoutError {}

pub enum ProgressMsg {
    Started { index: usize, label: String },
    Finished { index: usize },
}

#[allow(clippy::needless_pass_by_value)]
fn progress_display_loop(rx: mpsc::Receiver<ProgressMsg>) {
    let mut active: BTreeMap<usize, (String, Instant)> = BTreeMap::new();
    let mut prev_lines: usize = 0;
    let mut out = std::io::stdout();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(ProgressMsg::Started { index, label }) => {
                active.insert(index, (label, Instant::now()));
            }
            Ok(ProgressMsg::Finished { index }) => {
                active.remove(&index);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if prev_lines > 0 {
            write!(out, "\x1B[{prev_lines}A\x1B[J").ok();
        }
        for (label, start) in active.values() {
            let elapsed = start.elapsed().as_secs();
            writeln!(out, "  fetching   {label}... {elapsed}s").ok();
        }
        out.flush().ok();
        prev_lines = active.len();
    }

    if prev_lines > 0 {
        write!(out, "\x1B[{prev_lines}A\x1B[J").ok();
        out.flush().ok();
    }
}

/// Read from `reader` in a loop, appending to a buffer and updating the
/// shared `last_activity` timestamp on every successful read.
fn drain_with_activity(mut reader: impl std::io::Read, last_activity: &Mutex<Instant>) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    let mut data = Vec::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                data.extend_from_slice(&buf[..n]);
                if let Ok(mut ts) = last_activity.lock() {
                    *ts = Instant::now();
                }
            }
        }
    }
    data
}

/// Spawn `cmd` and kill it if no stdout/stderr output arrives within
/// `idle_timeout`.  Returns the collected `Output` on success, or an
/// `IdleTimeoutError` if the process stalls.
fn spawn_with_idle_timeout(
    cmd: &mut std::process::Command,
    idle_timeout: Duration,
) -> Result<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn process: {e}"))?;

    // Safety: stdout/stderr are guaranteed to be Some after piped() + spawn()
    let stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdout"))?;
    let stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stderr"))?;

    let last_activity = Arc::new(Mutex::new(Instant::now()));

    let la_out = Arc::clone(&last_activity);
    let stdout_handle = thread::spawn(move || drain_with_activity(stdout_pipe, &la_out));

    let la_err = Arc::clone(&last_activity);
    let stderr_handle = thread::spawn(move || drain_with_activity(stderr_pipe, &la_err));

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_handle.join().unwrap_or_default();
                let stderr = stderr_handle.join().unwrap_or_default();
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                let idle = last_activity
                    .lock()
                    .map(|ts| ts.elapsed())
                    .unwrap_or(Duration::ZERO);
                if idle >= idle_timeout {
                    // Kill and reap
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(IdleTimeoutError.into());
                }
                thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(anyhow::anyhow!("failed to wait on child: {e}")),
        }
    }
}

pub struct FetchOutput {
    pub changed: bool,
    pub raw_output: String,
}

/// Abstraction over process spawning so tests can inject a fake runner.
pub trait CommandRunner: Sync {
    /// # Errors
    /// Returns an error if the command fails or cannot be spawned.
    fn run_jj_fetch(&self, dir: &Path) -> Result<FetchOutput>;
    /// # Errors
    /// Returns an error if the rebase command fails.
    fn run_jj_rebase(&self, dir: &Path) -> Result<()>;
    /// Returns the change IDs of all conflicted commits.
    /// # Errors
    /// Returns an error if the jj command fails.
    fn run_jj_conflicts(&self, dir: &Path) -> Result<Vec<String>>;
    /// # Errors
    /// Returns an error if the undo command fails.
    fn run_jj_undo(&self, dir: &Path) -> Result<()>;
}

pub struct ProcessRunner {
    pub idle_timeout: Duration,
}

impl CommandRunner for ProcessRunner {
    fn run_jj_fetch(&self, dir: &Path) -> Result<FetchOutput> {
        let refs_before = git_remote_refs(dir)?;

        let mut cmd = std::process::Command::new("jj");
        cmd.args(["git", "fetch"]).current_dir(dir);

        let output = if self.idle_timeout.is_zero() {
            cmd.output()
                .map_err(|e| anyhow::anyhow!("failed to spawn jj: {e}"))?
        } else {
            spawn_with_idle_timeout(&mut cmd, self.idle_timeout)?
        };

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

    fn run_jj_rebase(&self, dir: &Path) -> Result<()> {
        let output = std::process::Command::new("jj")
            .args(["rebase", "-b", "@", "-o", "trunk()"])
            .current_dir(dir)
            .output()
            .map_err(|e| anyhow::anyhow!("failed to spawn jj: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            anyhow::bail!("jj rebase failed in {}: {stderr}", dir.display())
        }
    }

    fn run_jj_conflicts(&self, dir: &Path) -> Result<Vec<String>> {
        let output = std::process::Command::new("jj")
            .args([
                "log",
                "--no-graph",
                "-r",
                "conflicts()",
                "-T",
                "change_id ++ \"\\n\"",
            ])
            .current_dir(dir)
            .output()
            .map_err(|e| anyhow::anyhow!("failed to spawn jj: {e}"))?;
        if output.status.success() {
            let ids = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
                .collect();
            Ok(ids)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            anyhow::bail!("jj log conflicts() failed in {}: {stderr}", dir.display())
        }
    }

    fn run_jj_undo(&self, dir: &Path) -> Result<()> {
        let output = std::process::Command::new("jj")
            .args(["undo"])
            .current_dir(dir)
            .output()
            .map_err(|e| anyhow::anyhow!("failed to spawn jj: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            anyhow::bail!("jj undo failed in {}: {stderr}", dir.display())
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
    TimedOut,
    Failed(String),
}

#[derive(Debug)]
pub enum RebaseStatus {
    Skipped,
    Rebased,
    RebasedWithConflicts,
    ConflictsUndone,
    Failed(String),
}

pub struct FetchOptions {
    pub verbose: bool,
    pub rebase: bool,
    pub with_conflicts: bool,
    pub idle_timeout: Duration,
}

#[derive(Debug)]
pub struct FetchResult {
    pub path: PathBuf,
    pub label: String,
    pub status: FetchStatus,
    pub rebase_status: RebaseStatus,
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
    opts: &FetchOptions,
    progress: Option<&mpsc::SyncSender<ProgressMsg>>,
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

    // Parallel fetch+rebase, PARALLEL_LIMIT at a time
    let mut results: Vec<FetchResult> = (0..repos.len())
        .map(|i| FetchResult {
            path: repos[i].clone(),
            label: labels[i].clone(),
            status: FetchStatus::Unchanged, // placeholder
            rebase_status: RebaseStatus::Skipped,
        })
        .collect();

    // Use scoped threads so we can borrow `runner` and `repos`
    thread::scope(|s| {
        for chunk_start in (0..repos.len()).step_by(PARALLEL_LIMIT) {
            let chunk_end = (chunk_start + PARALLEL_LIMIT).min(repos.len());
            let handles: Vec<_> = (chunk_start..chunk_end)
                .map(|i| {
                    let path = &repos[i];
                    let label = labels[i].clone();
                    s.spawn(move || {
                        if let Some(tx) = progress {
                            tx.send(ProgressMsg::Started { index: i, label }).ok();
                        }
                        let fetch_outcome = runner.run_jj_fetch(path);
                        if let Some(tx) = progress {
                            tx.send(ProgressMsg::Finished { index: i }).ok();
                        }
                        let fetch_status = match fetch_outcome {
                            Ok(ref out) if out.changed => FetchStatus::Changed,
                            Ok(_) => FetchStatus::Unchanged,
                            Err(ref e) if e.downcast_ref::<IdleTimeoutError>().is_some() => {
                                FetchStatus::TimedOut
                            }
                            Err(ref e) => FetchStatus::Failed(e.to_string()),
                        };

                        let rebase_status = if opts.rebase {
                            do_rebase(runner, path, opts.with_conflicts)
                        } else {
                            RebaseStatus::Skipped
                        };

                        (i, fetch_status, rebase_status)
                    })
                })
                .collect();

            for handle in handles {
                // join() only fails on thread panic, which we treat as an error
                let (i, fetch_status, rebase_status) = handle
                    .join()
                    .unwrap_or_else(|_| unreachable!("fetch thread panicked"));
                results[i].status = fetch_status;
                results[i].rebase_status = rebase_status;
            }
        }
    });

    Ok(results)
}

fn do_rebase(runner: &impl CommandRunner, dir: &Path, with_conflicts: bool) -> RebaseStatus {
    let conflicts_before = match runner.run_jj_conflicts(dir) {
        Ok(ids) => ids,
        Err(e) => return RebaseStatus::Failed(e.to_string()),
    };

    if let Err(e) = runner.run_jj_rebase(dir) {
        return RebaseStatus::Failed(e.to_string());
    }

    let conflicts_after = match runner.run_jj_conflicts(dir) {
        Ok(ids) => ids,
        Err(e) => return RebaseStatus::Failed(e.to_string()),
    };

    // New conflicts = conflicts_after - conflicts_before
    let has_new_conflicts = conflicts_after
        .iter()
        .any(|id| !conflicts_before.contains(id));

    if !has_new_conflicts {
        RebaseStatus::Rebased
    } else if with_conflicts {
        RebaseStatus::RebasedWithConflicts
    } else {
        if let Err(e) = runner.run_jj_undo(dir) {
            return RebaseStatus::Failed(format!("undo failed after conflicts: {e}"));
        }
        RebaseStatus::ConflictsUndone
    }
}

const fn rebase_suffix(fetch: &FetchStatus, rebase: &RebaseStatus) -> &'static str {
    match rebase {
        RebaseStatus::Skipped => "",
        // Annotate rebase when the fetch brought in changes, failed, or timed out;
        // "unchanged (rebased)" looks contradictory when the rebase was a no-op.
        RebaseStatus::Rebased => {
            if matches!(fetch, FetchStatus::Unchanged) {
                ""
            } else {
                " (rebased)"
            }
        }
        RebaseStatus::RebasedWithConflicts => " (rebased, conflicts kept)",
        RebaseStatus::ConflictsUndone => " (rebase had conflicts, undone)",
        RebaseStatus::Failed(_) => " (rebase failed)",
    }
}

/// # Errors
/// Returns an error if writing to `out` or `err` fails.
pub fn display_results(
    results: &[FetchResult],
    verbose: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<()> {
    for result in results {
        let suffix = rebase_suffix(&result.status, &result.rebase_status);
        match &result.status {
            FetchStatus::Changed => writeln!(out, "  changed    {}{suffix}", result.label)?,
            FetchStatus::Unchanged => {
                writeln!(out, "  unchanged  {}{suffix}", result.label)?;
            }
            FetchStatus::TimedOut => writeln!(err, "  timed out  {}", result.label)?,
            FetchStatus::Failed(e) => writeln!(err, "  error      {}: {e}", result.label)?,
        }
        if verbose {
            if let RebaseStatus::Failed(e) = &result.rebase_status {
                writeln!(err, "  rebase error {}: {e}", result.label)?;
            }
        }
    }
    Ok(())
}

/// # Errors
/// Returns an error if the config cannot be loaded or any fetch fails.
pub fn run(
    config_path: &Path,
    opts: &FetchOptions,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let config = Config::load_or_default(config_path)?;

    if config.repos.is_empty() {
        writeln!(
            out,
            "No repositories registered. Use `jgl add <path>` to add one."
        )?;
        return Ok(());
    }

    let runner = ProcessRunner {
        idle_timeout: opts.idle_timeout,
    };

    let is_tty = std::io::stdout().is_terminal();
    let (progress_tx, display_handle) = if is_tty {
        let (tx, rx) = mpsc::sync_channel(PARALLEL_LIMIT * 2);
        let handle = thread::spawn(|| progress_display_loop(rx));
        (Some(tx), Some(handle))
    } else {
        (None, None)
    };

    let results = run_with_results(config_path, &runner, opts, progress_tx.as_ref())?;

    drop(progress_tx);
    if let Some(handle) = display_handle {
        handle.join().unwrap_or(());
    }

    display_results(&results, opts.verbose, out, err)?;

    let failures = results
        .iter()
        .filter(|r| matches!(r.status, FetchStatus::Failed(_) | FetchStatus::TimedOut))
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
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Fetch(PathBuf),
        Rebase(PathBuf),
        Conflicts(PathBuf),
        Undo(PathBuf),
    }

    struct FakeRunner {
        fail_paths: Vec<PathBuf>,
        changed_paths: Vec<PathBuf>,
        timeout_paths: Vec<PathBuf>,
        rebase_fail_paths: Vec<PathBuf>,
        // Queue of responses for successive run_jj_conflicts calls
        conflict_responses: Arc<Mutex<Vec<Vec<String>>>>,
        calls: Arc<Mutex<Vec<Call>>>,
    }

    impl FakeRunner {
        fn new() -> Self {
            Self {
                fail_paths: vec![],
                changed_paths: vec![],
                timeout_paths: vec![],
                rebase_fail_paths: vec![],
                conflict_responses: Arc::new(Mutex::new(vec![])),
                calls: Arc::new(Mutex::new(vec![])),
            }
        }

        fn with_fail(mut self, path: PathBuf) -> Self {
            self.fail_paths.push(path);
            self
        }

        fn with_changed(mut self, path: PathBuf) -> Self {
            self.changed_paths.push(path);
            self
        }

        fn with_timeout(mut self, path: PathBuf) -> Self {
            self.timeout_paths.push(path);
            self
        }

        fn with_rebase_fail(mut self, path: PathBuf) -> Self {
            self.rebase_fail_paths.push(path);
            self
        }

        fn with_conflict_responses(self, responses: Vec<Vec<String>>) -> Self {
            *self.conflict_responses.lock().unwrap() = responses;
            self
        }

        fn was_called(&self, call: &Call) -> bool {
            self.calls.lock().unwrap().contains(call)
        }
    }

    impl CommandRunner for FakeRunner {
        fn run_jj_fetch(&self, dir: &Path) -> Result<FetchOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Fetch(dir.to_path_buf()));
            if self.timeout_paths.iter().any(|p| p == dir) {
                return Err(IdleTimeoutError.into());
            }
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

        fn run_jj_rebase(&self, dir: &Path) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Rebase(dir.to_path_buf()));
            if self.rebase_fail_paths.iter().any(|p| p == dir) {
                anyhow::bail!("simulated rebase failure");
            }
            Ok(())
        }

        fn run_jj_conflicts(&self, dir: &Path) -> Result<Vec<String>> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Conflicts(dir.to_path_buf()));
            let mut responses = self.conflict_responses.lock().unwrap();
            if responses.is_empty() {
                Ok(vec![])
            } else {
                Ok(responses.remove(0))
            }
        }

        fn run_jj_undo(&self, dir: &Path) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Undo(dir.to_path_buf()));
            Ok(())
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
            ..Default::default()
        };
        config.save(path).unwrap();
    }

    fn no_rebase() -> FetchOptions {
        FetchOptions {
            verbose: false,
            rebase: false,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        }
    }

    #[test]
    fn empty_config_prints_hint() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let mut out = Vec::new();
        let mut err = Vec::new();
        run(&config_path, &no_rebase(), &mut out, &mut err).unwrap();
        assert!(String::from_utf8(out)
            .unwrap()
            .contains("No repositories registered"));
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

        let runner = FakeRunner::new();
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
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

        let runner = FakeRunner::new().with_fail(repo_a.clone());
        // run_with_results does NOT return Err for individual failures
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
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

        let runner = FakeRunner::new().with_changed(repo_a);
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
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

        let runner = FakeRunner::new().with_fail(repo_a);
        // run() wraps run_with_results and bails on failures
        // We can't use run() with a fake runner via the public API, so test via run_with_results
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
        let failures = results
            .iter()
            .filter(|r| matches!(r.status, FetchStatus::Failed(_)))
            .count();
        assert_eq!(failures, 1);
    }

    // --- rebase tests ---

    #[test]
    fn rebase_after_successful_fetch() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new();
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::Rebased),
            "expected Rebased, got {:?}",
            results[0].rebase_status
        );
    }

    #[test]
    fn rebase_with_new_conflicts_triggers_undo() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new().with_conflict_responses(vec![
            vec![],                       // before: no conflicts
            vec!["change123".to_owned()], // after: new conflict
        ]);
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::ConflictsUndone),
            "expected ConflictsUndone, got {:?}",
            results[0].rebase_status
        );
        assert!(
            runner.was_called(&Call::Undo(repo_a)),
            "expected undo to be called"
        );
    }

    #[test]
    fn with_conflicts_keeps_conflicted_rebase() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner =
            FakeRunner::new().with_conflict_responses(vec![vec![], vec!["change123".to_owned()]]);
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: true,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::RebasedWithConflicts),
            "expected RebasedWithConflicts, got {:?}",
            results[0].rebase_status
        );
        assert!(
            !runner.was_called(&Call::Undo(repo_a)),
            "expected undo NOT to be called"
        );
    }

    #[test]
    fn pre_existing_conflicts_not_treated_as_new() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new().with_conflict_responses(vec![
            vec!["preexisting".to_owned()],
            vec!["preexisting".to_owned()],
        ]);
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::Rebased),
            "expected Rebased, got {:?}",
            results[0].rebase_status
        );
        assert!(
            !runner.was_called(&Call::Undo(repo_a)),
            "expected undo NOT to be called"
        );
    }

    #[test]
    fn rebase_runs_despite_fetch_failure() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new().with_fail(repo_a.clone());
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::Rebased),
            "expected Rebased despite fetch failure, got {:?}",
            results[0].rebase_status
        );
        assert!(
            runner.was_called(&Call::Rebase(repo_a)),
            "expected rebase to be called even after fetch failure"
        );
    }

    #[test]
    fn rebase_failure_captured() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new().with_rebase_fail(repo_a);
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::Failed(_)),
            "expected Failed, got {:?}",
            results[0].rebase_status
        );
    }

    #[test]
    fn no_rebase_when_flag_not_set() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new();
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::Skipped),
            "expected Skipped, got {:?}",
            results[0].rebase_status
        );
        assert!(
            !runner.was_called(&Call::Rebase(repo_a)),
            "expected rebase NOT to be called"
        );
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
        let paths = vec![PathBuf::from("/home/user/projects/jgl")];
        let labels = compute_labels(&paths);
        assert_eq!(labels, vec!["jgl".to_owned()]);
    }

    #[test]
    fn compute_labels_empty() {
        let paths: Vec<PathBuf> = vec![];
        let labels = compute_labels(&paths);
        assert!(labels.is_empty());
    }

    // --- rebase_suffix display tests ---

    #[test]
    fn rebased_suffix_shown_only_when_fetch_changed() {
        assert_eq!(
            rebase_suffix(&FetchStatus::Changed, &RebaseStatus::Rebased),
            " (rebased)",
            "should show (rebased) when fetch changed"
        );
        assert_eq!(
            rebase_suffix(&FetchStatus::Unchanged, &RebaseStatus::Rebased),
            "",
            "should suppress (rebased) when fetch unchanged"
        );
    }

    #[test]
    fn notable_rebase_suffixes_always_shown() {
        for fetch in [FetchStatus::Changed, FetchStatus::Unchanged] {
            assert_eq!(
                rebase_suffix(&fetch, &RebaseStatus::ConflictsUndone),
                " (rebase had conflicts, undone)"
            );
            assert_eq!(
                rebase_suffix(&fetch, &RebaseStatus::RebasedWithConflicts),
                " (rebased, conflicts kept)"
            );
            assert_eq!(
                rebase_suffix(&fetch, &RebaseStatus::Failed("e".into())),
                " (rebase failed)"
            );
        }
    }

    // --- idle timeout tests ---

    #[test]
    fn timeout_captured_as_timed_out_status() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new().with_timeout(repo_a);
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0].status, FetchStatus::TimedOut),
            "expected TimedOut, got {:?}",
            results[0].status
        );
    }

    #[test]
    fn rebase_runs_despite_timeout() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let repo_a = tmp.path().join("repo_a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_config(&config_path, &[repo_a.to_str().unwrap()]);

        let runner = FakeRunner::new().with_timeout(repo_a.clone());
        let opts = FetchOptions {
            verbose: false,
            rebase: true,
            with_conflicts: false,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
        };
        let results = run_with_results(&config_path, &runner, &opts, None).unwrap();
        assert!(
            matches!(results[0].rebase_status, RebaseStatus::Rebased),
            "expected Rebased despite timeout, got {:?}",
            results[0].rebase_status
        );
        assert!(
            runner.was_called(&Call::Rebase(repo_a)),
            "expected rebase to be called even after timeout"
        );
    }

    #[test]
    fn display_results_shows_timed_out() {
        let results = vec![FetchResult {
            path: PathBuf::from("/repo"),
            label: "repo".to_owned(),
            status: FetchStatus::TimedOut,
            rebase_status: RebaseStatus::Skipped,
        }];
        let mut out = Vec::new();
        let mut err = Vec::new();
        display_results(&results, false, &mut out, &mut err).unwrap();
        let err_str = String::from_utf8(err).unwrap();
        assert!(
            err_str.contains("timed out"),
            "expected 'timed out' in stderr, got: {err_str}"
        );
        assert!(
            err_str.contains("repo"),
            "expected label in stderr, got: {err_str}"
        );
    }

    #[test]
    fn timeout_mixed_with_success() {
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

        let runner = FakeRunner::new()
            .with_timeout(repo_a.clone())
            .with_changed(repo_b.clone());
        let results = run_with_results(&config_path, &runner, &no_rebase(), None).unwrap();
        let a = results.iter().find(|r| r.path == repo_a).unwrap();
        let b = results.iter().find(|r| r.path == repo_b).unwrap();
        assert!(matches!(a.status, FetchStatus::TimedOut));
        assert!(matches!(b.status, FetchStatus::Changed));
    }

    // --- spawn_with_idle_timeout integration tests ---

    #[test]
    fn idle_timeout_kills_stalled_process() {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("60");
        let start = Instant::now();
        let result = spawn_with_idle_timeout(&mut cmd, Duration::from_secs(1));
        let elapsed = start.elapsed();
        assert!(result.is_err(), "expected error from timed-out process");
        assert!(
            result
                .unwrap_err()
                .downcast_ref::<IdleTimeoutError>()
                .is_some(),
            "expected IdleTimeoutError"
        );
        // Should complete in roughly 1-2 seconds, not 60
        assert!(
            elapsed < Duration::from_secs(5),
            "took too long: {elapsed:?}"
        );
    }

    #[test]
    fn idle_timeout_allows_active_process() {
        // Process outputs every 0.3s for ~0.9s total; idle timeout is 2s
        let mut cmd = std::process::Command::new("bash");
        cmd.args(["-c", "for i in 1 2 3; do echo progress; sleep 0.3; done"]);
        let result = spawn_with_idle_timeout(&mut cmd, Duration::from_secs(2));
        assert!(
            result.is_ok(),
            "active process should not time out: {result:?}"
        );
        let output = result.unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("progress"),
            "expected progress output, got: {stdout}"
        );
    }

    #[test]
    fn idle_timeout_resets_on_output() {
        // Process is silent for 0.5s, then outputs, then silent for 0.5s, then outputs.
        // Total ~2s, but idle timeout is 1s. Since output arrives before each 1s window,
        // the process should complete.
        let mut cmd = std::process::Command::new("bash");
        cmd.args([
            "-c",
            "sleep 0.5; echo a; sleep 0.5; echo b; sleep 0.5; echo c",
        ]);
        let result = spawn_with_idle_timeout(&mut cmd, Duration::from_secs(1));
        assert!(
            result.is_ok(),
            "process with periodic output should not time out: {result:?}"
        );
    }
}
