use anyhow::{anyhow, Result};
use log::debug;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum CommandCategory {
    Simple,
    Interactive,
    Complex,
    Builtin,
}

#[derive(Debug, Deserialize)]
pub struct CommandClassification {
    #[serde(rename = "simple_commands")]
    pub simple: SimpleCommands,
    #[serde(rename = "interactive_commands")]
    pub interactive: InteractiveCommands,
    #[serde(rename = "complex_commands")]
    pub complex: ComplexCommands,
    #[serde(rename = "builtin_commands")]
    pub builtin: BuiltinCommands,
    #[serde(rename = "command_priority")]
    pub priority: CommandPriority,
}

#[derive(Debug, Deserialize)]
pub struct SimpleCommands {
    pub simple: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InteractiveCommands {
    pub interactive: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ComplexCommands {
    pub complex: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BuiltinCommands {
    pub builtin: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommandPriority {
    pub builtin: u32,
    pub simple: u32,
    pub complex: u32,
    pub interactive: u32,
}

impl CommandClassification {
    pub fn classify(&self, command: &str) -> Option<CommandCategory> {
        // Match routing priority so category checks stay consistent with execution.
        if self.builtin.builtin.contains(&command.to_string()) {
            return Some(CommandCategory::Builtin);
        }
        if self.simple.simple.contains(&command.to_string()) {
            return Some(CommandCategory::Simple);
        }
        if self.interactive.interactive.contains(&command.to_string()) {
            return Some(CommandCategory::Interactive);
        }
        if self.complex.complex.contains(&command.to_string()) {
            return Some(CommandCategory::Complex);
        }
        None
    }

    pub fn get_priority(&self, category: &CommandCategory) -> u32 {
        match category {
            CommandCategory::Builtin => self.priority.builtin,
            CommandCategory::Simple => self.priority.simple,
            CommandCategory::Complex => self.priority.complex,
            CommandCategory::Interactive => self.priority.interactive,
        }
    }

    pub fn is_winuxcmd_command(&self, command: &str) -> bool {
        self.simple.simple.contains(&command.to_string())
            || self.interactive.interactive.contains(&command.to_string())
            || self.complex.complex.contains(&command.to_string())
    }

    pub fn is_builtin_command(&self, command: &str) -> bool {
        self.builtin.builtin.contains(&command.to_string())
    }

    pub fn is_interactive(&self, command: &str) -> bool {
        self.interactive.interactive.contains(&command.to_string())
    }
}

pub fn load_classification() -> Result<CommandClassification> {
    // Get executable path and build config search paths relative to it
    let exe_path =
        std::env::current_exe().map_err(|e| anyhow!("Failed to get executable path: {}", e))?;

    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow!("Failed to get executable directory"))?;

    // Try multiple possible locations for the classification file
    let possible_paths = vec![
        PathBuf::from("commands_classification.toml"), // Current directory
        exe_dir.join("commands_classification.toml"),  // Same directory as executable
        exe_dir.join("../../commands_classification.toml"), // Development: from target/release back to project root
        exe_dir.join("../../../commands_classification.toml"), // Development: deeper nesting
    ];

    for config_path in &possible_paths {
        if let Ok(content) = std::fs::read_to_string(config_path) {
            let classification: CommandClassification = toml::from_str(&content).map_err(|e| {
                anyhow!(
                    "Failed to parse classification file {}: {}",
                    config_path.display(),
                    e
                )
            })?;
            return Ok(classification);
        }
    }

    Err(anyhow!(
        "Failed to read classification file from any location. Searched: {}",
        possible_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_classification() {
        let classification = load_classification().unwrap();

        // Test known commands
        assert_eq!(classification.classify("ls"), Some(CommandCategory::Simple));
        assert_eq!(
            classification.classify("grep"),
            Some(CommandCategory::Simple)
        );
        assert_eq!(
            classification.classify("less"),
            Some(CommandCategory::Interactive)
        );
        assert_eq!(
            classification.classify("top"),
            Some(CommandCategory::Interactive)
        );
        assert_eq!(
            classification.classify("sed"),
            Some(CommandCategory::Complex)
        );
        assert_eq!(
            classification.classify("xargs"),
            Some(CommandCategory::Complex)
        );
        assert_eq!(
            classification.classify("cd"),
            Some(CommandCategory::Builtin)
        );

        // Test unknown command
        assert_eq!(classification.classify("nonexistent"), None);
    }

    #[test]
    fn test_is_winuxcmd_command() {
        let classification = load_classification().unwrap();

        assert!(classification.is_winuxcmd_command("ls"));
        assert!(classification.is_winuxcmd_command("grep"));
        assert!(classification.is_winuxcmd_command("less"));
        assert!(classification.is_winuxcmd_command("sed"));

        assert!(!classification.is_winuxcmd_command("cd"));
        assert!(!classification.is_winuxcmd_command("exit"));
    }

    #[test]
    fn test_is_builtin_command() {
        let classification = load_classification().unwrap();

        assert!(classification.is_builtin_command("cd"));
        assert!(classification.is_builtin_command("exit"));
        assert!(classification.is_builtin_command("pwd"));

        assert!(!classification.is_builtin_command("ls"));
        assert!(!classification.is_builtin_command("grep"));
    }

    #[test]
    fn test_is_interactive() {
        let classification = load_classification().unwrap();

        assert!(classification.is_interactive("less"));
        assert!(classification.is_interactive("top"));

        assert!(!classification.is_interactive("ls"));
        assert!(!classification.is_interactive("grep"));
    }

    #[test]
    fn test_get_priority() {
        let classification = load_classification().unwrap();

        assert_eq!(classification.get_priority(&CommandCategory::Builtin), 1);
        assert_eq!(classification.get_priority(&CommandCategory::Simple), 2);
        assert_eq!(classification.get_priority(&CommandCategory::Complex), 3);
        assert_eq!(
            classification.get_priority(&CommandCategory::Interactive),
            4
        );
    }
}

/// Route decision for command execution
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// Native WinSH builtin command (highest priority)
    Builtin,
    /// Execute via WinuxCmd DLL
    WinuxCmdDLL(CommandCategory),
    /// Execute via PATH as external command
    ExternalCommand,
    /// Command not found
    NotFound,
}

/// Command router for determining how to execute commands
pub struct CommandRouter {
    classification: CommandClassification,
    ffi_available: bool,
    enable_dll: bool,
}

impl CommandRouter {
    /// Create a new command router
    pub fn new(classification: CommandClassification, enable_dll: bool) -> Self {
        let ffi_available = if enable_dll {
            crate::winuxcmd_ffi::WinuxCmdFFI::is_initialized()
        } else {
            false
        };
        Self {
            classification,
            ffi_available,
            enable_dll,
        }
    }

    /// Route a command to the appropriate executor
    ///
    /// Routing priority:
    /// 1. Builtin commands (highest)
    /// 2. WinuxCmd DLL commands (if FFI available AND enabled)
    /// 3. External commands from PATH (lowest)
    pub fn route_command(&self, command: &str) -> RouteDecision {
        // Check if command contains path separator - use external execution
        if command.contains('\\') || command.contains('/') {
            return RouteDecision::ExternalCommand;
        }

        // 1. Check builtin first (highest priority)
        if self.classification.is_builtin_command(command) {
            return RouteDecision::Builtin;
        }

        // 2. Check WinuxCmd DLL (if FFI available AND enabled)
        // But force certain commands to use PATH for better signal handling
        if self.enable_dll && self.ffi_available {
            if let Some(category) = self.classification.classify(command) {
                // Interactive commands need TTY support, use PATH execution
                if category == CommandCategory::Interactive {
                    debug!("Force PATH execution for interactive command: {}", command);
                    return RouteDecision::ExternalCommand;
                }

                // Commands with known issues in DLL implementation
                // Force them to use PATH for better compatibility
                let force_path_commands = vec![
                    "top", // top needs proper TTY handling
                ];
                if force_path_commands.iter().any(|&cmd| cmd == command) {
                    debug!("Force PATH execution for compatibility: {}", command);
                    return RouteDecision::ExternalCommand;
                }

                // Text processing commands that might wait for input
                // Force them to use PATH for proper Ctrl+C handling
                let input_waiting_commands = vec![
                    "grep", "sed", "awk", "perl", "python", "ruby", "less", "more", "vi", "vim",
                    "nano", "ed", "emacs", "ssh", "telnet", "ftp", "sftp", "nc", "netcat",
                ];
                if input_waiting_commands.iter().any(|&cmd| cmd == command) {
                    debug!(
                        "Force PATH execution for input-waiting command: {}",
                        command
                    );
                    return RouteDecision::ExternalCommand;
                }

                debug!("Route to WinuxCmd DLL: {}", command);
                return RouteDecision::WinuxCmdDLL(category);
            }
        }

        // 3. Fall back to external command execution
        // We don't return NotFound here because the command might exist in PATH
        RouteDecision::ExternalCommand
    }

    /// Update FFI availability status
    pub fn update_daemon_status(&mut self) {
        self.ffi_available = crate::winuxcmd_ffi::WinuxCmdFFI::is_initialized();
    }

    /// Check if FFI is available
    pub fn is_ffi_available(&self) -> bool {
        self.ffi_available
    }

    /// Get reference to command classification
    pub fn classification(&self) -> &CommandClassification {
        &self.classification
    }

    /// Set DLL enable/disable status
    pub fn set_enable_dll(&mut self, enable: bool) {
        self.enable_dll = enable;
        if enable {
            self.ffi_available = crate::winuxcmd_ffi::WinuxCmdFFI::is_initialized();
        } else {
            self.ffi_available = false;
        }
    }

    /// Check if DLL is enabled
    pub fn is_dll_enabled(&self) -> bool {
        self.enable_dll
    }
}

#[cfg(test)]
mod router_tests {
    use super::*;

    #[test]
    fn test_router_creation() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);
        // Just test creation works
        assert_eq!(router.classification().priority.builtin, 1);
    }

    #[test]
    fn test_route_builtin() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);

        assert_eq!(router.route_command("cd"), RouteDecision::Builtin);
        assert_eq!(router.route_command("pwd"), RouteDecision::Builtin);
        assert_eq!(router.route_command("echo"), RouteDecision::Builtin);
    }

