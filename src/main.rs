use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, ExitCode, Stdio};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "lazyack",
    version,
    about = "Global hotkey to approve AI agent prompts via cmux"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Path to config (default: ~/.config/lazyack/config.json)"
    )]
    config: Option<PathBuf>,

    #[arg(
        short = 'd',
        long = "daemon",
        help = "Detach from terminal and log to ~/Library/Logs/lazyack.log. Stop with `pkill lazyack`."
    )]
    daemon: bool,

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
        None => {
            if cli.daemon {
                return match spawn_detached() {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("daemon spawn failed: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            lazyack::daemon::run(config)
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn spawn_detached() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;

    let log_path = log_path()?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut cmd = ProcCommand::new(&exe);
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "-d" && a != "--daemon")
        .collect();
    cmd.args(&args);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(log_file.try_clone()?));
    cmd.stderr(Stdio::from(log_file));

    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn()?;
    println!(
        "lazyack started in background (pid {}, log: {})",
        child.id(),
        log_path.display()
    );
    println!("Stop with: pkill lazyack");
    Ok(())
}

fn log_path() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| std::io::Error::other("HOME not set"))?;
    Ok(Path::new(&home)
        .join("Library")
        .join("Logs")
        .join("lazyack.log"))
}
