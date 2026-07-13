//! Compat test runner.
//!
//! Each `<name>.sh` in `tests/compat/fixtures/` is executed via the built
//! `winuxsh` binary, and its stdout is compared against `<name>.expected`.
//!
//! Tests are marked `#[ignore]` because they require winuxcmd.exe to be
//! discoverable in PATH (which is the case on developer machines and CI when
//! winuxcmd is installed). Run with:
//!
//!   cargo test --test compat -- --ignored

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn winuxsh_binary() -> PathBuf {
    // cargo test builds the bin to target/<profile>/winuxsh[.exe]
    let p = PathBuf::from(env!("CARGO_BIN_EXE_winuxsh"));
    if !p.exists() {
        // fall back to the known target dir layout
        let mut fallback = repo_root();
        fallback.push("target");
        fallback.push("debug");
        fallback.push(if cfg!(windows) { "winuxsh.exe" } else { "winuxsh" });
        if fallback.exists() {
            return fallback;
        }
    }
    p
}

fn fixtures_dir() -> PathBuf {
    let mut p = repo_root();
    p.push("tests");
    p.push("compat");
    p.push("fixtures");
    p
}

fn normalize(s: &str) -> String {
    // strip trailing whitespace per line; collapse CRLF -> LF; trim trailing newlines
    s.replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn run_case(name: &str) {
    let dir = fixtures_dir();
    let script = dir.join(format!("{name}.sh"));
    let expected_path = dir.join(format!("{name}.expected"));

    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));

    let script_content =
        fs::read_to_string(&script).unwrap_or_else(|e| panic!("read {}: {e}", script.display()));

    // Run via `winuxsh -c <script_content>` so we exercise the same path as
    // interactive use without relying on the line-by-line script-file reader
    // (which still has heredoc/continuation gaps tracked as T-4).
    let bin = winuxsh_binary();
    assert!(bin.exists(), "winuxsh binary not found at {}", bin.display());

    let output = Command::new(&bin)
        .arg("-c")
        .arg(&script_content)
        .output()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", bin.display()));

    if !output.stderr.is_empty() {
        eprintln!(
            "[{name}] stderr: {}",
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }

    let actual = String::from_utf8_lossy(&output.stdout);
    let actual_norm = normalize(&actual);
    let expected_norm = normalize(&expected);
    assert_eq!(
        actual_norm, expected_norm,
        "[{name}] mismatch\n--- expected ---\n{expected_norm}\n--- actual ---\n{actual_norm}\n"
    );
}

macro_rules! compat_test {
    ($name:ident, $label:literal) => {
        #[test]
        #[ignore = "requires winuxcmd in PATH; run with --ignored"]
        fn $name() {
            run_case($label);
        }
    };
}

compat_test!(var_expansion, "var_expansion");
compat_test!(command_substitution, "command_substitution");
compat_test!(pipeline, "pipeline");
compat_test!(if_else, "if_else");
compat_test!(for_loop, "for_loop");
compat_test!(function, "function");
compat_test!(alias, "alias");
compat_test!(exit_code, "exit_code");
compat_test!(string_param, "string_param");
compat_test!(echo_flags, "echo_flags");
