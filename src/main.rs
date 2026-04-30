// WinSH MVP6 - Array Support and Internationalization
//
// MVP6 Features:
// - Array support (definition, access, expansion)
// - Internationalization (English only)
// - Enhanced config file support (terminal styling)
// - Plugin system support
// - Modular architecture following Rust best practices

use anyhow::Result;

// Win32 API for Ctrl+C handling
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
static mut CURRENT_CHILD_PID: u32 = 0;

#[cfg(windows)]
static CTRL_C_RECEIVED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
use windows_sys::Win32::Foundation::BOOL;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_LOGOFF_EVENT,
    CTRL_SHUTDOWN_EVENT, PHANDLER_ROUTINE,
};

#[cfg(windows)]
unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT => {
            // Ctrl+C received
            CTRL_C_RECEIVED.store(true, Ordering::SeqCst);

            // If there's a child process running, try to terminate it
            if CURRENT_CHILD_PID != 0 {
                // Terminate the child process only
                use windows_sys::Win32::System::Threading::{
                    OpenProcess, TerminateProcess, PROCESS_TERMINATE,
                };
                let handle = OpenProcess(PROCESS_TERMINATE, 0, CURRENT_CHILD_PID);
                if !handle.is_null() {
                    TerminateProcess(handle, 1);
                }
                return 1; // Signal handled
            }

            // No child process, let the default handler run
            return 0;
        }
        _ => 0, // Let default handlers run for other signals
    }
}
#[cfg(windows)]
pub fn setup_ctrl_c_handler() {
    unsafe {
        if SetConsoleCtrlHandler(Some(ctrl_handler), 1) == 0 {
            eprintln!("Warning: Failed to set Ctrl+C handler");
        } else {
            log::debug!("Ctrl+C handler installed successfully");
        }
    }
}

#[cfg(windows)]
pub fn set_current_child_pid(pid: u32) {
    unsafe {
        CURRENT_CHILD_PID = pid;
    }
}

#[cfg(windows)]
pub fn clear_current_child_pid() {
    unsafe {
        CURRENT_CHILD_PID = 0;
    }
}

#[cfg(windows)]
pub fn is_ctrl_c_received() -> bool {
    CTRL_C_RECEIVED.swap(false, Ordering::SeqCst)
}

#[cfg(not(windows))]
pub fn setup_ctrl_c_handler() {}
#[cfg(not(windows))]
pub fn set_current_child_pid(_: u32) {}
#[cfg(not(windows))]
pub fn clear_current_child_pid() {}
#[cfg(not(windows))]
pub fn is_ctrl_c_received() -> bool {
    false
}

use colored::Colorize;
use reedline::Signal;
use std::env;
use std::path::PathBuf;

mod array;
mod builtins;
mod command_router;
mod completion;
mod config;
mod error;
mod executor;
mod job;
mod oh_my_winuxsh;
mod plugin;
mod shell;
mod theme;
mod tokenizer;
mod winuxcmd_ffi;

use shell::Shell;
use winuxcmd_ffi::WinuxCmdFFI;

fn print_usage() {
    println!("WinSH usage:");
    println!("  winuxsh -c \"command\"");
    println!("  winuxsh script.sh [args...]");
    println!("  winuxsh --help | -h");
    println!("  winuxsh --version");
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".red(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Initialize logging (default to error level only)
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Error)
        .parse_env("RUST_LOG")
        .init();

    // Setup Ctrl+C handler
    setup_ctrl_c_handler();

    // Initialize WinuxCmd FFI
    if let Err(e) = initialize_winuxcmd() {
        eprintln!("Warning: Failed to initialize WinuxCmd: {}", e);
    }

    // Parse command line arguments

    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "-c" => {
                if args.len() > 2 {
                    let mut shell = Shell::new(true)?;
                    if let Err(e) = shell.save_history(&args[2]) {
                        eprintln!(
                            "{} {}",
                            "Warning:".yellow(),
                            format!("Failed to save history: {}", e)
                        );
                    }
                    shell.execute_command(&args[2])?;
                } else {
                    eprintln!("{} {}", "Error:".red(), "-c requires an argument");
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                print_usage();
            }
            "--version" => {
                println!(
                    "{}",
                    "WinSH MVP6 - Array Support and Internationalization version 0.6.0".green()
                );
            }
            _ => {
                // Check if it's a script file
                let script_path = PathBuf::from(&args[1]);
                if script_path.exists() {
                    let mut shell = Shell::new(true)?;
                    shell.run_script_file(&script_path, &args[2..])?;
                } else {
                    eprintln!("{} {}", "Unknown argument:".red(), args[1]);
                    print_usage();
                    std::process::exit(1);
                }
            }
        }
        return Ok(());
    }

    let mut shell = Shell::new(true)?;
    shell.run_repl()?;

    Ok(())
}

// Add this to shell module temporarily
impl Shell {
    pub fn run_repl(&mut self) -> Result<()> {
        println!(
            "{}",
            "WinSH MVP6 - Array Support and Internationalization".green()
        );
        println!("Type 'help' for available commands");
        println!();

        loop {
            let prompt = self.get_prompt();

            match self.line_editor.read_line(&prompt) {
                Ok(Signal::Success(buffer)) => {
                    let line = buffer.trim();
                    if line.is_empty() {
                        continue;
                    }

                    if let Err(e) = self.save_history(line) {
                        eprintln!(
                            "{} {}",
                            "Warning:".yellow(),
                            format!("Failed to save history: {}", e)
                        );
                    }

                    // Execute command
                    if let Err(e) = self.execute_command(line) {
                        eprintln!("{} {}", "Error:".red(), e);
                    }

                    // Update completion state with current directory after command execution
                    self.update_completion_state();
                }
                Ok(Signal::CtrlD) => {
                    println!();
                    println!("Goodbye!");
                    break;
                }
                Ok(Signal::CtrlC) => {
                    println!();
                    continue;
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Initialize WinuxCmd daemon (FFI disabled, always succeeds)
fn initialize_winuxcmd() -> anyhow::Result<()> {
    Ok(())
}
