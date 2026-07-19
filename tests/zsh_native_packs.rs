//! Binary-level tests for native zsh pack inventory commands.

use std::path::PathBuf;
use std::process::{Command, Output};

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
fn zsh_native_packs_text_lists_preinstalled_pack_contract() {
    let output = run_winuxsh("--zsh-native-packs");
    assert_success(&output, "zsh native packs text");
    let stdout = stdout_text(&output);

    assert!(stdout.contains("Native zsh plugin packs"), "{stdout}");
    assert!(
        stdout.contains("no Oh My Zsh or zsh plugin source is vendored or sourced"),
        "{stdout}"
    );
    assert!(
        stdout.contains("- git kind=alias tier=profile default=off"),
        "{stdout}"
    );
    assert!(
        stdout.contains("- zsh-autosuggestions kind=widget tier=always_on default=on"),
        "{stdout}"
    );
    assert!(
        stdout.contains("- direnv kind=lifecycle tier=explicit_trust default=off"),
        "{stdout}"
    );
}

#[test]
fn zsh_native_packs_json_lists_machine_readable_packs() {
    let output = run_winuxsh("--zsh-native-packs-json");
    assert_success(&output, "zsh native packs json");
    let stdout = stdout_text(&output);

    assert!(stdout.contains(r#""name": "git""#), "{stdout}");
    assert!(stdout.contains(r#""risk_tier": "profile""#), "{stdout}");
    assert!(
        stdout.contains(r#""name": "zsh-autosuggestions""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""startup_default": true"#), "{stdout}");
}

fn run_winuxsh(arg: &str) -> Output {
    Command::new(winuxsh_binary())
        .arg(arg)
        .output()
        .unwrap_or_else(|err| panic!("failed to run winuxsh {arg}: {err}"))
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed: status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        stdout_text(output),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}
