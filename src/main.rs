mod app;
mod audio;
mod config;
mod init;
mod presets;
mod session;
mod static_data;
mod ui;
mod buffered;

use crate::app::{App, CurrentView};
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode};
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

        app.update(dt);

        let size = terminal.size()?;
        app.width = size.width;
        app.height = size.height;
        app.update_grid_cols();

        terminal.draw(|f| ui::ui(f, app))?;

        let timeout = tick_rate.saturating_sub(now.elapsed());
        if crossterm::event::poll(timeout)? {
            loop {
                match event::read()? {
                    Event::Key(key) => {
                        if app.view == CurrentView::Help {
                            if key.code == KeyCode::Char('q')
                                || (key.code == KeyCode::Char('c')
                                    && key
                                        .modifiers
                                        .contains(crossterm::event::KeyModifiers::CONTROL))
                            {
                                return Ok(());
                            }
                            app.view = CurrentView::Main;
                        } else if app.preset_input_mode {
                            match key.code {
                                KeyCode::Enter => {
                                    app.confirm_preset_input();
                                    app.preset_input_mode = false;
                                    app.preset_input_buffer.clear();
                                }
                                KeyCode::Esc => {
                                    app.preset_input_mode = false;
                                    app.preset_rename_target = None;
                                    app.preset_input_buffer.clear();
                                }
                                KeyCode::Backspace => {
                                    app.preset_input_buffer.pop();
                                }
                                KeyCode::Char(c) => {
                                    app.preset_input_buffer.push(c);
                                }
                                _ => {}
                            }
                        } else if app.search_mode {
                            match key.code {
                                KeyCode::Enter => {
                                    app.search_mode = false;
                                }
                                KeyCode::Esc => {
                                    app.search_mode = false;
                                    app.search_query.clear();
                                    app.scroll_into_view();
                                }
                                KeyCode::Backspace => {
                                    app.search_query.pop();
                                    app.validate_cursor_position();
                                }
                                KeyCode::Char(c) => {
                                    app.search_query.push(c);
                                    app.validate_cursor_position();
                                }
                                _ => {}
                            }
                        } else if app.view == CurrentView::Downloads {
                            if key.code == KeyCode::Tab {
                                app.view = CurrentView::Main;
                            } else {
                                handle_add_sound_keys(app, key);
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('q') => {
                                    app.quitting = true;
                                    return Ok(());
                                }
                                KeyCode::Esc => {
                                    if !app.search_query.is_empty() {
                                        app.search_query.clear();
                                        app.scroll_into_view();
                                    } else {
                                        app.quitting = true;
                                        return Ok(());
                                    }
                                }
                                KeyCode::Char('c')
                                    if key
                                        .modifiers
                                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                                {
                                    app.quitting = true;
                                    return Ok(());
                                }

                                KeyCode::Tab => {
                                    app.view = match app.view {
                                        CurrentView::Main => CurrentView::Presets,
                                        CurrentView::Presets => {
                                            if app.yt_dlp_available {
                                                CurrentView::Downloads
                                            } else {
                                                CurrentView::Main
                                            }
                                        }
                                        CurrentView::Downloads => CurrentView::Main,
                                        _ => CurrentView::Main,
                                    };
                                }

                                // Help
                                KeyCode::Char('?') => app.view = CurrentView::Help,

                                // Add Sound
                                KeyCode::Char('a') if app.view == CurrentView::Main => {
                                    if app.yt_dlp_available {
                                        app.view = CurrentView::Downloads;
                                        app.add_sound_name.clear();
                                        app.add_sound_category.clear();
                                        app.add_sound_url.clear();
                                        app.add_sound_status.clear();
                                        app.add_sound_focus_index = 0;
                                        app.add_sound_suggestion = None;
                                    }
                                }

                                // Master Mute
                                KeyCode::Char('m') => app.toggle_mute(),

                                _ => match app.view {
                                    CurrentView::Main => handle_main_keys(app, key.code),
                                    CurrentView::Presets => handle_presets_keys(app, key.code),
                                    CurrentView::Downloads => handle_add_sound_keys(app, key),
                                    CurrentView::AssetMissing => match key.code {
                                        KeyCode::Enter => app.start_asset_download(),
                                        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                                            app.view = CurrentView::Main
                                        }
                                        _ => {}
                                    },
                                    CurrentView::DownloadingAssets
                                        if app.asset_download_error.is_some()
                                            && key.code == KeyCode::Esc =>
                                    {
                                        app.view = CurrentView::Main;
                                        app.asset_download_error = None;
                                    }
                                    _ => {}
                                },
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        app.handle_mouse_event(mouse);
                    }
                    _ => {}
                }

                // Check if there are more events to process immediately
                if !crossterm::event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }

        if app.quitting {
            return Ok(());
        }
    }
}

