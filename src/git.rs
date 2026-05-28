/// Git integration — side-git snapshots for workspace safety.
/// Inspired by CodeWhale's side-git approach: snapshot outside the repo's .git.
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub struct GitOps {
    pub workdir: PathBuf,
    pub side_git_dir: PathBuf,
}

impl GitOps {
    pub fn new(workdir: PathBuf) -> Self {
        let side_git_dir = workdir.join(".mimo-tui").join("snapshots");
        Self { workdir, side_git_dir }
    }

    /// Initialize the side-git repo
    pub fn init(&self) -> Result<()> {
        if !self.side_git_dir.exists() {
            std::fs::create_dir_all(&self.side_git_dir)?;
            Command::new("git")
                .arg("init")
                .current_dir(&self.side_git_dir)
                .output()?;
        }
        Ok(())
    }

    /// Take a snapshot of the current workspace
    pub fn snapshot(&self, label: &str) -> Result<String> {
        self.init()?;

        // Copy changed files to side-git
        let output = Command::new("git")
            .args(["diff", "--name-only"])
            .current_dir(&self.workdir)
            .output()?;

        let changed = String::from_utf8_lossy(&output.stdout);
        for file in changed.lines() {
            if file.is_empty() { continue; }
            let src = self.workdir.join(file);
            let dst = self.side_git_dir.join(file);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::copy(&src, &dst) {
                    eprintln!("Warning: Could not copy {}: {}", file, e);
                }
            }
        }

        // Also copy untracked files
        let output = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard"])
            .current_dir(&self.workdir)
            .output()?;

        let untracked = String::from_utf8_lossy(&output.stdout);
        for file in untracked.lines() {
            if file.is_empty() { continue; }
            let src = self.workdir.join(file);
            let dst = self.side_git_dir.join(file);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::copy(&src, &dst) {
                    eprintln!("Warning: Could not copy {}: {}", file, e);
                }
            }
        }

        // Commit in side-git
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.side_git_dir)
            .output()?;

        let output = Command::new("git")
            .args(["commit", "-m", label, "--allow-empty"])
            .current_dir(&self.side_git_dir)
            .output()?;

        let hash = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&self.side_git_dir)
            .output()?;

        Ok(String::from_utf8_lossy(&hash.stdout).trim().to_string())
    }

    /// List recent snapshots
    pub fn list(&self, count: usize) -> Result<Vec<String>> {
        if !self.side_git_dir.exists() {
            return Ok(vec![]);
        }

        let output = Command::new("git")
            .args(["log", "--oneline", &format!("-{}", count)])
            .current_dir(&self.side_git_dir)
            .output()?;

        let lines = String::from_utf8_lossy(&output.stdout);
        Ok(lines.lines().map(|l| l.to_string()).collect())
    }

    /// Restore a snapshot by hash
    pub fn restore(&self, hash: &str) -> Result<()> {
        Command::new("git")
            .args(["checkout", hash, "--", "."])
            .current_dir(&self.side_git_dir)
            .output()?;

        // Copy back to workdir
        let output = Command::new("git")
            .args(["diff", "--name-only", "HEAD", hash])
            .current_dir(&self.side_git_dir)
            .output()?;

        for file in String::from_utf8_lossy(&output.stdout).lines() {
            if file.is_empty() { continue; }
            let src = self.side_git_dir.join(file);
            let dst = self.workdir.join(file);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::copy(&src, &dst);
            }
        }

        Ok(())
    }

    /// Get current git status of the workspace
    pub fn status(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["status", "--short"])
            .current_dir(&self.workdir)
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if workdir is a git repo
    pub fn is_git_repo(&self) -> bool {
        Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(&self.workdir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
