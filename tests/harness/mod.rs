#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn jj_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new("jj");
    cmd.current_dir(dir)
        .env("JJ_USER", "Test User")
        .env("JJ_EMAIL", "test@example.com")
        .env("JJ_CONFIG", "/dev/null");
    cmd
}

fn run_checked(cmd: &mut Command) {
    let output = cmd.output().expect("failed to spawn command");
    assert!(
        output.status.success(),
        "command failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

pub struct TestRepo {
    path: PathBuf,
    remotes: Vec<String>,
    commits: Vec<(String, Vec<(String, String)>)>,
}

impl TestRepo {
    pub const fn new(path: PathBuf) -> Self {
        Self {
            path,
            remotes: Vec::new(),
            commits: Vec::new(),
        }
    }

    pub fn with_remote(mut self, name: &str) -> Self {
        self.remotes.push(name.to_owned());
        self
    }

    pub fn with_commit(mut self, msg: &str, files: &[(&str, &str)]) -> Self {
        self.commits.push((
            msg.to_owned(),
            files
                .iter()
                .map(|(f, c)| ((*f).to_owned(), (*c).to_owned()))
                .collect(),
        ));
        self
    }

    pub fn build(self) -> BuiltRepo {
        let base = self.path.parent().expect("repo path must have a parent");

        // Create bare git remotes
        let repo_name = self
            .path
            .file_name()
            .expect("repo path must have a file name")
            .to_string_lossy();
        let mut remote_paths: HashMap<String, PathBuf> = HashMap::new();
        for name in &self.remotes {
            let remote_path = base.join(format!("{repo_name}-remote-{name}"));
            run_checked(Command::new("git").args([
                "init",
                "--bare",
                remote_path.to_str().unwrap(),
            ]));
            remote_paths.insert(name.clone(), remote_path);
        }

        // Init colocated jj repo
        std::fs::create_dir_all(&self.path).unwrap();
        run_checked(jj_cmd(&self.path).args(["git", "init", "--colocate"]));

        // Register remotes
        for (name, path) in &remote_paths {
            run_checked(jj_cmd(&self.path).args([
                "git",
                "remote",
                "add",
                name,
                path.to_str().unwrap(),
            ]));
        }

        // Create commits
        for (msg, files) in &self.commits {
            for (rel_path, content) in files {
                let full = self.path.join(rel_path);
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&full, content).unwrap();
            }
            run_checked(jj_cmd(&self.path).args(["commit", "-m", msg]));
        }

        // Create bookmark and push to first remote
        if !self.commits.is_empty() {
            if let Some(first_remote) = self.remotes.first() {
                run_checked(jj_cmd(&self.path).args(["bookmark", "create", "main", "-r", "@-"]));
                run_checked(jj_cmd(&self.path).args([
                    "git",
                    "push",
                    "--remote",
                    first_remote,
                    "-b",
                    "main",
                ]));
            }
        }

        BuiltRepo {
            path: self.path,
            remote_paths,
        }
    }
}

pub struct BuiltRepo {
    path: PathBuf,
    remote_paths: HashMap<String, PathBuf>,
}

impl BuiltRepo {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn remote_path(&self, name: &str) -> &Path {
        self.remote_paths.get(name).expect("remote not found")
    }

    pub fn run_jj(&self, args: &[&str]) -> Output {
        jj_cmd(&self.path)
            .args(args)
            .output()
            .expect("failed to spawn jj")
    }

    /// Number of non-root commits visible in the repo (includes working copy).
    pub fn commit_count(&self) -> usize {
        let output = jj_cmd(&self.path)
            .args([
                "log",
                "--no-graph",
                "-T",
                "change_id ++ \"\\n\"",
                "-r",
                "all() ~ root()",
            ])
            .output()
            .expect("failed to spawn jj");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    }

    /// Commit messages (first line) of all non-root, non-empty commits, newest first.
    pub fn log_messages(&self) -> Vec<String> {
        let output = jj_cmd(&self.path)
            .args([
                "log",
                "--no-graph",
                "-T",
                "description.first_line() ++ \"\\n\"",
                "-r",
                "all() ~ root()",
            ])
            .output()
            .expect("failed to spawn jj");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub fn has_file(&self, path: &str) -> bool {
        self.path.join(path).exists()
    }

    /// Clone this repo (from its first remote) into a new directory.
    pub fn clone_as(&self, path: PathBuf) -> TestClone {
        let remote_path = self
            .remote_paths
            .values()
            .next()
            .expect("no remotes to clone from");
        run_checked(jj_cmd(self.path.parent().unwrap()).args([
            "git",
            "clone",
            "--colocate",
            remote_path.to_str().unwrap(),
            path.to_str().unwrap(),
        ]));
        TestClone {
            path,
            remote_paths: self.remote_paths.clone(),
        }
    }
}

pub struct TestClone {
    path: PathBuf,
    remote_paths: HashMap<String, PathBuf>,
}

impl TestClone {
    pub fn commit(&self, msg: &str, files: &[(&str, &str)]) {
        for (rel_path, content) in files {
            let full = self.path.join(rel_path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full, content).unwrap();
        }
        run_checked(jj_cmd(&self.path).args(["commit", "-m", msg]));
    }

    /// Move `main` bookmark to the last commit and push.
    pub fn push(&self, remote: &str) {
        run_checked(jj_cmd(&self.path).args(["bookmark", "set", "main", "-r", "@-"]));
        run_checked(jj_cmd(&self.path).args(["git", "push", "--remote", remote, "-b", "main"]));
    }

    pub fn run_jj(&self, args: &[&str]) -> Output {
        jj_cmd(&self.path)
            .args(args)
            .output()
            .expect("failed to spawn jj")
    }

    #[allow(dead_code)]
    pub fn remote_path(&self, name: &str) -> &Path {
        self.remote_paths.get(name).expect("remote not found")
    }
}
