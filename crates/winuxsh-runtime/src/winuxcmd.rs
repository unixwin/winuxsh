//! winuxcmd integration via PATH injection.
//!
//! rubash's `Executor` looks up external commands via `find_user_command()`,
//! which walks `PATH`. We don't use FFI/DLL — we just prepend the directory
//! containing `winuxcmd.exe` to the process `PATH` so rubash finds it first.

use std::path::PathBuf;
use std::process::Command;
use anyhow::{anyhow, Result};

/// Locate `winuxcmd.exe` by checking, in order:
///   1. `$WINUXCMD_PATH` env var (file or directory)
///   2. `<exe_dir>/winuxcmd/winuxcmd.exe`
///   3. `<exe_dir>/utils/winuxcmd/winuxcmd.exe`
///   4. `winuxcmd.exe` reachable via current `PATH`
pub fn find_winuxcmd() -> Option<PathBuf> {
    // 1. WINUXCMD_PATH override
    if let Ok(p) = std::env::var("WINUXCMD_PATH") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Some(path);
        }
        if path.is_dir() {
            let candidate = path.join("winuxcmd.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 2/3. Relative to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for rel in ["winuxcmd/winuxcmd.exe", "utils/winuxcmd/winuxcmd.exe"] {
                let candidate = exe_dir.join(rel);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // 4. PATH lookup using `where.exe` on Windows
    #[cfg(windows)]
    {
        if let Ok(out) = Command::new("where.exe").arg("winuxcmd.exe").output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(line) = text.lines().next() {
                    let p = PathBuf::from(line.trim());
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }

    None
}

/// Prepend the directory containing `winuxcmd.exe` to `PATH` so rubash's
/// command lookup finds winuxcmd-provided coreutils first. Returns the
/// directory that was injected, or an error if winuxcmd couldn't be found.
pub fn ensure_on_path() -> Result<PathBuf> {
    let exe = find_winuxcmd().ok_or_else(|| anyhow!("winuxcmd.exe not found (looked in WINUXCMD_PATH, exe dir, and PATH)"))?;
    let dir = exe.parent().ok_or_else(|| anyhow!("winuxcmd.exe has no parent directory"))?.to_path_buf();

    let current_path = std::env::var("PATH").unwrap_or_default();
    let dir_str = dir.to_string_lossy().to_string();

    // Skip if already at the front (idempotent).
    if current_path.starts_with(&dir_str) {
        return Ok(dir);
    }

    // Avoid duplicating the entry elsewhere in PATH.
    let mut parts: Vec<String> = current_path
        .split(if cfg!(windows) { ';' } else { ':' })
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != &dir_str)
        .collect();
    parts.insert(0, dir_str.clone());
    let new_path = parts.join(if cfg!(windows) { ";" } else { ":" });
    // On Windows, `std::env::set_var` normalizes &quot;PATH&quot; to &quot;Path&quot;.
    // Rubash internally uses `env_vars.get(&quot;PATH&quot;)` (all caps), which
    // is case-sensitive in the HashMap.  Force the all-caps entry so rubash
    // can find it.
    #[cfg(windows)]
    std::env::set_var("PATH", &new_path);
    log::debug!("winuxcmd PATH injected: {}", dir_str);
    Ok(dir)
}

/// Run `winuxcmd.exe --version` and return the first line of stdout.
pub fn version() -> Option<String> {
    let exe = find_winuxcmd()?;
    let out = Command::new(&exe).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|s| s.to_string())
}

/// List all commands provided by winuxcmd by scanning its directory for
/// sibling `*.exe` shims, or by invoking `winuxcmd.exe --list` if supported.
pub fn list_commands() -> Vec<String> {
    // Try `--list` first (forward-compat with future winuxcmd versions).
    if let Some(exe) = find_winuxcmd() {
        if let Ok(out) = Command::new(&exe).arg("--list").output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let cmds: Vec<String> = text
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !cmds.is_empty() {
                    return cmds;
                }
            }
        }
    }

    // Fallback: scan the directory for *.exe shims.
    if let Some(exe) = find_winuxcmd() {
        if let Some(dir) = exe.parent() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut cmds: Vec<String> = Vec::new();
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()).map(|s| s.eq_ignore_ascii_case("exe")).unwrap_or(false) {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem != "winuxcmd" {
                                cmds.push(stem.to_string());
                            }
                        }
                    }
                }
                cmds.sort();
                return cmds;
            }
        }
    }

    Vec::new()
}


