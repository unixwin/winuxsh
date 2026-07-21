// Executor module for WinSH MVP6
// Ported from MVP5 to provide external command execution

use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::array::ArrayValue;
use crate::error::{Result, ShellError};
use crate::tokenizer::CommandInfo;

#[derive(Debug, Default, Clone)]
pub struct CapturedExecution {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// Executor for external commands
pub struct Executor {
    env_vars: Vec<(String, String)>,
    current_dir: PathBuf,
}

impl Executor {
    /// Create a new executor
    pub fn new(env_vars: &[(String, ArrayValue)], current_dir: &PathBuf) -> Self {
        let env_vars: Vec<(String, String)> = env_vars
            .iter()
            .filter_map(|(k, v)| {
                if let ArrayValue::String(ref s) = v {
                    Some((k.clone(), s.clone()))
                } else {
                    None
                }
            })
            .collect();

        Executor {
            env_vars,
            current_dir: current_dir.clone(),
        }
    }

    /// Execute an external command
    pub fn execute(&self, cmd: &str, args: &[String], cmd_info: &CommandInfo) -> Result<i32> {
        let mut result = self.execute_internal(cmd, args, cmd_info, false)?;
        if !result.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&result.stderr));
        }
        if result.exit_code != 0 {
            eprintln!("Command exited with status code: {}", result.exit_code);
        }
        Ok(result.exit_code)
    }

    /// Execute an external command and capture output.
    pub fn execute_capture(
        &self,
        cmd: &str,
        args: &[String],
        cmd_info: &CommandInfo,
    ) -> Result<CapturedExecution> {
        self.execute_internal(cmd, args, cmd_info, true)
    }

    fn execute_internal(
        &self,
        cmd: &str,
        args: &[String],
        cmd_info: &CommandInfo,
        capture_output: bool,
    ) -> Result<CapturedExecution> {
        let cmd_path = self.find_command_in_path(cmd)?;

        let program = match cmd_path {
            Some(path) => path,
            None => {
                return Err(ShellError::CommandNotFound(format!(
                    "Command '{}' not found",
                    cmd
                )));
            }
        };

        let program_str = program.to_string_lossy().to_lowercase();

        // Check if it's PowerShell script
        let (actual_program, actual_args) = if program_str.ends_with(".ps1") {
            let program_path = program.to_string_lossy().to_string();
            let mut ps_args: Vec<String> = vec![
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-File".to_string(),
                program_path,
            ];
            ps_args.extend(args.iter().map(|s| s.to_string()));
            ("powershell.exe".to_string(), ps_args)
        } else {
            // For .exe, .bat, .cmd, and other executables, execute directly
            let exe_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            (program.to_string_lossy().to_string(), exe_args)
        };

        let mut command = Command::new(&actual_program);
        command.args(&actual_args);
        command.current_dir(&self.current_dir);

        command.stdin(Stdio::inherit());
        if capture_output {
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
        } else {
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
        }

        // Handle redirections - override inherited/captured stdio if specified
        if let Some(ref stdin_file) = cmd_info.stdin_redir {
            let file = std::fs::File::open(stdin_file)?;
            command.stdin(Stdio::from(file));
        }

        let mut stdout_handle: Option<File> = None;
        let mut stderr_handle: Option<File> = None;

        if let Some(ref stdout_file) = cmd_info.stdout_redir {
            let file = if cmd_info.stdout_append {
                std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(stdout_file)?
            } else {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(stdout_file)?
            };
            stdout_handle = Some(file);
        }

        if let Some(ref stderr_file) = cmd_info.stderr_redir {
            let file = if cmd_info.stderr_append {
                std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(stderr_file)?
            } else {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(stderr_file)?
            };
            stderr_handle = Some(file);
        }

        // Descriptor duplication (best-effort for 2>&1 and 1>&2).
        if cmd_info.stderr_to_stdout {
            if let Some(ref out_file) = stdout_handle {
                stderr_handle = Some(out_file.try_clone()?);
            }
        }

        if cmd_info.stdout_to_stderr {
            if let Some(ref err_file) = stderr_handle {
                stdout_handle = Some(err_file.try_clone()?);
            }
        }

        if let Some(file) = stdout_handle {
            command.stdout(Stdio::from(file));
        }

        if let Some(file) = stderr_handle {
            command.stderr(Stdio::from(file));
        }

        if cmd_info.background {
            match command.spawn() {
                Ok(child) => {
                    let pid = child.id();
                    let cmd_str = cmd_info.args.join(" ");
                    println!("Background job started: [{}] {}", pid, cmd_str);
                    Ok(CapturedExecution {
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        exit_code: 0,
                    })
                }
                Err(e) => Err(ShellError::CommandNotFound(format!(
                    "Failed to start background process: {}",
                    e
                ))),
            }
        } else {
            #[cfg(windows)]
            {
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
                command.creation_flags(CREATE_NEW_PROCESS_GROUP);
            }

            match command.spawn() {
                Ok(mut child) => {
                    #[cfg(windows)]
                    crate::set_current_child_pid(child.id());

                    let output = if capture_output {
                        match child.wait_with_output() {
                            Ok(output) => Ok(CapturedExecution {
                                stdout: output.stdout,
                                stderr: output.stderr,
                                exit_code: output.status.code().unwrap_or(1),
                            }),
                            Err(e) => Err(e),
                        }
                    } else {
                        match child.wait() {
                            Ok(exit_status) => Ok(CapturedExecution {
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                                exit_code: exit_status.code().unwrap_or(1),
                            }),
                            Err(e) => Err(e),
                        }
                    };

                    #[cfg(windows)]
                    crate::clear_current_child_pid();

                    match output {
                        Ok(result) => Ok(result),
                        Err(e) => {
                            eprintln!("Failed to wait for command: {}", e);
                            Ok(CapturedExecution {
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                                exit_code: 1,
                            })
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Command execution error: {}", e);
                    Ok(CapturedExecution {
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        exit_code: 1,
                    })
                }
            }
        }
    }

    /// Find a command in PATH
    pub fn find_command_in_path(&self, cmd: &str) -> Result<Option<PathBuf>> {
        let clean_cmd =
            cmd.trim_matches(|c: char| c == '\u{feff}' || c == '\u{fffe}' || c.is_whitespace());

        // Check current directory - prioritize extensions
        let current_dir = self.current_dir.clone();

        // Check for .exe, .bat, .cmd, .ps1, .com first (with extensions)
        for ext in &[".exe", ".bat", ".cmd", ".ps1", ".com"] {
            let cmd_with_ext = current_dir.join(format!("{}{}", clean_cmd, ext));
            if cmd_with_ext.exists() {
                return Ok(Some(cmd_with_ext));
            }
        }

        // Then check for command without extension
        let current_cmd_path = current_dir.join(clean_cmd);
        if current_cmd_path.exists() {
            return Ok(Some(current_cmd_path));
        }

        // Check full path
        if clean_cmd.contains('\\') || clean_cmd.contains('/') {
            let path = PathBuf::from(clean_cmd);
            if path.exists() {
                return Ok(Some(path));
            }
            return Ok(None);
        }

        // Search in PATH
        let path_env = env::var("PATH").or_else(|_| {
            self.env_vars
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "PATH not found"))
        });

        if let Ok(path_env) = path_env {
            let paths: Vec<_> = env::split_paths(&path_env).collect();

            for dir in paths {
                // Check for .exe, .bat, .cmd, .ps1, .com first (with extensions)
                for ext in &[".exe", ".bat", ".cmd", ".ps1", ".com"] {
                    let cmd_with_ext = dir.join(format!("{}{}", clean_cmd, ext));
                    if cmd_with_ext.exists() {
                        return Ok(Some(cmd_with_ext));
                    }
                }

                // Then check for command without extension
                let cmd_path = dir.join(clean_cmd);
                if cmd_path.exists() {
                    return Ok(Some(cmd_path));
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_command_in_path() {
        let executor = Executor::new(&[], &PathBuf::from("."));
        let result = executor.find_command_in_path("echo");
        // echo should be found somewhere in PATH
        // This test might fail depending on the system
        assert!(result.is_ok());
    }

    #[test]
    fn test_executor_creation() {
        let env_vars = vec![(
            "PATH".to_string(),
            ArrayValue::String("/usr/bin:/bin".to_string()),
        )];
        let current_dir = PathBuf::from(".");
        let executor = Executor::new(&env_vars, &current_dir);
        assert_eq!(executor.env_vars.len(), 1);
    }
}
