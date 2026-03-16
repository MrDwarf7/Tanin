mod app;
mod audio;
mod config;
mod init;
mod presets;
mod session;
mod static_data;
mod ui;
mod buffered;

use crate::app::{Action, App};
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

use std::panic;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable debug logging to tanin.log
    #[arg(short, long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    init::setup_loggging(args.debug)?;
    panic::set_hook(Box::new(init::handle_panic));
    let mut terminal = init::setup_terminal()?;

    let mut app = App::new()?;

    let res = run_app(&mut terminal, &mut app);

    init::restore_terminal(&mut terminal)?;

    if let Err(err) = res {
        eprintln!("{:?}", err);
    }

    app.save_session();

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(30);

    loop {
        let now = std::time::Instant::now();
        let dt = now.duration_since(last_tick);
        last_tick = now;

        // TODO: @mrdwarf7 : we can cleanup these 3 calls, terminal is awkward though
        //
        app.update(dt);
        let size = terminal.size()?;
        app.update_grid_cols(size.width, size.height);

        terminal.draw(|f| ui::ui(f, app))?;

        let timeout = tick_rate.saturating_sub(now.elapsed());

        if crossterm::event::poll(timeout)? {
            // Drain all pending event
            while crossterm::event::poll(Duration::from_millis(0))? {
                let event = event::read()?;
                if app.handle_event(event)? == Action::Quit {
                    return Ok(());
                }
            }
        }

        if app.quitting {
            return Ok(());
        }
    }
}
