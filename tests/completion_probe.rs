//! Binary-level completion probe tests.
//!
//! These exercise `Shell::new()` plus the REPL completer without needing to
//! drive reedline through an interactive terminal.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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
fn empty_command_line_suggests_core_commands() {
    let env = ProbeEnv::new("winuxsh-completion-empty");
    let suggestions = run_probe("", &env, &[]);

    assert_contains(&suggestions, "ls");
    assert_contains(&suggestions, "grep");
}

#[test]
fn partial_command_word_suggests_command() {
    let env = ProbeEnv::new("winuxsh-completion-partial");
    let suggestions = run_probe("gre", &env, &[]);

    assert_contains(&suggestions, "grep");
}

#[test]
fn substring_completion_config_suggests_middle_command_match() {
    let env = ProbeEnv::new("winuxsh-completion-substring");
    env.write_config(
        r#"
[completions]
matching = "substring"
"#,
    );

    let suggestions = run_probe("ep", &env, &[]);

    assert_contains(&suggestions, "grep");
}

#[test]
fn command_completion_result_cap_limits_blank_tab() {
    let env = ProbeEnv::new("winuxsh-completion-result-cap");
    env.write_config(
        r#"
[completions]
max_command_results = 1
"#,
    );

    let suggestions = run_probe("", &env, &[]);

    assert_eq!(suggestions.len(), 1, "got {suggestions:?}");
}

#[test]
fn path_command_is_suggested_by_prefix() {
    if !cfg!(windows) {
        return;
    }

    let env = ProbeEnv::new("winuxsh-completion-path");
    let bin = env.root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("probecli.cmd"), "@echo off\r\necho probe\r\n").unwrap();

    let old_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{};{}", native_path(&bin), old_path);
    let suggestions = run_probe(
        "pro",
        &env,
        &[
            ("PATH", path),
            ("PATHEXT", ".COM;.EXE;.BAT;.CMD".to_string()),
        ],
    );

    assert_contains(&suggestions, "probecli");
}

#[test]
fn blank_argument_position_suggests_paths() {
    let env = ProbeEnv::new("winuxsh-completion-path-argument");
    std::fs::create_dir_all(env.start.join("adir")).unwrap();
    std::fs::write(env.start.join("alpha.txt"), "alpha").unwrap();
    std::fs::write(env.start.join(".hidden"), "hidden").unwrap();

    let suggestions = run_probe("ls ", &env, &[]);

    assert_contains(&suggestions, "adir/");
    assert_contains(&suggestions, "alpha.txt");
    assert_not_contains(&suggestions, ".hidden");
    assert_before(&suggestions, "adir/", "alpha.txt");
}

#[test]
fn dot_prefix_suggests_hidden_paths() {
    let env = ProbeEnv::new("winuxsh-completion-hidden-prefix");
    std::fs::write(env.start.join(".hidden"), "hidden").unwrap();

    let suggestions = run_probe("ls .", &env, &[]);

    assert_contains(&suggestions, ".hidden");
}

#[test]
fn cd_blank_argument_position_suggests_directories_only() {
    let env = ProbeEnv::new("winuxsh-completion-cd-argument");
    std::fs::create_dir_all(env.start.join("adir")).unwrap();
    std::fs::write(env.start.join("alpha.txt"), "alpha").unwrap();

    let suggestions = run_probe("cd ", &env, &[]);

    assert_contains(&suggestions, "adir/");
    assert_not_contains(&suggestions, "alpha.txt");
}

#[test]
fn path_completion_preserves_typed_directory_prefix() {
    let env = ProbeEnv::new("winuxsh-completion-prefix");
    let parent = env.start.join("parent");
    std::fs::create_dir_all(parent.join("adir")).unwrap();
    std::fs::write(parent.join("child.txt"), "child").unwrap();

    let directory_suggestions = run_probe("ls parent/", &env, &[]);
    assert_contains(&directory_suggestions, "parent/adir/");
    assert_contains(&directory_suggestions, "parent/child.txt");

    let partial_suggestions = run_probe("ls parent/ch", &env, &[]);
    assert_contains(&partial_suggestions, "parent/child.txt");
}

#[test]
fn path_completion_escapes_spaces_in_candidates() {
    let env = ProbeEnv::new("winuxsh-completion-spaces");
    std::fs::create_dir_all(env.start.join("two dir")).unwrap();
    std::fs::write(env.start.join("two words.txt"), "two").unwrap();

    let suggestions = run_probe("ls tw", &env, &[]);

    assert_contains(&suggestions, "two\\ dir/");
    assert_contains(&suggestions, "two\\ words.txt");
    assert_before(&suggestions, "two\\ dir/", "two\\ words.txt");
}

