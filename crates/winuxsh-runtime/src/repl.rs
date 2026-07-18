//! Reedline REPL loop

use reedline::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    EditCommand, EditMode, Emacs, FileBackedHistory, KeyCode, KeyModifiers, Keybindings, ListMenu,
    MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, Vi,
};

use crate::autosuggest::HistoryAutosuggestHinter;
use crate::completion::WinuxshCompleter;
use crate::config::{EditorMode, NativeWidgetConfig};
use crate::shell::Shell;
use crate::syntax_highlighting::WinuxshSyntaxHighlighter;
use crate::zsh_compat::NativeWidgetSuggestion;

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
        .with_edit_mode(build_edit_mode(
            shell.editor_mode,
            &shell.native_widgets,
            &shell.native_widget_bindings,
        ));

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

fn build_edit_mode(
    mode: EditorMode,
    native_widgets: &NativeWidgetConfig,
    native_widget_bindings: &[NativeWidgetSuggestion],
) -> Box<dyn EditMode> {
    match mode {
        EditorMode::Emacs => {
            let mut keybindings = default_emacs_keybindings();
            add_menu_keybindings(&mut keybindings);
            add_native_widget_keybindings(
                &mut keybindings,
                NativeKeymapTarget::Emacs,
                native_widgets,
                native_widget_bindings,
            );
            Box::new(Emacs::new(keybindings))
        }
        EditorMode::Vi => {
            let mut insert_keybindings = default_vi_insert_keybindings();
            let mut normal_keybindings = default_vi_normal_keybindings();
            add_menu_keybindings(&mut insert_keybindings);
            add_menu_keybindings(&mut normal_keybindings);
            add_native_widget_keybindings(
                &mut insert_keybindings,
                NativeKeymapTarget::ViInsert,
                native_widgets,
                native_widget_bindings,
            );
            add_native_widget_keybindings(
                &mut normal_keybindings,
                NativeKeymapTarget::ViNormal,
                native_widgets,
                native_widget_bindings,
            );
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeKeymapTarget {
    Emacs,
    ViInsert,
    ViNormal,
}

fn add_native_widget_keybindings(
    keybindings: &mut Keybindings,
    target: NativeKeymapTarget,
    config: &NativeWidgetConfig,
    bindings: &[NativeWidgetSuggestion],
) {
    if !config.enabled {
        return;
    }

    add_native_widget_preset_keybindings(keybindings, &config.presets);

    if !config.import_bindkeys {
        return;
    }

    for binding in bindings {
        if !native_widget_keymap_applies(binding.keymap.as_deref(), target) {
            continue;
        }
        let Some(key) = binding.key.as_deref().and_then(parse_zsh_key_sequence) else {
            continue;
        };
        let Some(event) = native_widget_event(&binding.widget) else {
            continue;
        };
        keybindings.add_binding(key.0, key.1, event);
    }
}

fn add_native_widget_preset_keybindings(keybindings: &mut Keybindings, presets: &[String]) {
    if presets
        .iter()
        .any(|preset| preset.eq_ignore_ascii_case("autosuggestions"))
    {
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char(' '),
            ReedlineEvent::HistoryHintComplete,
        );
    }
}

fn native_widget_keymap_applies(keymap: Option<&str>, target: NativeKeymapTarget) -> bool {
    let Some(keymap) = keymap else {
        return true;
    };
    match (keymap, target) {
        ("main", _) => true,
        ("emacs", NativeKeymapTarget::Emacs) => true,
        ("viins", NativeKeymapTarget::ViInsert) => true,
        ("vicmd", NativeKeymapTarget::ViNormal) => true,
        _ => false,
    }
}

fn native_widget_event(widget: &str) -> Option<ReedlineEvent> {
    match widget {
        "autosuggest-accept" => Some(ReedlineEvent::HistoryHintComplete),
        "autosuggest-execute" => Some(ReedlineEvent::Multiple(vec![
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::Enter,
        ])),
        "autosuggest-partial-accept" => Some(ReedlineEvent::HistoryHintWordComplete),
        "history-substring-search-up" => Some(ReedlineEvent::Up),
        "history-substring-search-down" => Some(ReedlineEvent::Down),
        _ => None,
    }
}

fn parse_zsh_key_sequence(value: &str) -> Option<(KeyModifiers, KeyCode)> {
    match value {
        "^[[A" | "\\e[A" | "\\eOA" => Some((KeyModifiers::NONE, KeyCode::Up)),
        "^[[B" | "\\e[B" | "\\eOB" => Some((KeyModifiers::NONE, KeyCode::Down)),
        "^[[C" | "\\e[C" | "\\eOC" => Some((KeyModifiers::NONE, KeyCode::Right)),
        "^[[D" | "\\e[D" | "\\eOD" => Some((KeyModifiers::NONE, KeyCode::Left)),
        "^?" => Some((KeyModifiers::NONE, KeyCode::Backspace)),
        _ => parse_control_key_sequence(value).or_else(|| parse_plain_key_sequence(value)),
    }
}

fn parse_control_key_sequence(value: &str) -> Option<(KeyModifiers, KeyCode)> {
    let rest = value.strip_prefix('^')?;
    let mut chars = rest.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let code = match ch {
        ' ' => KeyCode::Char(' '),
        '[' => KeyCode::Esc,
        ch if ch.is_ascii_alphabetic() => KeyCode::Char(ch.to_ascii_lowercase()),
        ch => KeyCode::Char(ch),
    };
    Some((KeyModifiers::CONTROL, code))
}

fn parse_plain_key_sequence(value: &str) -> Option<(KeyModifiers, KeyCode)> {
    let mut chars = value.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some((KeyModifiers::NONE, KeyCode::Char(ch)))
}

/// Run the interactive REPL.
pub fn run_repl(shell: &mut Shell) -> anyhow::Result<()> {
    println!("Winuxsh v2 - rubash + winuxcmd on Windows");
    println!("Type 'exit' to quit. Press Ctrl+D for EOF.");
    println!();

    shell.restore_last_working_dir_for_repl();
    let mut line_editor = build_line_editor(shell)?;

    loop {
        shell.run_precmd_hooks();
        match line_editor.read_line(&shell.prompt) {
            Ok(Signal::Success(buffer)) => {
                let line = buffer.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "exit" || line == "logout" {
                    break;
                }
                let _ = shell.execute_interactive_line(line);
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

    #[test]
    fn native_widget_preset_adds_autosuggest_accept_binding() {
        let mut keybindings = default_emacs_keybindings();
        let config = NativeWidgetConfig {
            enabled: true,
            presets: vec!["autosuggestions".to_string()],
            import_bindkeys: false,
        };

        add_native_widget_keybindings(
            &mut keybindings,
            NativeKeymapTarget::Emacs,
            &config,
            &[],
        );

        assert_eq!(
            keybindings.find_binding(KeyModifiers::CONTROL, KeyCode::Char(' ')),
            Some(ReedlineEvent::HistoryHintComplete)
        );
    }

    #[test]
    fn native_widget_imports_recognized_bindkey_widgets() {
        let mut keybindings = default_emacs_keybindings();
        let config = NativeWidgetConfig {
            enabled: true,
            presets: Vec::new(),
            import_bindkeys: true,
        };
        let bindings = vec![native_widget_binding("^ ", None, "autosuggest-accept")];

        add_native_widget_keybindings(
            &mut keybindings,
            NativeKeymapTarget::Emacs,
            &config,
            &bindings,
        );

        assert_eq!(
            keybindings.find_binding(KeyModifiers::CONTROL, KeyCode::Char(' ')),
            Some(ReedlineEvent::HistoryHintComplete)
        );
    }

    #[test]
    fn native_widget_bindkeys_respect_vi_keymaps() {
        let mut insert = default_vi_insert_keybindings();
        let mut normal = default_vi_normal_keybindings();
        let config = NativeWidgetConfig {
            enabled: true,
            presets: Vec::new(),
            import_bindkeys: true,
        };
        let bindings = vec![native_widget_binding("^F", Some("viins"), "autosuggest-accept")];

        add_native_widget_keybindings(
            &mut insert,
            NativeKeymapTarget::ViInsert,
            &config,
            &bindings,
        );
        add_native_widget_keybindings(
            &mut normal,
            NativeKeymapTarget::ViNormal,
            &config,
            &bindings,
        );

        assert_eq!(
            insert.find_binding(KeyModifiers::CONTROL, KeyCode::Char('f')),
            Some(ReedlineEvent::HistoryHintComplete)
        );
        assert_ne!(
            normal.find_binding(KeyModifiers::CONTROL, KeyCode::Char('f')),
            Some(ReedlineEvent::HistoryHintComplete)
        );
    }

    #[test]
    fn native_widget_maps_history_substring_arrows_to_history_navigation() {
        let mut keybindings = default_emacs_keybindings();
        let config = NativeWidgetConfig {
            enabled: true,
            presets: Vec::new(),
            import_bindkeys: true,
        };
        let bindings = vec![
            native_widget_binding("^[[A", None, "history-substring-search-up"),
            native_widget_binding("^[[B", None, "history-substring-search-down"),
        ];

        add_native_widget_keybindings(
            &mut keybindings,
            NativeKeymapTarget::Emacs,
            &config,
            &bindings,
        );

        assert_eq!(
            keybindings.find_binding(KeyModifiers::NONE, KeyCode::Up),
            Some(ReedlineEvent::Up)
        );
        assert_eq!(
            keybindings.find_binding(KeyModifiers::NONE, KeyCode::Down),
            Some(ReedlineEvent::Down)
        );
    }

    fn native_widget_binding(
        key: &str,
        keymap: Option<&str>,
        widget: &str,
    ) -> NativeWidgetSuggestion {
        NativeWidgetSuggestion {
            widget: widget.to_string(),
            function: None,
            key: Some(key.to_string()),
            keymap: keymap.map(str::to_string),
            source_file: None,
            line: None,
            origin: "test".to_string(),
        }
    }
}
