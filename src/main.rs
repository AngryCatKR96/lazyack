use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "lazyack",
    version,
    about = "Global hotkey to approve AI agent prompts via cmux"
)]
struct Cli {
    #[arg(long, global = true, help = "Path to config (default: ~/.config/lazyack/config.json)")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Diagnose configuration, cmux connectivity, and hotkey conflicts")]
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match lazyack::config::Config::load(cli.config.as_ref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = match cli.command {
        Some(Command::Doctor) => lazyack::doctor::run(config),
        None => lazyack::daemon::run(config),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