    #[test]
    fn test_route_winuxcmd_simple() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);

        if router.is_ffi_available() {
            assert_eq!(
                router.route_command("ls"),
                RouteDecision::WinuxCmdDLL(CommandCategory::Simple)
            );
            assert_eq!(
                router.route_command("grep"),
                RouteDecision::WinuxCmdDLL(CommandCategory::Simple)
            );
        } else {
            // Fallback to external if daemon not available
            assert_eq!(router.route_command("ls"), RouteDecision::ExternalCommand);
        }
    }

    #[test]
    fn test_route_winuxcmd_interactive() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);

        if router.is_ffi_available() {
            assert_eq!(
                router.route_command("less"),
                RouteDecision::WinuxCmdDLL(CommandCategory::Interactive)
            );
            assert_eq!(
                router.route_command("top"),
                RouteDecision::WinuxCmdDLL(CommandCategory::Interactive)
            );
        }
    }

    #[test]
    fn test_route_winuxcmd_complex() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);

        if router.is_ffi_available() {
            assert_eq!(
                router.route_command("sed"),
                RouteDecision::WinuxCmdDLL(CommandCategory::Complex)
            );
            assert_eq!(
                router.route_command("xargs"),
                RouteDecision::WinuxCmdDLL(CommandCategory::Complex)
            );
        }
    }

    #[test]
    fn test_route_external() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);

        // Commands not in classification should route to external
        assert_eq!(
            router.route_command("notepad"),
            RouteDecision::ExternalCommand
        );
        assert_eq!(
            router.route_command("unknowncmd"),
            RouteDecision::ExternalCommand
        );
    }

    #[test]
    fn test_route_with_path() {
        let classification = load_classification().unwrap();
        let router = CommandRouter::new(classification, true);

        // Commands with path separators should use external execution
        assert_eq!(
            router.route_command("C:\\Git\\bin\\ls.exe"),
            RouteDecision::ExternalCommand
        );
        assert_eq!(
            router.route_command("./ls.exe"),
            RouteDecision::ExternalCommand
        );
    }
}
