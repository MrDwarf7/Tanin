pub mod audio;
pub mod download;
pub mod input;
pub mod navigation;
pub mod presets;

use crate::audio::AudioEngine;
use crate::config::Config;
use crate::presets::PresetsConfig;
use crate::session::{Session, SoundState};
use crate::static_data::{get_active_assets_path, get_bundled_sounds, Sound};
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
pub use download::{DownloadEvent, DownloadStatus, DownloadTask};
use std::sync::mpsc::Receiver;

#[derive(Debug, PartialEq)]
pub enum Action {
    Continue,
    Quit,
}

pub enum AssetDownloadEvent {
    ConfigDownloaded(Vec<Sound>),
    Error(String),
}

#[derive(PartialEq)]
pub enum CurrentView {
    Main,
    Presets,
    Help,
    Downloads,
    AssetMissing,
    DownloadingAssets,
}

pub struct App {
    pub sounds: Vec<Sound>,
    pub cursor_pos: usize,
    pub view: CurrentView,
    pub audio_engine: Option<AudioEngine>,
    pub config: Config,
    pub session: Session,
    pub presets_config: PresetsConfig,
    pub quitting: bool,
    pub grid_cols: u16, // is grid_cols basically `height` ?????
    pub width: u16,
    pub height: u16,
    pub muted: bool,
    pub previous_volume: f32,
    pub grid_scroll: u16,

    // Preset view state
    pub preset_cursor_pos: usize,
    pub preset_input_mode: bool,
    pub preset_input_buffer: String,
    pub preset_rename_target: Option<usize>,
    pub active_preset: Option<String>,
    pub animation_offset: f32,

    // Add Sound view state
    pub add_sound_name: String,
    pub add_sound_category: String,
    pub add_sound_icon: String,
    pub add_sound_url: String,
    pub add_sound_focus_index: usize, // 0: Name, 1: Category, 2: Icon, 3: URL
    pub add_sound_status: String,
    pub add_sound_suggestion: Option<String>,

    // Search state
    pub search_query: String,
    pub search_mode: bool,

    // Download Queue
    pub yt_dlp_available: bool,
    pub download_queue: Vec<DownloadTask>,
    pub active_download_index: Option<usize>,
    pub download_rx: Option<Receiver<DownloadEvent>>,

    // Asset Download
    pub asset_download_rx: Option<Receiver<AssetDownloadEvent>>,
    pub asset_download_error: Option<String>,
}

impl App {
    /// Create app
    pub fn new() -> Result<Self> {
        let session = Session::load()?;

        // Check yt-dlp availability
        let yt_dlp_available = std::process::Command::new("yt-dlp")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let mut app = Self {
            sounds: Vec::new(),
            cursor_pos: 0,
            view: CurrentView::Main,
            audio_engine: AudioEngine::try_new().ok(),
            config: Config::load()?,
            session: session.clone(),
            presets_config: PresetsConfig::load().unwrap_or_default(),
            quitting: false,
            grid_cols: 1,
            width: 80,
            height: 24,
            muted: false,
            previous_volume: session.global_volume,
            grid_scroll: 0,
            preset_cursor_pos: 0,
            preset_input_mode: false,
            preset_input_buffer: String::new(),
            preset_rename_target: None,
            active_preset: None,
            animation_offset: 0.0,
            add_sound_name: String::new(),
            add_sound_category: String::new(),
            add_sound_icon: "🎵".to_string(),
            add_sound_url: String::new(),
            add_sound_focus_index: 0,
            add_sound_status: String::new(),
            add_sound_suggestion: None,

            search_query: String::new(),
            search_mode: false,

            yt_dlp_available,
            download_queue: Vec::new(),
            active_download_index: None,
            download_rx: None,

            asset_download_rx: None,
            asset_download_error: None,
        };

        match get_active_assets_path() {
            Some(path) => app.sounds.extend(get_bundled_sounds(&path)),
            None => app.view = CurrentView::AssetMissing,
        }

        app.sounds.extend(crate::static_data::load_custom_sounds());

        // Sort all sounds to ensure categories are grouped correctly (merging bundled + custom)
        app.sort_sounds();

        app.check_and_download_missing_files();

        // Apply config
        if let Some(engine) = &mut app.audio_engine {
            engine.set_master_volume(session.global_volume);

            for sound in &mut app.sounds {
                if let Some(sc) = session.sounds.get(&sound.id) {
                    sound.volume_linear = sc.volume;
                    if sc.enabled {
                        if let Err(e) =
                            engine.play(&sound.id, &sound.file_path, sound.volume_linear)
                        {
                            log::error!("Failed to auto-play sound '{}': {}", sound.id, e);
                            sound.error_state = true;
                        }
                    }
                }
            }
        }

        Ok(app)
    }

