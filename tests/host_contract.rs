//! Windows-native host contract tests for the `winuxsh -c` surface.
//!
//! These tests intentionally exercise the built binary instead of internal
//! helpers: this is the contract humans and agents rely on.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn winuxsh_binary() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_BIN_EXE_winuxsh"));
    if p.exists() {
        return p;
    }

    let mut fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fallback.push("target");
    fallback.push("debug");
    fallback.push(if cfg!(windows) {
        "winuxsh.exe"
    } else {
        "winuxsh"
    });
    fallback
}

#[test]
fn cwd_cd_pwd_and_windows_child_process_agree() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-cwd");
    let home = temp.join("home");
    let start = temp.join("start");
    let target = temp.join("target");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();
    std::fs::create_dir_all(&target).unwrap();

    let target_shell_path = shell_path(&target);
    let script = format!("cd {}; pwd; cmd.exe /C cd", shell_quote(&target_shell_path));
    let output = run_winuxsh(&script, &start, &home, &[]);
    assert_success(&output, "cwd contract");

    let stdout = stdout_lines(&output);
    assert_eq!(stdout.len(), 2, "stdout was {stdout:?}");
    assert_same_path(&stdout[0], &target_shell_path);
    assert_same_path(&stdout[1], &target_shell_path);
    assert!(
        !stdout[0].starts_with("/c/"),
        "pwd must prefer Windows-native drive paths, got {:?}",
        stdout[0]
    );

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn slash_drive_paths_are_compat_input_not_default_output() {
    if !cfg!(windows) {
        return;
    }

    let users = PathBuf::from(r"C:\Users");
    if !users.is_dir() {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-slash-drive");
    let home = temp.join("home");
    let start = temp.join("start");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();

    let output = run_winuxsh("cd /c/Users; pwd", &start, &home, &[]);
    assert_success(&output, "slash-drive cwd contract");

    let stdout = stdout_lines(&output);
    assert_eq!(stdout.len(), 1, "stdout was {stdout:?}");
    assert_same_path(&stdout[0], "C:/Users");
    assert!(
        !stdout[0].starts_with("/c/"),
        "compat input must not become default output, got {:?}",
        stdout[0]
    );

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn native_backslash_drive_paths_work_for_winuxcmd_and_cd() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-native-path");
    let home = temp.join("home");
    let start = temp.join("start");
    let target = temp.join("target");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("marker.txt"), "ok").unwrap();

    let target_native_path = native_path(&target);
    let output = run_winuxsh(&format!("ls {}", target_native_path), &start, &home, &[]);
    assert_success(&output, "native backslash path ls");
    let stdout = stdout_lines(&output);
    assert!(
        stdout.iter().any(|line| line.contains("marker.txt")),
        "ls output did not include marker.txt: {stdout:?}"
    );

    let output = run_winuxsh(
        &format!("cd {}; pwd", target_native_path),
        &start,
        &home,
        &[],
    );
    assert_success(&output, "native backslash path cd");
    let stdout = stdout_lines(&output);
    assert_eq!(stdout.len(), 1, "stdout was {stdout:?}");
    assert_same_path(&stdout[0], &shell_path(&target));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn path_lookup_finds_windows_pathextext_commands() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-path");
    let home = temp.join("home");
    let start = temp.join("start");
    let bin = temp.join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("hostcontractprobe.cmd"),
        "@echo off\r\necho path-ok\r\n",
    )
    .unwrap();

    let old_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{};{}", native_path(&bin), old_path);
    let output = run_winuxsh(
        "hostcontractprobe",
        &start,
        &home,
        &[
            ("PATH", path),
            ("PATHEXT", ".COM;.EXE;.BAT;.CMD".to_string()),
        ],
    );
    assert_success(&output, "PATH contract");
    assert_eq!(normalize_text(&output.stdout), "path-ok");

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn exported_env_reaches_windows_child_processes() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-env");
    let home = temp.join("home");
    let start = temp.join("start");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();

    let output = run_winuxsh(
        "export WINUXSH_HOST_CONTRACT=ok; cmd.exe /C echo %WINUXSH_HOST_CONTRACT%",
        &start,
        &home,
        &[],
    );
    assert_success(&output, "env contract");
    assert_eq!(normalize_text(&output.stdout), "ok");

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn tilde_resolves_to_normal_windows_home() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-home");
    let home = temp.join("home");
    let start = temp.join("start");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();

    let output = run_winuxsh("cd ~; pwd; cmd.exe /C cd", &start, &home, &[]);
    assert_success(&output, "home contract");

    let expected_home = shell_path(&home);
    let stdout = stdout_lines(&output);
    assert_eq!(stdout.len(), 2, "stdout was {stdout:?}");
    assert_same_path(&stdout[0], &expected_home);
    assert_same_path(&stdout[1], &expected_home);

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn stdout_stderr_and_exit_code_are_preserved() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-stdio");
    let home = temp.join("home");
    let start = temp.join("start");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();

    let output = run_winuxsh("echo out; echo err >&2; exit 7", &start, &home, &[]);
    assert_eq!(
        output.status.code(),
        Some(7),
        "expected exit code 7, got {:?}; stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(normalize_text(&output.stdout), "out");
    assert_eq!(normalize_text(&output.stderr), "err");

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn piped_stdin_without_args_runs_plain_script_surface() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-piped-stdin");
    let home = temp.join("home");
    let start = temp.join("start");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();

    let mut child = Command::new(winuxsh_binary())
        .current_dir(&start)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("ZDOTDIR", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("spawn winuxsh: {err}"));

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"printf 'alpha\\nbeta\\n' | grep alpha\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert_success(&output, "piped stdin script surface");
    assert_eq!(normalize_text(&output.stdout), "alpha");
    assert_eq!(normalize_text(&output.stderr), "");
    assert_no_terminal_controls(&output.stdout);
    assert_no_terminal_controls(&output.stderr);

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn command_mode_grep_capture_stays_plain() {
    if !cfg!(windows) {
        return;
    }

    let temp = unique_temp_dir("winuxsh-host-grep-capture");
    let home = temp.join("home");
    let start = temp.join("start");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&start).unwrap();

    let output = run_winuxsh("printf 'alpha\nbeta\n' | grep alpha", &start, &home, &[]);
    assert_success(&output, "captured grep");
    assert_eq!(normalize_text(&output.stdout), "alpha");
    assert_no_terminal_controls(&output.stdout);
    assert_no_terminal_controls(&output.stderr);

    let _ = std::fs::remove_dir_all(temp);
}

