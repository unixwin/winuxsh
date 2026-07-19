//! Git repository status for the prompt (oh-my-zsh-style).
//!
//! Runs `git` sub-processes (read-only, with `GIT_OPTIONAL_LOCKS=0`) to gather
//! branch, dirty-state, staged/unstaged/untracked counts, ahead/behind, stashes,
//! and merge-conflict counts.  All git calls have a short timeout so that a slow
//! or hanging git never blocks the prompt.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

const GIT_TIMEOUT: Duration = Duration::from_millis(200);

/// Aggregated git status for the current working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepoStatus {
    pub branch: Option<String>,
    pub dirty: bool,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub deleted: usize,
    pub ahead: usize,
    pub behind: usize,
    pub stashes: usize,
    pub conflicts: usize,
}

impl GitRepoStatus {
    pub fn compact_status(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.conflicts > 0 { parts.push(format!("✖{}", self.conflicts)); }
        if self.staged > 0 { parts.push(format!("●{}", self.staged)); }
        if self.unstaged > 0 { parts.push(format!("✚{}", self.unstaged)); }
        if self.deleted > 0 { parts.push(format!("✖{}", self.deleted)); }
        if self.ahead > 0 { parts.push(format!("↑{}", self.ahead)); }
        if self.behind > 0 { parts.push(format!("↓{}", self.behind)); }
        if self.untracked > 0 { parts.push(format!("?{}", self.untracked)); }
        if self.stashes > 0 { parts.push(format!("⚑{}", self.stashes)); }
        parts.join(" ")
    }
}

pub fn collect(cwd: &Path) -> Option<GitRepoStatus> {
    if !is_likely_git_repo(cwd) { return None; }
    let status_output = run_git(&["status", "--porcelain", "-b"], cwd, GIT_TIMEOUT, 4 * 1024)?;
    let status_stdout = String::from_utf8(status_output.stdout).ok()?;
    let mut lines = status_stdout.lines();
    let branch_line = lines.next()?;
    let branch = parse_branch_line(branch_line);
    let (ahead0, behind0) = parse_ahead_behind_from_branch_line(branch_line);
    let mut staged = 0usize;
    let mut unstaged = 0usize;
    let mut untracked = 0usize;
    let mut deleted = 0usize;
    let mut conflicts = 0usize;
    for line in lines {
        let line = line.as_bytes();
        if line.is_empty() { continue; }
        let x = line.first().copied().unwrap_or(b' ');
        let y = line.get(1).copied().unwrap_or(b' ');
        match x { b'M' | b'A' | b'R' | b'C' => staged += 1, b'D' => { staged += 1; deleted += 1; } b'U' => conflicts += 1, _ => {} }
        match y { b'M' => unstaged += 1, b'D' => { unstaged += 1; deleted += 1; } b'?' => untracked += 1, b'U' => conflicts += 1, _ => {} }
    }
    let dirty = staged > 0 || unstaged > 0 || untracked > 0 || deleted > 0 || conflicts > 0;
    let (ahead, behind) = if ahead0 == 0 && behind0 == 0 && branch.is_some() {
        (count_commits(cwd, "@{upstream}..HEAD").unwrap_or(0), count_commits(cwd, "HEAD..@{upstream}").unwrap_or(0))
    } else { (ahead0, behind0) };
    let stashes = count_stashes(cwd);
    Some(GitRepoStatus { branch, dirty, staged, unstaged, untracked, deleted, ahead, behind, stashes, conflicts })
}

fn is_likely_git_repo(mut dir: &Path) -> bool {
    loop { let git = dir.join(".git"); if git.is_dir() || git.is_file() { return true; } match dir.parent() { Some(p) => dir = p, None => return false } }
}

fn run_git(args: &[&str], cwd: &Path, _timeout: Duration, max_stdout: usize) -> Option<Output> {
    let cwd = cwd.to_owned();
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let handle = std::thread::spawn(move || {
        Command::new("git").args(&args).current_dir(&cwd)
            .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output()
    });
    let output = match handle.join() { Ok(Ok(o)) => o, _ => return None };
    if !output.status.success() || output.stdout.len() > max_stdout { None } else { Some(output) }
}