    pub fn handle_event(&mut self, event: Event) -> Result<Action> {
        match event {
            Event::Key(key) => self.handle_key_event(key),
            Event::Mouse(mouse) => {
                self.handle_mouse_event(mouse);
                Ok(Action::Continue)
            }
            Event::Resize(width, height) => {
                self.update_grid_cols(width, height);
                Ok(Action::Continue)
            }
            _ => Ok(Action::Continue),
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Action> {
        // Helper closure to check for Ctrl + char combinations
        let key_with_ctrl = |event: KeyEvent, char: char| {
            event.code == KeyCode::Char(char)
                && key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
        };

        if self.view == CurrentView::Help {
            if key.code == KeyCode::Char('q') || key_with_ctrl(key, 'c') {
                return Ok(Action::Quit);
            }
            self.view = CurrentView::Main;
            return Ok(Action::Continue);
        }

        if self.preset_input_mode {
            match key.code {
                KeyCode::Enter => {
                    self.confirm_preset_input();
                    self.preset_input_mode = false;
                    self.preset_input_buffer.clear();
                }

                KeyCode::Esc => {
                    self.preset_input_mode = false;
                    self.preset_rename_target = None;
                    self.preset_input_buffer.clear();
                }
                KeyCode::Backspace => {
                    self.preset_input_buffer.pop();
                }
                KeyCode::Char(c) => self.preset_input_buffer.push(c),
                _ => {}
            }
            return Ok(Action::Continue);
        }

        if self.search_mode {
            match key.code {
                KeyCode::Enter => self.search_mode = false,
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_query.clear();
                    self.scroll_into_view();
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.validate_cursor_position();
                }

                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.validate_cursor_position();
                }
                _ => {}
            }
            return Ok(Action::Continue);
        }

        if self.view == CurrentView::Downloads {
            if key.code == KeyCode::Tab {
                self.view = CurrentView::Main;
            } else {
                self.handle_add_sound_keys(key);
                // handle_add_sound_keys(app, key);
            }

            return Ok(Action::Continue);
        }

        match key.code {
            KeyCode::Char('q') => {
                self.quitting = true;
                Ok(Action::Quit)
            }
            KeyCode::Esc => {
                if !self.search_query.is_empty() {
                    self.search_query.clear();
                    self.scroll_into_view();
                    Ok(Action::Continue)
                } else {
                    self.quitting = true;
                    Ok(Action::Quit)
                }
            }
            // KeyCode::Char('c') if key .modifiers .contains(crossterm::event::KeyModifiers::CONTROL) => {
            KeyCode::Char('c') if key_with_ctrl(key, 'c') => {
                self.quitting = true;
                Ok(Action::Quit)
            }

            KeyCode::Tab => {
                self.view = match self.view {
                    CurrentView::Main => CurrentView::Presets,
                    CurrentView::Presets => {
                        if self.yt_dlp_available {
                            CurrentView::Downloads
                        } else {
                            CurrentView::Main
                        }
                    }
                    CurrentView::Downloads => CurrentView::Main,
                    _ => CurrentView::Main,
                };
                Ok(Action::Continue)
            }

            // Help
            KeyCode::Char('?') => {
                self.view = CurrentView::Help;
                Ok(Action::Continue)
            }

            // Add Sound
            KeyCode::Char('a') if self.view == CurrentView::Main => {
                if self.yt_dlp_available {
                    self.view = CurrentView::Downloads;
                    self.add_sound_name.clear();
                    self.add_sound_category.clear();
                    self.add_sound_url.clear();
                    self.add_sound_status.clear();
                    self.add_sound_focus_index = 0;
                    self.add_sound_suggestion = None;
                }
                Ok(Action::Continue)
            }

            // Master Mute
            KeyCode::Char('m') => {
                self.toggle_mute();
                Ok(Action::Continue)
            }

            _ => {
                match self.view {
                    CurrentView::Main => self.handle_main_keys(key.code),
                    CurrentView::Presets => self.handle_presets_keys(key.code),
                    CurrentView::Downloads => self.handle_add_sound_keys(key),
                    CurrentView::AssetMissing => match key.code {
                        KeyCode::Enter => self.start_asset_download(),
                        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                            self.view = CurrentView::Main
                        }
                        _ => {}
                    },
                    CurrentView::DownloadingAssets
                        if self.asset_download_error.is_some() && key.code == KeyCode::Esc =>
                    {
                        self.view = CurrentView::Main;
                        self.asset_download_error = None;
                    }
                    _ => {}
                }
                Ok(Action::Continue)
            }
        }
    }

    fn handle_presets_keys(&mut self, code: KeyCode) {
        match code {
            // BUG: @mrdwarf7 [enum_over_index] :
            KeyCode::Up | KeyCode::Char('k') if self.preset_cursor_pos > 0 => {
                self.preset_cursor_pos -= 1;
            }
            KeyCode::Up | KeyCode::Char('k') => {}
            KeyCode::Down | KeyCode::Char('j')
                if self.preset_cursor_pos < self.presets_config.presets.len().saturating_sub(1) =>
            {
                self.preset_cursor_pos += 1;
            }
            KeyCode::Down | KeyCode::Char('j') => {}
            KeyCode::Enter => {
                self.load_preset(self.preset_cursor_pos);
            }
            KeyCode::Char('n') => {
                self.preset_input_mode = true;
            }
            KeyCode::Char('r') => {
                self.start_renaming_preset();
            }
            KeyCode::Char('u') => {
                self.update_preset_sounds();
            }
            KeyCode::Char('d') => {
                self.delete_preset(self.preset_cursor_pos);
            }
            _ => {}
        }
    }

    fn handle_main_keys(&mut self, code: KeyCode) {
        match code {
            // Navigation
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
            }
            KeyCode::Left | KeyCode::Char('h') => self.move_left(),
            KeyCode::Right | KeyCode::Char('l') => self.move_right(),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),

            // Sound Control
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_current_sound(),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let vol = self
                    .sounds
                    .get(self.cursor_pos)
                    .map(|sound| sound.volume_linear);

                if let Some(v) = vol {
                    self.set_current_volume(v + 0.1);
                }
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let vol = self
                    .sounds
                    .get(self.cursor_pos)
                    .map(|sound| sound.volume_linear);

                if let Some(v) = vol {
                    self.set_current_volume(v - 0.1);
                }
            }

            // Quick Volume
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(d) = c.to_digit(10) {
                    let vol = if d == 0 { 1.0 } else { d as f32 / 10.0 };
                    self.set_current_volume(vol);
                }
            }

            // Master Volume
            KeyCode::Char('<') | KeyCode::Char(',') => {
                self.set_master_volume(self.session.global_volume - 0.1);
            }
            KeyCode::Char('>') | KeyCode::Char('.') => {
                self.set_master_volume(self.session.global_volume + 0.1);
            }

            // Stop All
            KeyCode::Char('s') => self.stop_all(),

            _ => {}
        }
    }

    fn handle_add_sound_keys(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.view = CurrentView::Main;
                self.add_sound_name.clear();
                self.add_sound_category.clear();
                self.add_sound_url.clear();
                self.add_sound_status.clear();
                self.add_sound_suggestion = None;
            }
            KeyCode::Down => {
                self.add_sound_focus_index = (self.add_sound_focus_index + 1) % 4;
            }
            KeyCode::Up => {
                if self.add_sound_focus_index == 0 {
                    self.add_sound_focus_index = 3;
                } else {
                    self.add_sound_focus_index -= 1;
                }
            }
            KeyCode::Right if self.add_sound_focus_index == 1 => {
                if let Some(suggestion) = &self.add_sound_suggestion {
                    self.add_sound_category = suggestion.clone();
                    self.add_sound_suggestion = None;
                }
            }
            KeyCode::Right => {}
            KeyCode::Enter => {
                if self.add_sound_focus_index == 3 {
                    self.start_download();
                } else {
                    self.add_sound_focus_index += 1;
                }
            }
            // BUG: @mrdwarf7 [enum_over_index] : This kind of state 'indexing' is incredibly brittle
            // and fits the Rust use-case for pattern matching on enums much better.
            // I would strongly recommend refactoring this to be an enum representing the current
            // field in focus, and then match on that enum instead of using an index. This would
            // make the code much more readable and maintainable, and reduce the chances of bugs due
            // to incorrect indexing.
            //
            KeyCode::Backspace => {
                let buffer = match self.add_sound_focus_index {
                    0 => &mut self.add_sound_name,
                    1 => &mut self.add_sound_category,
                    2 => &mut self.add_sound_icon,
                    3 => &mut self.add_sound_url,
                    _ => return,
                };
                buffer.pop();

                if self.add_sound_focus_index == 1 {
                    self.update_suggestion();
                }
            }

            // BUG: @mrdwarf7 [enum_over_index] : This kind of state 'indexing' is incredibly brittle
            // and fits the Rust use-case for pattern matching on enums much better.
            // I would strongly recommend refactoring this to be an enum representing the current
            // field in focus, and then match on that enum instead of using an index. This would
            // make the code much more readable and maintainable, and reduce the chances of bugs due
            // to incorrect indexing.
            //
            KeyCode::Char(c) => {
                let buffer = match self.add_sound_focus_index {
                    0 => &mut self.add_sound_name,
                    1 => &mut self.add_sound_category,
                    2 => &mut self.add_sound_icon,
                    3 => &mut self.add_sound_url,
                    _ => return,
                };
                buffer.push(c);

                if self.add_sound_focus_index == 1 {
                    self.update_suggestion();
                }
            }
            _ => {}
        }
    }

    fn update_suggestion(&mut self) {
        if self.add_sound_category.is_empty() {
            self.add_sound_suggestion = None;
            return;
        }

        let input = self.add_sound_category.to_lowercase();
        let categories: Vec<String> = self.sounds.iter().map(|s| s.category.clone()).collect();

        // Find first category that starts with input and is longer
        if let Some(cat) = categories
            .iter()
            .find(|c| c.to_lowercase().starts_with(&input) && c.len() > input.len())
        {
            self.add_sound_suggestion = Some(cat.clone());
        } else {
            self.add_sound_suggestion = None;
        }
    }

    pub fn start_asset_download(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.asset_download_rx = Some(rx);
        self.view = CurrentView::DownloadingAssets;
        self.asset_download_error = None;

        std::thread::spawn(move || match crate::static_data::download_config() {
            Ok(sounds) => {
                let _ = tx.send(AssetDownloadEvent::ConfigDownloaded(sounds));
            }
            Err(e) => {
                let _ = tx.send(AssetDownloadEvent::Error(e.to_string()));
            }
        });
    }

    // TODO: @mdwarf7 [refactor] : This function is huge and does way too much.
    // This can be _drastically_ simplified using a sort of
    // 'asset download manager' with a queue + tx/rx implementation.
    // Just attach a channel end to the App struct, and when something happens, send
    // and event to the DL manager, and wait (or poll on it), then update UI
    // on if we were successful or not. This would also make it much easier to add a 'download
    // progress' UI, and would cleanly separate concerns of downloading assets from the main app
    // logic.
    //
    pub fn update(&mut self, dt: std::time::Duration) {
        if let Some(rx) = &self.asset_download_rx {
            match rx.try_recv() {
                Ok(AssetDownloadEvent::ConfigDownloaded(sounds)) => {
                    self.asset_download_rx = None;

                    // Populate queue
                    for sound in sounds {
                        if !std::path::Path::new(&sound.file_path).exists() {
                            if let Some(url) = &sound.url {
                                self.download_queue.push(DownloadTask {
                                    name: sound.name.clone(),
                                    category: sound.category.clone(),
                                    url: url.clone(),
                                    status: DownloadStatus::Pending,
                                    icon: sound.icon.clone(),
                                    target_filename: std::path::Path::new(&sound.file_path)
                                        .file_name()
                                        .map(|s| s.to_string_lossy().to_string()),
                                });
                            }
                        }
                    }

                    // Reload sounds to pick up the new config
                    if self.config.general.enable_bundled_sounds {
                        if let Some(path) = get_active_assets_path() {
                            self.sounds = get_bundled_sounds(path);
                        }
                    }
                    self.sounds.extend(crate::static_data::load_custom_sounds());
                    self.sort_sounds();

                    // Switch to Downloads view
                    self.view = CurrentView::Downloads;
                    // Force yt_dlp available check just in case, though we checked at start
                    // If it's false, the user will see an empty download list or we should warn them.
                    // But we assume they have it if they chose to download.

                    // break;
                }
                Ok(AssetDownloadEvent::Error(e)) => {
                    self.asset_download_rx = None;
                    self.asset_download_error = Some(e);
                    // break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                // break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.asset_download_rx = None;
                    self.asset_download_error = Some("Thread disconnected".to_string());
                    // break;
                }
            }
        }

        if let Some(engine) = &mut self.audio_engine {
            engine.update(dt);
        }
        self.animation_offset += dt.as_secs_f32() * 3.0;

        // Queue Management
        if self.active_download_index.is_none() {
            // Find next pending task
            if let Some(idx) = self
                .download_queue
                .iter()
                .position(|t| matches!(t.status, DownloadStatus::Pending))
            {
                self.spawn_download_task(idx);
            }
        }

        // Poll download events
        if let Some(rx) = &self.download_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => match event {
                        DownloadEvent::Progress(p) => {
                            if let Some(idx) = self.active_download_index {
                                if let Some(task) = self.download_queue.get_mut(idx) {
                                    task.status = DownloadStatus::Downloading(p);
                                }
                            }
                        }
                        DownloadEvent::Success(name, cat, path, icon, url) => {
                            if let Some(idx) = self.active_download_index {
                                if let Some(task) = self.download_queue.get_mut(idx) {
                                    task.status = DownloadStatus::Done;
                                }
                            }
                            self.active_download_index = None;
                            self.download_rx = None;

                            // Keep URL in config
                            if let Err(e) = crate::static_data::add_custom_sound(
                                &name,
                                &cat,
                                &path,
                                &icon,
                                Some(&url),
                            ) {
                                log::error!("Failed to save config after download: {}", e);
                            } else {
                                log::info!("Successfully added sound '{}' with URL", name);
                                let id = name.to_lowercase().replace(" ", "_");
                                let new_sound = crate::static_data::Sound {
                                    id: id.clone(),
                                    name,
                                    category: cat,
                                    file_path: path,
                                    volume_linear: 0.5,
                                    icon,
                                    url: Some(url.clone()),
                                    error_state: false,
                                };
                                // Check if sound already exists (update case)
                                if let Some(existing) = self.sounds.iter_mut().find(|s| s.id == id)
                                {
                                    existing.file_path = new_sound.file_path;
                                    existing.url = Some(url);
                                    existing.error_state = false;
                                } else {
                                    let mut s = new_sound;
                                    s.url = Some(url);
                                    self.sounds.push(s);
                                }

                                self.sort_sounds();
                            }
                            break;
                        }
                        DownloadEvent::Error(e) => {
                            if let Some(idx) = self.active_download_index {
                                if let Some(task) = self.download_queue.get_mut(idx) {
                                    task.status = DownloadStatus::Error(e.clone());
                                }
                            }
                            self.active_download_index = None;
                            self.download_rx = None;
                            log::error!("Download error: {}", e);
                            break;
                        }
                    },
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if let Some(idx) = self.active_download_index {
                            if let Some(task) = self.download_queue.get_mut(idx) {
                                task.status =
                                    DownloadStatus::Error("Thread disconnected".to_string());
                            }
                        }
                        self.active_download_index = None;
                        self.download_rx = None;
                        break;
                    }
                }
            }
        }
    }

    pub fn get_filtered_sounds(&self) -> Vec<(usize, &Sound)> {
        // BUG: @mrdwarf7 [refactor] : Why do we constantly pull
        // items _off_ of self/App? If we understand our data flow we shouldn't need to.
        //
        let hidden = &self.config.general.hidden_categories;
        if self.search_query.is_empty() {
            self.sounds
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    // Check hidden categories
                    if hidden.contains(&s.category) {
                        return false;
                    }
                    // Check hidden per-sound config
                    if let Some(sc) = self.config.sounds.get(&s.id) {
                        if sc.hidden {
                            return false;
                        }
                    }
                    true
                })
                .collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.sounds
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    // Check hidden categories first
                    if hidden.contains(&s.category) {
                        return false;
                    }
                    // Check hidden per-sound config
                    if let Some(sc) = self.config.sounds.get(&s.id) {
                        if sc.hidden {
                            return false;
                        }
                    }

                    s.name.to_lowercase().contains(&query)
                        || s.category.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    pub fn sort_sounds(&mut self) {
        let order = &self.config.general.category_order;
        self.sounds.sort_by(|a, b| {
            let pos_a = order.iter().position(|c| c == &a.category);
            let pos_b = order.iter().position(|c| c == &b.category);

            match (pos_a, pos_b) {
                (Some(ia), Some(ib)) => {
                    let cmp = ia.cmp(&ib);
                    if cmp == std::cmp::Ordering::Equal {
                        a.id.cmp(&b.id)
                    } else {
                        cmp
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => {
                    let cmp = a.category.cmp(&b.category);
                    if cmp == std::cmp::Ordering::Equal {
                        a.id.cmp(&b.id)
                    } else {
                        cmp
                    }
                }
            }
        });
    }

    /// Save config
    pub fn save_session(&mut self) {
        for sound in &self.sounds {
            let enabled = if let Some(engine) = &self.audio_engine {
                engine.is_playing(&sound.id)
            } else {
                false
            };

            self.session.sounds.insert(
                sound.id.clone(),
                SoundState {
                    enabled,
                    volume: sound.volume_linear,
                },
            );
        }
        let _ = self.session.save();
        let _ = self.presets_config.save();
    }
}