fn run_winuxsh(script: &str, cwd: &Path, home: &Path, extra_env: &[(&str, String)]) -> Output {
    let mut command = Command::new(winuxsh_binary());
    command
        .arg("-c")
        .arg(script)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("ZDOTDIR", home);

    for (key, value) in extra_env {
        command.env(key, value);
    }

    command
        .output()
        .unwrap_or_else(|err| panic!("spawn winuxsh: {err}"))
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout_lines(output: &Output) -> Vec<String> {
    normalize_text(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn normalize_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .trim()
        .to_string()
}

fn assert_no_terminal_controls(bytes: &[u8]) {
    for byte in bytes {
        assert_ne!(*byte, 0x1b, "unexpected ANSI escape in output");
        assert!(
            *byte >= 0x20 || matches!(*byte, b'\t' | b'\n' | b'\r'),
            "unexpected control byte 0x{byte:02X} in output"
        );
    }
}

fn assert_same_path(actual: &str, expected: &str) {
    assert_eq!(
        comparable_path(actual),
        comparable_path(expected),
        "path mismatch: actual={actual:?}, expected={expected:?}"
    );
}

fn comparable_path(value: &str) -> String {
    let mut value = value.trim().replace('\\', "/");
    if cfg!(windows) && value.len() >= 2 && value.as_bytes()[1] == b':' {
        let drive = value[0..1].to_ascii_uppercase();
        value.replace_range(0..1, &drive);
    }
    value.trim_end_matches('/').to_string()
}

fn shell_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn native_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
}
