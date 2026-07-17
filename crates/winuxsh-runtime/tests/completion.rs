//! End-to-end smoke for the completion pipeline.
//!
//! Builds a CompletionState, registers a fixture dir, then asks for
//! `rg -<Tab>` completions and asserts the expected flags are returned.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use winuxsh_runtime::completion::{CompletionContext, CompletionState};
use winuxsh_runtime::completion::external::{CommandDef, FlagDef};

#[test]
fn loads_toml_definitions_from_dir() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("completions");

    let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("."))));
    {
        let mut s = state.lock().unwrap();
        s.load_completion_dirs(&[fixture_dir]);
    }

    // Build a context where the cursor is right after `rg -`
    let input = "rg -".to_string();
    let ctx = CompletionContext::new(PathBuf::from("."), input.clone(), input.len());

    let s = state.lock().unwrap();
    let suggestions: Vec<String> = s
        .plugins
        .iter()
        .flat_map(|p| p.complete(&ctx).map(|r| r.completions).unwrap_or_default())
        .collect();

    // We expect at least the long flags we defined in rg.toml
    assert!(
        suggestions.iter().any(|s| s == "--ignore-case"),
        "expected --ignore-case in suggestions, got: {:?}",
        suggestions
    );
    assert!(
        suggestions.iter().any(|s| s == "--regexp"),
        "expected --regexp in suggestions, got: {:?}",
        suggestions
    );
    assert!(
        suggestions.iter().any(|s| s == "--type"),
        "expected --type in suggestions, got: {:?}",
        suggestions
    );
}

#[test]
fn loads_builtin_winuxcmd_definitions_without_user_dirs() {
    let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("."))));
    {
        let mut s = state.lock().unwrap();
        s.load_completion_dirs(&[]);
    }

    assert_suggests(&state, "ls -", "--all");
    assert_suggests(&state, "grep -", "--ignore-case");
    assert_suggests(&state, "find -", "-name");
    assert_suggests(&state, "cat -", "--number");
    assert_suggests(&state, "cp -", "--recursive");
    assert_suggests(&state, "mv -", "--target-directory");
    assert_suggests(&state, "rm -", "--force");
    assert_suggests(&state, "mkdir -", "--parents");
    assert_suggests(&state, "touch -", "--no-create");
    assert_suggests(&state, "chmod -", "--recursive");
}

#[test]
fn user_toml_overrides_builtin_definition() {
    let temp_dir = unique_temp_dir("winuxsh-completion-override");
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(
        temp_dir.join("ls.toml"),
        r#"
command = "ls"
description = "test override"

[[flags]]
long = "--custom-only"
description = "fixture override flag"
"#,
    )
    .unwrap();

    let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("."))));
    {
        let mut s = state.lock().unwrap();
        s.load_completion_dirs(&[temp_dir.clone()]);
    }

    assert_suggests(&state, "ls -", "--custom-only");
    assert_not_suggests(&state, "ls -", "--all");

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn translated_zsh_definitions_are_loaded_before_user_dirs() {
    let imported = CommandDef {
        command: "ztool".to_string(),
        description: Some("imported from zsh".to_string()),
        flags: vec![FlagDef {
            short: None,
            long: Some("--zsh-imported".to_string()),
            description: Some("zsh imported flag".to_string()),
            takes_value: false,
            values_source: None,
        }],
        subcommands: Vec::new(),
    };

    let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("."))));
    {
        let mut s = state.lock().unwrap();
        s.load_completion_dirs_with_definitions(&[], vec![imported]);
    }

    assert_suggests(&state, "ztool -", "--zsh-imported");
}

#[test]
fn translated_zsh_definitions_merge_with_builtins() {
    let imported = CommandDef {
        command: "ls".to_string(),
        description: Some("imported zsh ls".to_string()),
        flags: vec![FlagDef {
            short: None,
            long: Some("--zsh-extra".to_string()),
            description: Some("extra zsh flag".to_string()),
            takes_value: false,
            values_source: None,
        }],
        subcommands: Vec::new(),
    };

    let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("."))));
    {
        let mut s = state.lock().unwrap();
        s.load_completion_dirs_with_definitions(&[], vec![imported]);
    }

    assert_suggests(&state, "ls -", "--all");
    assert_suggests(&state, "ls -", "--zsh-extra");
}

#[test]
fn user_toml_overrides_translated_zsh_definitions() {
    let temp_dir = unique_temp_dir("winuxsh-zsh-completion-override");
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(
        temp_dir.join("ztool.toml"),
        r#"
command = "ztool"
description = "user override"

[[flags]]
long = "--user-only"
description = "user override flag"
"#,
    )
    .unwrap();

    let imported = CommandDef {
        command: "ztool".to_string(),
        description: Some("imported from zsh".to_string()),
        flags: vec![FlagDef {
            short: None,
            long: Some("--zsh-imported".to_string()),
            description: Some("zsh imported flag".to_string()),
            takes_value: false,
            values_source: None,
        }],
        subcommands: Vec::new(),
    };

    let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("."))));
    {
        let mut s = state.lock().unwrap();
        s.load_completion_dirs_with_definitions(&[temp_dir.clone()], vec![imported]);
    }

    assert_suggests(&state, "ztool -", "--user-only");
    assert_not_suggests(&state, "ztool -", "--zsh-imported");

    let _ = std::fs::remove_dir_all(temp_dir);
}

fn suggestions_for(state: &Arc<Mutex<CompletionState>>, input: &str) -> Vec<String> {
    let ctx = CompletionContext::new(PathBuf::from("."), input.to_string(), input.len());
    let s = state.lock().unwrap();
    s.plugins
        .iter()
        .flat_map(|p| p.complete(&ctx).map(|r| r.completions).unwrap_or_default())
        .collect()
}

fn assert_suggests(state: &Arc<Mutex<CompletionState>>, input: &str, expected: &str) {
    let suggestions = suggestions_for(state, input);
    assert!(
        suggestions.iter().any(|s| s == expected),
        "expected {expected} for {input:?}, got: {:?}",
        suggestions
    );
}

fn assert_not_suggests(state: &Arc<Mutex<CompletionState>>, input: &str, unexpected: &str) {
    let suggestions = suggestions_for(state, input);
    assert!(
        !suggestions.iter().any(|s| s == unexpected),
        "did not expect {unexpected} for {input:?}, got: {:?}",
        suggestions
    );
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
}
