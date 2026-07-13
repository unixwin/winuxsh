//! Reedline REPL loop

use reedline::{
    FileBackedHistory, Reedline, Signal, ReedlineMenu, ListMenu,
};
use crate::completion::WinuxshCompleter;
use crate::shell::Shell;

const HISTORY_SIZE: usize = 10000;

/// Build a `Reedline` instance for the shell.
pub fn build_line_editor(shell: &mut Shell) -> anyhow::Result<Reedline> {
    // History
    let history = FileBackedHistory::with_file(
        HISTORY_SIZE,
        shell.history_path.clone(),
    )
    .map_err(|e| anyhow::anyhow!("failed to open history file {}: {}", shell.history_path.display(), e))?;

    // Completer (must implement reedline::Completer)
    let completer = WinuxshCompleter::new(shell.completion_state.clone());

    // Menu with its own external completer
    let menu = ReedlineMenu::WithCompleter {
        menu: Box::new(ListMenu::default()),
        completer: Box::new(completer),
    };

    let editor = Reedline::create()
        .with_history(Box::new(history))
        .with_menu(menu);

    Ok(editor)
}

/// Run the interactive REPL.
pub fn run_repl(shell: &mut Shell) -> anyhow::Result<()> {
    println!("Winuxsh v2 - rubash + winuxcmd on Windows");
    println!("Type 'exit' to quit. Press Ctrl+D for EOF.");
    println!();

    let mut line_editor = build_line_editor(shell)?;

    loop {
        match line_editor.read_line(&shell.prompt) {
            Ok(Signal::Success(buffer)) => {
                let line = buffer.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "exit" || line == "logout" {
                    break;
                }
                let _ = shell.execute_line(line);
                shell.update_completion_state();
            }
            Ok(Signal::CtrlD) => {
                println!();
                break;
            }
            Ok(Signal::CtrlC) => {
                println!();
                continue;
            }
            Err(e) => {
                eprintln!("winuxsh: line editor error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