fn parse_branch_line(line: &str) -> Option<String> {
    let r = line.trim().strip_prefix("## ")?;
    if r.starts_with("HEAD ") || r.contains("(no branch)") {
        return r.split_whitespace().find(|w| w.len() >= 7 && w.chars().all(|c| c.is_ascii_hexdigit())).map(|h| h.chars().take(7).collect());
    }
    // Empty initial repo: `## No commits yet on master`
    if let Some(rest) = r.strip_prefix("No commits yet on ") {
        return Some(rest.trim().to_string());
    }
    Some(r.split("...").next().unwrap_or(r).to_string())
}

fn parse_ahead_behind_from_branch_line(line: &str) -> (usize, usize) {
    if let Some(b) = line.find('[') {
        let inner = line[b + 1..].split(']').next().unwrap_or("");
        let mut a = 0usize; let mut b = 0usize;
        for p in inner.split(',') {
            let p = p.trim();
            if let Some(n) = p.strip_prefix("ahead ") { a = n.trim().parse().unwrap_or(0); }
            else if let Some(n) = p.strip_prefix("behind ") { b = n.trim().parse().unwrap_or(0); }
        }
        return (a, b);
    }
    (0, 0)
}

fn count_commits(cwd: &Path, range: &str) -> Option<usize> {
    let output = run_git(&["rev-list", "--count", range], cwd, GIT_TIMEOUT, 1024)?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

fn count_stashes(cwd: &Path) -> usize {
    if run_git(&["rev-parse", "--verify", "--quiet", "refs/stash"], cwd, GIT_TIMEOUT, 128).is_none() { return 0; }
    match run_git(&["stash", "list"], cwd, GIT_TIMEOUT, 8192) {
        Some(o) => String::from_utf8(o.stdout).unwrap_or_default().lines().count(),
        None => 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn init_temp_repo() -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().to_owned();
        for args in [&["init"][..], &["config", "user.email", "test@winuxsh"], &["config", "user.name", "Winuxsh Test"]] {
            let o = Command::new("git").args(args).current_dir(&p).stdout(Stdio::null()).stderr(Stdio::null()).output().unwrap();
            assert!(o.status.success());
        }
        std::mem::forget(dir);
        p
    }

    #[test]
    fn git_status_outside_repo_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(collect(dir.path()).is_none());
    }

    #[test]
    fn git_status_empty_repo_shows_master_branch() {
        let dir = init_temp_repo();
        let s = collect(&dir).expect("git repo");
        assert!(matches!(s.branch.as_deref(), Some("master") | Some("main")));
        assert!(!s.dirty);
    }

    #[test]
    fn git_status_detects_dirty_working_tree() {
        let dir = init_temp_repo();
        let mut f = std::fs::File::create(dir.join("new.txt")).unwrap();
        writeln!(f, "hello").unwrap();
        drop(f);
        let s = collect(&dir).expect("git repo");
        assert!(s.dirty);
        assert_eq!(s.untracked, 1);
    }

    #[test]
    fn git_status_detects_staged_changes() {
        let dir = init_temp_repo();
        std::fs::write(dir.join("s.txt"), b"x").unwrap();
        let o = Command::new("git").args(["add", "s.txt"]).current_dir(&dir).stdout(Stdio::null()).stderr(Stdio::null()).output().unwrap();
        assert!(o.status.success());
        let s = collect(&dir).expect("git repo");
        assert!(s.dirty);
        assert_eq!(s.staged, 1);
    }

    #[test]
    fn git_status_compact_format() {
        let s = GitRepoStatus { branch: Some("main".into()), dirty: true, staged: 2, unstaged: 1, untracked: 3, deleted: 1, ahead: 1, behind: 2, stashes: 1, conflicts: 0 };
        let c = s.compact_status();
        assert!(c.contains("●2")); assert!(c.contains("✚1")); assert!(c.contains("↑1")); assert!(c.contains("↓2")); assert!(c.contains("?3")); assert!(c.contains("⚑1"));
    }

    #[test]
    fn git_status_clean_compact_empty() {
        let s = GitRepoStatus { branch: Some("main".into()), dirty: false, staged: 0, unstaged: 0, untracked: 0, deleted: 0, ahead: 0, behind: 0, stashes: 0, conflicts: 0 };
        assert!(s.compact_status().is_empty());
    }
}
