use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, ExitCode, Stdio};

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "lazyack",
    version,
    about = "Global hotkey to approve AI agent prompts via cmux",
    arg_required_else_help = true
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Path to config (default: ~/.config/lazyack/config.json)"
    )]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Run the daemon (foreground by default; -d to background)")]
    Run {
        #[arg(
            short = 'd',
            long = "daemon",
            help = "Detach from terminal; log to ~/Library/Logs/lazyack.log"
        )]
        daemon: bool,
    },

    #[command(about = "Print whether a lazyack daemon is currently running")]
    Status,

    #[command(about = "Stop a running lazyack daemon (SIGTERM)")]
    Stop,

    #[command(about = "Diagnose configuration, cmux connectivity, and hotkey conflicts")]
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result: Result<(), String> = match cli.command {
        Some(Command::Run { daemon }) => {
            let config = match load_config(&cli) {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            if daemon {
                match spawn_detached() {
                    Ok(()) => return ExitCode::SUCCESS,
                    Err(e) => return fail(format!("daemon spawn failed: {e}")),
                }
            }
            if let Err(e) = write_pid_file(std::process::id()) {
                eprintln!("warn: could not write pid file: {e}");
            }
            let r = lazyack::daemon::run(config);
            let _ = remove_pid_file();
            r
        }
        Some(Command::Status) => {
            return match status() {
                Ok(Some(pid)) => {
                    println!("running (pid {pid})");
                    ExitCode::SUCCESS
                }
                Ok(None) => {
                    println!("not running");
                    ExitCode::from(3)
                }
                Err(e) => fail(format!("status check failed: {e}")),
            };
        }
        Some(Command::Stop) => {
            return match stop() {
                Ok(pid) => {
                    println!("sent SIGTERM to pid {pid}");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            };
        }
        Some(Command::Doctor) => {
            let config = match load_config(&cli) {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            lazyack::doctor::run(config)
        }
        None => {
            let _ = Cli::command().print_help();
            return ExitCode::SUCCESS;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

fn load_config(cli: &Cli) -> Result<lazyack::config::Config, String> {
    lazyack::config::Config::load(cli.config.as_ref())
}

fn fail(msg: String) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
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
    let _ = write_pid_file(child.id());
    println!(
        "lazyack started in background (pid {}, log: {})",
        child.id(),
        log_path.display()
    );
    println!("Stop with: lazyack stop");
    Ok(())
}

fn status() -> std::io::Result<Option<u32>> {
    let path = pid_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let pid: i32 = std::fs::read_to_string(&path)?
        .trim()
        .parse()
        .map_err(|e| std::io::Error::other(format!("invalid pid: {e}")))?;
    if process_alive(pid) {
        Ok(Some(pid as u32))
    } else {
        let _ = std::fs::remove_file(&path);
        Ok(None)
    }
}

fn stop() -> Result<i32, String> {
    let pid = match status().map_err(|e| e.to_string())? {
        Some(p) => p as i32,
        None => return Err("not running".into()),
    };
    let r = unsafe { libc::kill(pid, libc::SIGTERM) };
    if r == 0 {
        let _ = remove_pid_file();
        Ok(pid)
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

fn process_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

fn pid_file_path() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| std::io::Error::other("HOME not set"))?;
    let dir = Path::new(&home).join("Library/Caches/lazyack");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("lazyack.pid"))
}

fn write_pid_file(pid: u32) -> std::io::Result<()> {
    let path = pid_file_path()?;
    std::fs::write(path, format!("{pid}\n"))
}

fn remove_pid_file() -> std::io::Result<()> {
    let path = pid_file_path()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
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