#[test]
fn case_sensitive_completion_config_respects_path_case() {
    let env = ProbeEnv::new("winuxsh-completion-case-sensitive");
    env.write_config(
        r#"
[completions]
case_sensitive = true
"#,
    );
    std::fs::write(env.start.join("Alpha.txt"), "alpha").unwrap();

    let lower = run_probe("ls a", &env, &[]);
    assert_not_contains(&lower, "Alpha.txt");

    let upper = run_probe("ls A", &env, &[]);
    assert_contains(&upper, "Alpha.txt");
}

#[test]
fn path_completion_matches_escaped_spaces_in_input() {
    let env = ProbeEnv::new("winuxsh-completion-escaped-input");
    let parent = env.start.join("parent dir");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::write(env.start.join("two words.txt"), "two").unwrap();
    std::fs::write(parent.join("child.txt"), "child").unwrap();

    let file_suggestions = run_probe("ls two\\ w", &env, &[]);
    assert_contains(&file_suggestions, "two\\ words.txt");

    let nested_suggestions = run_probe("ls parent\\ dir/ch", &env, &[]);
    assert_contains(&nested_suggestions, "parent\\ dir/child.txt");
}

#[test]
fn path_completion_matches_double_quoted_input() {
    let env = ProbeEnv::new("winuxsh-completion-quoted-input");
    std::fs::write(env.start.join("two words.txt"), "two").unwrap();

    let suggestions = run_probe("ls \"two w", &env, &[]);

    assert_contains(&suggestions, "\"two words.txt\"");
}

#[test]
fn command_position_after_pipe_suggests_command() {
    let env = ProbeEnv::new("winuxsh-completion-pipe");
    let suggestions = run_probe("ls | gre", &env, &[]);

    assert_contains(&suggestions, "grep");
}

#[test]
fn blank_command_position_after_pipe_suggests_commands() {
    let env = ProbeEnv::new("winuxsh-completion-pipe-empty");
    let suggestions = run_probe("ls | ", &env, &[]);

    assert_contains(&suggestions, "grep");
    assert_contains(&suggestions, "ls");
}

#[test]
fn argument_position_does_not_suggest_commands() {
    let env = ProbeEnv::new("winuxsh-completion-arg");
    let suggestions = run_probe("echo gre", &env, &[]);

    assert_not_contains(&suggestions, "grep");
}

fn run_probe(line: &str, env: &ProbeEnv, extra_env: &[(&str, String)]) -> Vec<String> {
    let output = run_winuxsh_probe(line, &env.start, &env.home, extra_env);
    assert_success(&output, line);
    stdout_lines(&output)
}

fn run_winuxsh_probe(line: &str, cwd: &Path, home: &Path, extra_env: &[(&str, String)]) -> Output {
    let mut command = Command::new(winuxsh_binary());
    command
        .arg("--completion-probe")
        .arg(line)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("WINUXSH_CONFIG", home.join(".winshrc.toml"))
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
        "completion probe for {context:?} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
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

fn assert_contains(values: &[String], expected: &str) {
    assert!(
        values.iter().any(|value| value == expected),
        "expected {expected:?}, got {values:?}"
    );
}

fn assert_not_contains(values: &[String], unexpected: &str) {
    assert!(
        !values.iter().any(|value| value == unexpected),
        "did not expect {unexpected:?}, got {values:?}"
    );
}

fn assert_before(values: &[String], earlier: &str, later: &str) {
    let earlier_index = values
        .iter()
        .position(|value| value == earlier)
        .unwrap_or_else(|| panic!("missing {earlier:?} in {values:?}"));
    let later_index = values
        .iter()
        .position(|value| value == later)
        .unwrap_or_else(|| panic!("missing {later:?} in {values:?}"));
    assert!(
        earlier_index < later_index,
        "expected {earlier:?} before {later:?}, got {values:?}"
    );
}

fn native_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

struct ProbeEnv {
    root: PathBuf,
    home: PathBuf,
    start: PathBuf,
}

impl ProbeEnv {
    fn new(prefix: &str) -> Self {
        let root = unique_temp_dir(prefix);
        let home = root.join("home");
        let start = root.join("start");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&start).unwrap();
        Self { root, home, start }
    }

    fn write_config(&self, content: &str) {
        std::fs::write(self.home.join(".winshrc.toml"), content).unwrap();
    }
}

impl Drop for ProbeEnv {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
}