fn handle_main_keys(app: &mut App, code: KeyCode) {
    match code {
        // Navigation
        KeyCode::Char('/') => {
            app.search_mode = true;
            app.search_query.clear();
        }
        KeyCode::Left | KeyCode::Char('h') => app.move_left(),
        KeyCode::Right | KeyCode::Char('l') => app.move_right(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),

        // Sound Control
        KeyCode::Enter | KeyCode::Char(' ') => app.toggle_current_sound(),
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let vol = app
                .sounds
                .get(app.cursor_pos)
                .map(|sound| sound.volume_linear);

            if let Some(v) = vol {
                app.set_current_volume(v + 0.1);
            }
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let vol = app
                .sounds
                .get(app.cursor_pos)
                .map(|sound| sound.volume_linear);

            if let Some(v) = vol {
                app.set_current_volume(v - 0.1);
            }
        }

        // Quick Volume
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let Some(d) = c.to_digit(10) {
                let vol = if d == 0 { 1.0 } else { d as f32 / 10.0 };
                app.set_current_volume(vol);
            }
        }

        // Master Volume
        KeyCode::Char('<') | KeyCode::Char(',') => {
            app.set_master_volume(app.session.global_volume - 0.1);
        }
        KeyCode::Char('>') | KeyCode::Char('.') => {
            app.set_master_volume(app.session.global_volume + 0.1);
        }

        // Stop All
        KeyCode::Char('s') => app.stop_all(),

        _ => {}
    }
}

fn handle_presets_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') if app.preset_cursor_pos > 0 => {
            app.preset_cursor_pos -= 1;
        }
        KeyCode::Up | KeyCode::Char('k') => {}
        KeyCode::Down | KeyCode::Char('j')
            if app.preset_cursor_pos < app.presets_config.presets.len().saturating_sub(1) =>
        {
            app.preset_cursor_pos += 1;
        }
        KeyCode::Down | KeyCode::Char('j') => {}
        KeyCode::Enter => {
            app.load_preset(app.preset_cursor_pos);
        }
        KeyCode::Char('n') => {
            app.preset_input_mode = true;
        }
        KeyCode::Char('r') => {
            app.start_renaming_preset();
        }
        KeyCode::Char('u') => {
            app.update_preset_sounds();
        }
        KeyCode::Char('d') => {
            app.delete_preset(app.preset_cursor_pos);
        }
        _ => {}
    }
}

// Function definition for update_suggestion
fn update_suggestion(app: &mut App) {
    if app.add_sound_category.is_empty() {
        app.add_sound_suggestion = None;
        return;
    }

    let input = app.add_sound_category.to_lowercase();
    let categories: Vec<String> = app.sounds.iter().map(|s| s.category.clone()).collect();

    // Find first category that starts with input and is longer
    if let Some(cat) = categories
        .iter()
        .find(|c| c.to_lowercase().starts_with(&input) && c.len() > input.len())
    {
        app.add_sound_suggestion = Some(cat.clone());
    } else {
        app.add_sound_suggestion = None;
    }
}

fn handle_add_sound_keys(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.view = CurrentView::Main;
            app.add_sound_name.clear();
            app.add_sound_category.clear();
            app.add_sound_url.clear();
            app.add_sound_status.clear();
            app.add_sound_suggestion = None;
        }
        KeyCode::Down => {
            app.add_sound_focus_index = (app.add_sound_focus_index + 1) % 4;
        }
        KeyCode::Up => {
            if app.add_sound_focus_index == 0 {
                app.add_sound_focus_index = 3;
            } else {
                app.add_sound_focus_index -= 1;
            }
        }
        KeyCode::Right if app.add_sound_focus_index == 1 => {
            if let Some(suggestion) = &app.add_sound_suggestion {
                app.add_sound_category = suggestion.clone();
                app.add_sound_suggestion = None;
            }
        }
        KeyCode::Right => {}
        KeyCode::Enter => {
            if app.add_sound_focus_index == 3 {
                app.start_download();
            } else {
                app.add_sound_focus_index += 1;
            }
        }
        KeyCode::Backspace => {
            let buffer = match app.add_sound_focus_index {
                0 => &mut app.add_sound_name,
                1 => &mut app.add_sound_category,
                2 => &mut app.add_sound_icon,
                3 => &mut app.add_sound_url,
                _ => return,
            };
            buffer.pop();

            if app.add_sound_focus_index == 1 {
                update_suggestion(app);
            }
        }
        KeyCode::Char(c) => {
            let buffer = match app.add_sound_focus_index {
                0 => &mut app.add_sound_name,
                1 => &mut app.add_sound_category,
                2 => &mut app.add_sound_icon,
                3 => &mut app.add_sound_url,
                _ => return,
            };
            buffer.push(c);

            if app.add_sound_focus_index == 1 {
                update_suggestion(app);
            }
        }
        _ => {}
    }
}
