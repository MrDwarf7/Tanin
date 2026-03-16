use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::LevelFilter;
use ratatui::{backend::CrosstermBackend, Terminal};
use simplelog::{Config, WriteLogger};
use std::{fs::File, os::fd::AsFd};
use std::{io, panic::PanicHookInfo};

const LOG_FILE: &str = "tanin.log";
const DEV_NULL_PATH: &str = "/dev/null";

/// Returns the function that is called when a panic occurs
/// Register panic hook to restore terminal and log panic
pub fn handle_panic(info: &PanicHookInfo) {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    let _ = crossterm::execute!(stdout, crossterm::cursor::Show);

    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown".to_string());

    if let Some(s) = info.payload_as_str() {
        let log_msg = format!("PANIC: '{}' at {}", s, location);
        log::error!("{}", log_msg);

        // Also print to stderr for immediate feedback
        eprintln!("{}", log_msg);
    }
}

pub fn setup_loggging(debug: bool) -> Result<()> {
    if debug {
        let log_file = File::create(LOG_FILE)?;
        nix::unistd::dup2_stderr(log_file.as_fd())
            .map_err(|e| anyhow::anyhow!("Failed to redirect stderr: {}", e))?;

        let log_file_clone = log_file.try_clone()?;

        let _ = WriteLogger::init(LevelFilter::Debug, Config::default(), log_file_clone);
        log::info!("Starting Tanin in debug mode");
    } else {
        if let Ok(dev_null) = File::open(DEV_NULL_PATH) {
            nix::unistd::dup2_stderr(dev_null.as_fd())
                .map_err(|e| anyhow::anyhow!("Failed to redirect stderr: {}", e))?;
        }
    }
    Ok(())
}

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
