//! winuxsh entry point
//!
//! Usage:
//!   winuxsh                  → interactive REPL
//!   winuxsh -c "command"     → execute one command, print exit code, exit
//!   winuxsh script.sh        → execute a script file
//!   winuxsh --help | -h      → usage
//!   winuxsh --version        → version (winuxsh / rubash / winuxcmd)
//!   winuxsh --zsh-compat-report      → scan zsh config and print report
//!   winuxsh --zsh-compat-report-json → scan zsh config and print JSON report
//!   winuxsh --zsh-compat-import-plan → print a reviewable .winshrc.toml patch

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Initialize logging (only error level by default)
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Error)
        .parse_env("RUST_LOG")
        .init();

    // Install Ctrl+C handler (best-effort)
    winuxsh_runtime::ctrl_c::install();

    let args: Vec<String> = std::env::args().collect();

    if let Err(e) = run(&args) {
        eprintln!("winuxsh: {}", e);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run(args: &[String]) -> anyhow::Result<()> {
    if args.len() < 2 {
        return run_repl();
    }

    let first = &args[1];
    match first.as_str() {
        "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        "--version" | "-V" => {
            print_version();
            Ok(())
        }
        "--zsh-compat-report" => {
            print_zsh_compat_report(false)?;
            Ok(())
        }
        "--zsh-compat-report-json" => {
            print_zsh_compat_report(true)?;
            Ok(())
        }
        "--zsh-compat-import-plan" => {
            print_zsh_compat_import_plan()?;
            Ok(())
        }
        "-c" => {
            if args.len() < 3 {
                anyhow::bail!("-c requires an argument");
            }
            let mut shell = winuxsh_runtime::Shell::new()?;
            let code = shell.execute_script(&args[2])?;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        _ => {
            // Treat as a script file to execute
            let script = PathBuf::from(first);
            if !script.exists() {
                anyhow::bail!("unknown argument '{}' (not a script file)", first);
            }
            let mut shell = winuxsh_runtime::Shell::new()?;
            let content = std::fs::read_to_string(&script)?;
            shell.execute_script(&content)?;
            Ok(())
        }
    }
}

fn run_repl() -> anyhow::Result<()> {
    let mut shell = winuxsh_runtime::Shell::new()?;
    winuxsh_runtime::repl::run_repl(&mut shell)
}

fn print_usage() {
    println!("winuxsh - a bash-compatible shell for Windows");
    println!();
    println!("Usage:");
    println!("  winuxsh                    Start the interactive REPL");
    println!("  winuxsh -c \"command\"        Execute a command and exit");
    println!("  winuxsh script.sh [args...]  Execute a script file");
    println!("  winuxsh --help, -h          Show this help");
    println!("  winuxsh --version, -V       Show version info");
    println!("  winuxsh --zsh-compat-report      Scan zsh config and show a safe import report");
    println!("  winuxsh --zsh-compat-report-json Scan zsh config and show a JSON import report");
    println!("  winuxsh --zsh-compat-import-plan Print a reviewable .winshrc.toml import patch");
}

fn print_zsh_compat_import_plan() -> anyhow::Result<()> {
    let config = winuxsh_runtime::config::load();
    let options = winuxsh_runtime::zsh_compat::ZshImportOptions::for_report(&config.zsh);
    let report = winuxsh_runtime::zsh_compat::scan(&options);
    println!(
        "{}",
        winuxsh_runtime::zsh_compat::import_plan_toml(&options, &report)
    );
    Ok(())
}

fn print_zsh_compat_report(json: bool) -> anyhow::Result<()> {
    let config = winuxsh_runtime::config::load();
    let options = winuxsh_runtime::zsh_compat::ZshImportOptions::for_report(&config.zsh);
    let report = winuxsh_runtime::zsh_compat::scan(&options);
    if json {
        println!("{}", report.to_json_pretty()?);
    } else {
        println!("{}", report.to_human());
    }
    Ok(())
}

fn print_version() {
    println!("winuxsh 0.6.0");
    println!("  rubash:   {}", rubash_version());
    println!(
        "  winuxcmd: {}",
        winuxsh_runtime::winuxcmd::version().unwrap_or_else(|| "not found".to_string())
    );
}

fn rubash_version() -> &'static str {
    // When rubash exposes a version constant, switch to that.
    env!("CARGO_PKG_VERSION")
}
