//! End-to-end smoke for the completion pipeline.
//!
//! Builds a CompletionState, registers a fixture dir, then asks for
//! `rg -<Tab>` completions and asserts the expected flags are returned.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use winuxsh_runtime::completion::{CompletionContext, CompletionState};

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
        s.load_completion_dir(&[fixture_dir]);
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
