//! Reedline REPL loop

use reedline::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    EditCommand, EditMode, Emacs, FileBackedHistory, KeyCode, KeyModifiers, Keybindings, ListMenu,
    MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, Vi,
};

use crate::autosuggest::HistoryAutosuggestHinter;
use crate::completion::WinuxshCompleter;
use crate::config::EditorMode;
use crate::shell::Shell;
use crate::syntax_highlighting::WinuxshSyntaxHighlighter;

const HISTORY_SIZE: usize = 10000;
const COMPLETION_MENU: &str = "completion_menu";
const HISTORY_MENU: &str = "history_menu";

/// Build a `Reedline` instance for the shell.
pub fn build_line_editor(shell: &mut Shell) -> anyhow::Result<Reedline> {
    let history = FileBackedHistory::with_file(HISTORY_SIZE, shell.history_path.clone()).map_err(
        |e| {
            anyhow::anyhow!(
                "failed to open history file {}: {}",
                shell.history_path.display(),
                e
            )
        },
    )?;

    let completer = WinuxshCompleter::new(shell.completion_state.clone());

    let completion_menu = ReedlineMenu::WithCompleter {
        menu: Box::new(ListMenu::default().with_name(COMPLETION_MENU)),
        completer: Box::new(completer),
    };
    let history_menu = ReedlineMenu::HistoryMenu(Box::new(
        ListMenu::default().with_name(HISTORY_MENU),
    ));

    let mut editor = Reedline::create()
        .with_history(Box::new(history))
        .with_menu(completion_menu)
        .with_menu(history_menu)
        .with_edit_mode(build_edit_mode(shell.editor_mode));

    if shell.autosuggest.history_strategy_enabled() {
        editor = editor.with_hinter(Box::new(HistoryAutosuggestHinter::new(
            &shell.autosuggest,
        )));
    }
    if shell.syntax_highlighting.main_highlighter_enabled() {
        editor = editor.with_highlighter(Box::new(WinuxshSyntaxHighlighter::new(
            &shell.syntax_highlighting,
        )));
    }

    Ok(editor)
}

fn build_edit_mode(mode: EditorMode) -> Box<dyn EditMode> {
    match mode {
        EditorMode::Emacs => {
            let mut keybindings = default_emacs_keybindings();
            add_menu_keybindings(&mut keybindings);
            Box::new(Emacs::new(keybindings))
        }
        EditorMode::Vi => {
            let mut insert_keybindings = default_vi_insert_keybindings();
            let mut normal_keybindings = default_vi_normal_keybindings();
            add_menu_keybindings(&mut insert_keybindings);
            add_menu_keybindings(&mut normal_keybindings);
            Box::new(Vi::new(insert_keybindings, normal_keybindings))
        }
    }
}

fn add_menu_keybindings(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu(COMPLETION_MENU.to_string()),
            ReedlineEvent::Edit(vec![EditCommand::Complete]),
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emacs_keybindings_keep_ctrl_r_history_search_and_tab_completion() {
        let mut keybindings = default_emacs_keybindings();
        add_menu_keybindings(&mut keybindings);
        assert_eq!(
            keybindings.find_binding(KeyModifiers::CONTROL, KeyCode::Char('r')),
            Some(ReedlineEvent::SearchHistory)
        );
        assert!(matches!(
            keybindings.find_binding(KeyModifiers::NONE, KeyCode::Tab),
            Some(ReedlineEvent::UntilFound(_))
        ));
    }

    #[test]
    fn vi_keybindings_keep_ctrl_r_history_search_and_tab_completion() {
        let mut insert = default_vi_insert_keybindings();
        let mut normal = default_vi_normal_keybindings();
        add_menu_keybindings(&mut insert);
        add_menu_keybindings(&mut normal);

        for keybindings in [insert, normal] {
            assert_eq!(
                keybindings.find_binding(KeyModifiers::CONTROL, KeyCode::Char('r')),
                Some(ReedlineEvent::SearchHistory)
            );
            assert!(matches!(
                keybindings.find_binding(KeyModifiers::NONE, KeyCode::Tab),
                Some(ReedlineEvent::UntilFound(_))
            ));
        }
    }
}
