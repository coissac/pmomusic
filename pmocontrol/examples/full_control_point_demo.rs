//! Full interactive control point demo with Ratatui-powered UI.
//!
//! This version replaces the legacy println!-driven interface with a
//! Crossterm + Ratatui dashboard featuring menus, overlays and live updates.

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::process;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use pmocontrol::model::TrackMetadata;
use pmocontrol::{
    ControlPoint, DeviceRegistryRead, MediaBrowser, MediaEntry, MediaResource, MediaServerEvent,
    MediaServerInfo, MusicServer, PlaybackItem, PlaybackPosition, PlaybackStatus,
    PlaybackPositionInfo, RendererEvent, RendererInfo, TransportControl, VolumeControl,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_DISCOVERY_SECS: u64 = 15;
const TICK_RATE: Duration = Duration::from_millis(200);

#[derive(Clone)]
struct UiState {
    renderer_name: String,
    server_name: Option<String>,
    playback_state: Option<pmocontrol::capabilities::PlaybackState>,
    position: Option<PlaybackPositionInfo>,
    volume: Option<u16>,
    mute: Option<bool>,
    metadata: Option<TrackMetadata>,
    last_status: Option<String>,
    current_track_uri: Option<String>,
}

impl UiState {
    fn new(renderer_name: String) -> Self {
        Self {
            renderer_name,
            server_name: None,
            playback_state: None,
            position: None,
            volume: None,
            mute: None,
            metadata: None,
            last_status: Some("Interface initialisée.".to_string()),
            current_track_uri: None,
        }
    }

    fn placeholder() -> Self {
        Self::new("<renderer non sélectionné>".to_string())
    }

    fn set_status<S: Into<String>>(&mut self, status: S) {
        self.last_status = Some(status.into());
    }
}

fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    println!("=== Full Control Point Ratatui Demo ===");
    println!(
        "Starting control point with timeout={}s",
        DEFAULT_TIMEOUT_SECS
    );

    let control_point =
        ControlPoint::spawn(DEFAULT_TIMEOUT_SECS).context("Failed to start control point")?;

    println!(
        "Discovery running for {} seconds...",
        DEFAULT_DISCOVERY_SECS
    );
    std::thread::sleep(Duration::from_secs(DEFAULT_DISCOVERY_SECS));

    let registry = control_point.registry();
    let renderers = {
        let reg = registry.read().expect("registry poisoned");
        let list = reg.list_renderers();
        if list.is_empty() {
            eprintln!("No renderers discovered. Exiting.");
            process::exit(1);
        }
        list
    };

    let servers = {
        let reg = registry.read().expect("registry poisoned");
        let list: Vec<MediaServerInfo> = reg
            .list_servers()
            .into_iter()
            .filter(|s| s.has_content_directory && s.content_directory_control_url.is_some())
            .collect();
        if list.is_empty() {
            eprintln!("No media servers with ContentDirectory discovered. Exiting.");
            process::exit(1);
        }
        list
    };

    let control_point = Arc::new(control_point);
    let app = App::new(control_point, renderers, servers);
    if let Err(err) = run_app(app) {
        eprintln!("Application exited with error: {err}");
    }

    println!("\nExiting. Goodbye!");
    Ok(())
}

struct App {
    control_point: Arc<ControlPoint>,
    renderers: Vec<RendererInfo>,
    renderer_index: usize,
    renderer_info: Option<RendererInfo>,
    servers: Vec<MediaServerInfo>,
    server_index: usize,
    server_info: Option<MediaServerInfo>,
    music_server: Option<MusicServer>,
    browser: Option<BrowserState>,
    mode: Mode,
    ui_state: UiState,
    queue_snapshot: Vec<PlaybackItem>,
    queue_current_index: Option<usize>,
    show_queue_overlay: bool,
    show_help_overlay: bool,
    pending_binding_container: Option<String>,
    status_line: String,
    known_tracks: std::collections::HashMap<String, TrackMetadata>,
}

enum Mode {
    SelectRenderer,
    SelectServer,
    Browse,
    BindingPrompt,
    Control,
}

struct BrowserState {
    nav_state: NavigationState,
    entries: Vec<MediaEntry>,
    selected_index: usize,
}

enum AppEvent {
    Renderer(RendererEvent),
    Media(MediaServerEvent),
}

impl App {
    fn new(
        control_point: Arc<ControlPoint>,
        renderers: Vec<RendererInfo>,
        servers: Vec<MediaServerInfo>,
    ) -> Self {
        Self {
            control_point,
            renderers,
            renderer_index: 0,
            renderer_info: None,
            servers,
            server_index: 0,
            server_info: None,
            music_server: None,
            browser: None,
            mode: Mode::SelectRenderer,
            ui_state: UiState::placeholder(),
            queue_snapshot: Vec::new(),
            queue_current_index: None,
            show_queue_overlay: false,
            show_help_overlay: false,
            pending_binding_container: None,
            status_line: "Sélectionne un renderer avec ↑/↓ et Entrée".to_string(),
            known_tracks: HashMap::new(),
        }
    }

    fn renderer_id(&self) -> Option<pmocontrol::model::RendererId> {
        self.renderer_info.as_ref().map(|info| info.id.clone())
    }

    fn draw(&self, terminal: &mut ratatui::Frame<'_>) {
        match self.mode {
            Mode::SelectRenderer => self.draw_renderer_selection(terminal),
            Mode::SelectServer => self.draw_server_selection(terminal),
            Mode::Browse => self.draw_browser(terminal),
            _ => self.draw_control_screen(terminal),
        }

        if self.show_queue_overlay {
            self.draw_queue_overlay(terminal);
        }

        if self.show_help_overlay {
            self.draw_help_overlay(terminal);
        }

        if matches!(self.mode, Mode::BindingPrompt) {
            self.draw_binding_prompt(terminal);
        }
    }

    fn draw_renderer_selection(&self, f: &mut ratatui::Frame<'_>) {
        let area = f.size();
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Sélection du renderer");
        let items: Vec<ListItem> = self
            .renderers
            .iter()
            .map(|info| {
                let text = format!("{} | {}", info.friendly_name, info.model_name);
                ListItem::new(text)
            })
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        let mut state = ListState::default();
        state.select(Some(self.renderer_index));
        f.render_stateful_widget(list, area, &mut state);
        self.draw_status_line(f);
    }

    fn draw_server_selection(&self, f: &mut ratatui::Frame<'_>) {
        let area = f.size();
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Sélection du serveur");
        let items: Vec<ListItem> = self
            .servers
            .iter()
            .map(|info| {
                let text = format!("{} | {}", info.friendly_name, info.model_name);
                ListItem::new(text)
            })
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        let mut state = ListState::default();
        state.select(Some(self.server_index));
        f.render_stateful_widget(list, area, &mut state);
        self.draw_status_line(f);
    }

    fn draw_browser(&self, f: &mut ratatui::Frame<'_>) {
        let area = f.size();
        let Some(browser) = &self.browser else {
            self.draw_status_line(f);
            return;
        };

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            format!(
                "Navigation: {} (id: {})",
                browser.nav_state.current_container_title, browser.nav_state.current_container_id
            ),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

        let items: Vec<ListItem> = browser
            .entries
            .iter()
            .map(|entry| {
                let icon = if entry.is_container { "📁" } else { "♪" };
                let label = format!("{} {}", icon, entry.title);
                ListItem::new(label)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        let mut state = ListState::default();
        state.select(Some(browser.selected_index));
        f.render_stateful_widget(list, area, &mut state);
        self.draw_status_line(f);
    }

    fn draw_control_screen(&self, f: &mut ratatui::Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(8),
                Constraint::Length(3),
            ])
            .split(f.size());

        self.draw_header(f, chunks[0]);
        self.draw_playback_panel(f, chunks[1]);
        self.draw_help(f, chunks[2]);

        self.draw_status_line(f);
    }

    fn draw_header(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let ui = &self.ui_state;
        let renderer = &ui.renderer_name;
        let server = ui.server_name.as_deref().unwrap_or("<serveur?>");
        let state = ui
            .playback_state
            .as_ref()
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "Inconnu".to_string());
        let volume = ui
            .volume
            .map(|v| v.to_string())
            .unwrap_or_else(|| "--".to_string());
        let mute = match ui.mute {
            Some(true) => "ON",
            Some(false) => "OFF",
            None => "??",
        };

        let text = vec![
            Line::from(vec![Span::styled(
                format!("Renderer : {renderer}"),
                Style::default().fg(Color::Yellow),
            )]),
            Line::from(vec![Span::raw(format!("Serveur  : {server}"))]),
            Line::from(vec![Span::raw(format!(
                "État     : {state}  Volume {volume} | Mute {mute}"
            ))]),
        ];

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Statut"))
            .alignment(Alignment::Left);
        f.render_widget(paragraph, area);
    }

    fn draw_playback_panel(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(3)])
            .split(area);

        let meta_lines = render_metadata_block(self.ui_state.metadata.as_ref());
        let paragraph = Paragraph::new(meta_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Lecture en cours"),
            )
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(paragraph, chunks[0]);

        let gauge_area = chunks[1];
        let gauge = match self.ui_state.position.as_ref() {
            Some(pos) => build_progress_gauge(pos),
            None => Gauge::default()
                .block(Block::default().borders(Borders::ALL).title("Progression"))
                .label("en attente...")
                .ratio(0.0),
        };
        f.render_widget(gauge, gauge_area);
    }

    fn draw_help(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let lines = vec![
            Line::from("Commandes: h=Aide détaillée | R=Renderer | S=Serveur"),
            Line::from("           r=Play p=Pause s=Stop n=Next"),
            Line::from("           +=Vol+ -=Vol- m=Mute i=Infos k=Queue b=Binding"),
            Line::from("           q=Quit | ESC ferme les overlays"),
        ];
        let paragraph =
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Raccourcis"));
        f.render_widget(paragraph, area);
    }

    fn draw_help_overlay(&self, f: &mut ratatui::Frame<'_>) {
        let area = centered_rect(70, 60, f.size());
        let lines = vec![
            Line::from("Raccourcis disponibles:"),
            Line::from("  R / S   : re-sélection du renderer / serveur"),
            Line::from("  n / p   : navigation dans la file"),
            Line::from("  +/-/m   : volume et mute"),
            Line::from("  k       : afficher la queue actuelle"),
            Line::from("  b       : afficher le binding playlist"),
            Line::from("  h       : fermer cette aide"),
            Line::from("  ESC     : fermer les overlays"),
        ];
        let block = Block::default()
            .title("Aide détaillée (h pour fermer)")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_queue_overlay(&self, f: &mut ratatui::Frame<'_>) {
        let area = centered_rect(80, 70, f.size());
        let mut lines = Vec::new();
        lines.push(Line::from("Playlist actuelle (▶ = en cours):"));
        if self.queue_snapshot.is_empty() {
            lines.push(Line::from("  <vide>"));
        } else {
            for (idx, item) in self.queue_snapshot.iter().enumerate() {
                let title = item.title.as_deref().unwrap_or("<titre>");
                let artist = item.artist.as_deref().unwrap_or("");
                let prefix = match self.queue_current_index {
                    Some(current) if current == idx => "▶",
                    _ => " ",
                };
                let line = if artist.is_empty() {
                    format!("{prefix} [{idx}] {title}")
                } else {
                    format!("{prefix} [{idx}] {artist} - {title}")
                };
                lines.push(Line::from(line));
            }
        }
        let block = Block::default()
            .title("Playlist (fermer avec k ou Esc)")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_binding_prompt(&self, f: &mut ratatui::Frame<'_>) {
        let area = centered_rect(60, 40, f.size());
        let lines = vec![
            Line::from("Attacher la file au conteneur pour auto-refresh ?"),
            Line::from("y = oui | n = non"),
        ];
        let block = Block::default()
            .title("Binding playlist")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_status_line(&self, f: &mut ratatui::Frame<'_>) {
        let area = Rect {
            x: 0,
            y: f.size().height.saturating_sub(1),
            width: f.size().width,
            height: 1,
        };
        let status = self
            .ui_state
            .last_status
            .clone()
            .unwrap_or_else(|| self.status_line.clone());
        let paragraph = Paragraph::new(status).style(Style::default().fg(Color::Gray));
        f.render_widget(paragraph, area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        match self.mode {
            Mode::SelectRenderer => self.handle_renderer_key(key),
            Mode::SelectServer => self.handle_server_key(key),
            Mode::Browse => self.handle_browse_key(key),
            Mode::BindingPrompt => self.handle_binding_key(key),
            Mode::Control => self.handle_control_key(key),
        }
    }

    fn handle_renderer_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Up => {
                if self.renderer_index > 0 {
                    self.renderer_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.renderer_index + 1 < self.renderers.len() {
                    self.renderer_index += 1;
                }
            }
            KeyCode::Enter => {
                let info = self.renderers[self.renderer_index].clone();
                if let Some(current) = &self.renderer_info {
                    if current.id != info.id {
                        let _ = self.stop_current_renderer_playback();
                    }
                }
                self.ui_state = UiState::new(info.friendly_name.clone());
                self.renderer_info = Some(info);
                self.server_info = None;
                self.music_server = None;
                self.browser = None;
                self.queue_snapshot.clear();
                self.queue_current_index = None;
                self.known_tracks.clear();
                self.pending_binding_container = None;
                self.ui_state.current_track_uri = None;
                self.ui_state.server_name = None;
                self.show_queue_overlay = false;
                self.show_help_overlay = false;
                self.mode = Mode::SelectServer;
                self.update_renderer_status_from_device();
                self.status_line = "Sélectionne un serveur avec ↑/↓ et Entrée".to_string();
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_server_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Up => {
                if self.server_index > 0 {
                    self.server_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.server_index + 1 < self.servers.len() {
                    self.server_index += 1;
                }
            }
            KeyCode::Enter => {
                let info = self.servers[self.server_index].clone();
                match MusicServer::from_info(&info, Duration::from_secs(DEFAULT_TIMEOUT_SECS)) {
                    Ok(server) => {
                        let entries = server.browse_root()?;
                        let browser = BrowserState::new(entries);
                        self.music_server = Some(server);
                        self.server_info = Some(info.clone());
                        self.browser = Some(browser);
                        self.mode = Mode::Browse;
                        self.ui_state.server_name = Some(info.friendly_name.clone());
                        self.status_line =
                            "Navigue avec ↑/↓, Entrée pour ouvrir, s pour sélectionner".to_string();
                    }
                    Err(err) => {
                        self.ui_state
                            .set_status(format!("MusicServer init failed: {err}"));
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_browse_key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.code == KeyCode::Char('q') {
            return Ok(true);
        }
        let Some(browser) = self.browser.as_mut() else {
            return Ok(false);
        };
        let Some(server) = self.music_server.as_mut() else {
            return Ok(false);
        };

        match key.code {
            KeyCode::Up => {
                if browser.selected_index > 0 {
                    browser.selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if browser.selected_index + 1 < browser.entries.len() {
                    browser.selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = browser.current_entry() {
                    if entry.is_container {
                        match server.browse_children(&entry.id, 0, 100) {
                            Ok(children) => {
                                browser
                                    .nav_state
                                    .enter_container(entry.id.clone(), entry.title.clone());
                                browser.entries = children;
                                browser.selected_index = 0;
                            }
                            Err(err) => {
                                self.ui_state
                                    .set_status(format!("Impossible d'ouvrir: {err}"));
                            }
                        }
                    }
                }
            }
            KeyCode::Char('b') => {
                if browser.nav_state.go_back() {
                    let entries = if browser.nav_state.current_container_id == "0" {
                        server.browse_root()?
                    } else {
                        server.browse_children(&browser.nav_state.current_container_id, 0, 100)?
                    };
                    browser.entries = entries;
                    browser.selected_index = 0;
                }
            }
            KeyCode::Char('s') => {
                self.enqueue_current_container()?;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_binding_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') => {
                self.attach_binding(true)?;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.attach_binding(false)?;
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    fn handle_control_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('R') => {
                self.open_renderer_menu();
            }
            KeyCode::Char('S') => {
                self.open_server_menu();
            }
            KeyCode::Char('h') => {
                self.show_help_overlay = !self.show_help_overlay;
                if self.show_help_overlay {
                    self.ui_state.set_status("Aide ouverte");
                } else {
                    self.ui_state.set_status("Aide fermée");
                }
            }
            KeyCode::Char('p') => {
                self.pause_renderer()?;
            }
            KeyCode::Char('r') => {
                self.resume_renderer()?;
            }
            KeyCode::Char('s') => {
                self.stop_renderer()?;
            }
            KeyCode::Char('n') => {
                self.play_next()?;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.adjust_volume(5)?;
            }
            KeyCode::Char('-') => {
                self.adjust_volume(-5)?;
            }
            KeyCode::Char('m') => {
                self.toggle_mute()?;
            }
            KeyCode::Char('i') => {
                self.show_renderer_info()?;
            }
            KeyCode::Char('k') => {
                self.show_queue_overlay = !self.show_queue_overlay;
            }
            KeyCode::Char('b') => {
                self.show_binding();
            }
            KeyCode::Esc => {
                self.show_queue_overlay = false;
                self.show_help_overlay = false;
            }
            _ => {}
        }
        Ok(false)
    }

    fn enqueue_current_container(&mut self) -> Result<()> {
        let Some(browser) = self.browser.as_ref() else {
            return Ok(());
        };
        let current_container_id = browser.nav_state.current_container_id.clone();
        let entries = browser.entries.clone();
        let Some(server) = self.music_server.as_ref() else {
            return Ok(());
        };
        let renderer_id = self
            .renderer_id()
            .ok_or_else(|| anyhow!("Renderer not selected"))?;

        let items = collect_playable_items(server, &entries)?;
        if items.is_empty() {
            self.ui_state.set_status("Aucun média dans ce conteneur");
            return Ok(());
        }

        self.control_point.clear_queue(&renderer_id)?;
        self.control_point.enqueue_items(&renderer_id, items)?;
        let (full_queue, current_index) =
            self.control_point
                .get_full_queue_snapshot(&renderer_id)
                .context("failed to snapshot queue after enqueue")?;
        self.queue_snapshot = full_queue;
        self.queue_current_index = current_index;
        self.record_queue_metadata();
        self.pending_binding_container = Some(current_container_id);
        self.status_line = format!("File prête ({})", self.queue_snapshot.len());
        self.ui_state.set_status(&self.status_line);
        self.mode = Mode::BindingPrompt;
        Ok(())
    }

    fn attach_binding(&mut self, attach: bool) -> Result<()> {
        if attach {
            if let (Some(server), Some(container), Some(renderer_id)) = (
                &self.server_info,
                self.pending_binding_container.clone(),
                self.renderer_id(),
            ) {
                self.control_point.attach_queue_to_playlist(
                    &renderer_id,
                    server.id.clone(),
                    container.clone(),
                );
                self.ui_state
                    .set_status(format!("File liée à '{}'", container));
            }
        } else {
            self.ui_state.set_status("File locale uniquement");
        }

        self.pending_binding_container = None;
        self.mode = Mode::Control;
        self.start_playback()?;
        self.update_renderer_status_from_device();
        Ok(())
    }

    fn start_playback(&mut self) -> Result<()> {
        let renderer_id = self
            .renderer_id()
            .ok_or_else(|| anyhow!("Renderer not selected"))?;
        let next_item = self.peek_next_queue_item(&renderer_id);
        self.control_point.play_next_from_queue(&renderer_id)?;
        if let Some(item) = next_item {
            self.apply_item_as_current(&item);
        } else {
            self.ui_state.set_status("File vide");
        }
        self.refresh_queue_snapshot(&renderer_id);
        Ok(())
    }

    fn pause_renderer(&mut self) -> Result<()> {
        let renderer = self.get_renderer()?;
        renderer.pause()?;
        self.ui_state.set_status("Lecture en pause");
        Ok(())
    }

    fn resume_renderer(&mut self) -> Result<()> {
        let renderer = self.get_renderer()?;
        renderer.play()?;
        self.ui_state.set_status("Lecture reprise");
        Ok(())
    }

    fn stop_renderer(&mut self) -> Result<()> {
        let renderer = self.get_renderer()?;
        renderer.stop()?;
        self.ui_state.set_status("Lecture arrêtée");
        Ok(())
    }

    fn play_next(&mut self) -> Result<()> {
        let renderer_id = self
            .renderer_id()
            .ok_or_else(|| anyhow!("Renderer not selected"))?;
        let next_item = self.peek_next_queue_item(&renderer_id);
        self.control_point.play_next_from_queue(&renderer_id)?;
        if let Some(item) = next_item {
            self.apply_item_as_current(&item);
        } else {
            self.ui_state.set_status("Piste suivante (file vide)");
        }
        self.refresh_queue_snapshot(&renderer_id);
        Ok(())
    }

    fn adjust_volume(&mut self, delta: i32) -> Result<()> {
        let renderer = self.get_renderer()?;
        let current = renderer.volume()?;
        let new_volume = (current as i32 + delta).clamp(0, 100) as u16;
        renderer.set_volume(new_volume)?;
        self.ui_state.set_status(format!("Volume → {new_volume}"));
        self.ui_state.volume = Some(new_volume);
        Ok(())
    }

    fn toggle_mute(&mut self) -> Result<()> {
        let renderer = self.get_renderer()?;
        let current = renderer.mute()?;
        renderer.set_mute(!current)?;
        self.ui_state.mute = Some(!current);
        self.ui_state.set_status(if !current {
            "Mute activé"
        } else {
            "Mute désactivé"
        });
        Ok(())
    }

    fn show_renderer_info(&mut self) -> Result<()> {
        let renderer = self.get_renderer()?;
        let info = renderer.info();
        self.ui_state.set_status(format!(
            "Renderer: {} ({})",
            info.friendly_name, info.model_name
        ));
        Ok(())
    }

    fn show_binding(&mut self) {
        if let Some(renderer_id) = self.renderer_id() {
            if let Some((server_id, container_id, has_seen_update)) = self
                .control_point
                .current_queue_playlist_binding(&renderer_id)
            {
                self.ui_state.set_status(format!(
                    "Binding: {} -> {} (maj vue: {})",
                    server_id.0, container_id, has_seen_update
                ));
            } else {
                self.ui_state.set_status("Pas de binding actif");
            }
        }
    }

    fn stop_current_renderer_playback(&self) -> Result<()> {
        if let Some(renderer_id) = self.renderer_id() {
            if let Some(renderer) = self.control_point.music_renderer_by_id(&renderer_id) {
                renderer.stop()?;
            }
        }
        Ok(())
    }

    fn update_renderer_status_from_device(&mut self) {
        if let Ok(renderer) = self.get_renderer() {
            if let Ok(volume) = renderer.volume() {
                self.ui_state.volume = Some(volume);
            }
            if let Ok(mute) = renderer.mute() {
                self.ui_state.mute = Some(mute);
            }
            if let Ok(state) = renderer.playback_state() {
                self.ui_state.playback_state = Some(state);
            }
            if let Ok(position) = renderer.playback_position() {
                self.ui_state.position = Some(position);
            }
        }
    }

    fn open_renderer_menu(&mut self) {
        self.mode = Mode::SelectRenderer;
        self.status_line = "Sélectionne un renderer avec ↑/↓ et Entrée".to_string();
        self.ui_state.set_status("Menu renderer ouvert");
        self.show_queue_overlay = false;
        self.show_help_overlay = false;
    }

    fn open_server_menu(&mut self) {
        if self.renderer_info.is_some() {
            self.mode = Mode::SelectServer;
            self.status_line = "Sélectionne un serveur avec ↑/↓ et Entrée".to_string();
            self.ui_state.set_status("Menu serveur ouvert");
            self.show_queue_overlay = false;
            self.show_help_overlay = false;
        } else {
            self.ui_state.set_status("Sélectionne d'abord un renderer");
        }
    }

    fn record_queue_metadata(&mut self) {
        for item in &self.queue_snapshot {
            if let Some(meta) = playback_metadata_from_item(item) {
                self.known_tracks.insert(item.uri.clone(), meta);
            }
        }
    }

    fn apply_item_as_current(&mut self, item: &PlaybackItem) {
        if let Some(meta) = playback_metadata_from_item(item) {
            self.known_tracks.insert(item.uri.clone(), meta.clone());
            self.ui_state.metadata = Some(meta.clone());
            self.ui_state.set_status(format_track_status(&meta));
        } else {
            let label = item
                .title
                .as_deref()
                .map(|t| t.to_string())
                .unwrap_or_else(|| item.uri.clone());
            self.ui_state.metadata = None;
            self.ui_state.set_status(format!("Lecture: {label}"));
        }
        self.ui_state.current_track_uri = Some(item.uri.clone());
    }

    fn update_metadata_from_uri(&mut self, uri: &str) {
        self.ui_state.current_track_uri = Some(uri.to_string());
        if let Some(meta) = self.known_tracks.get(uri).cloned() {
            self.ui_state.metadata = Some(meta.clone());
            self.ui_state.set_status(format_track_status(&meta));
        } else {
            self.ui_state.metadata = None;
            self.ui_state.set_status(format!("Lecture: {uri}"));
        }
    }

    fn refresh_queue_snapshot(&mut self, renderer_id: &pmocontrol::model::RendererId) {
        if let Ok((queue, current_index)) =
            self.control_point.get_full_queue_snapshot(renderer_id)
        {
            self.queue_snapshot = queue;
            self.queue_current_index = current_index;
            self.record_queue_metadata();
            if let Some(idx) = self.queue_current_index {
                if let Some(item) = self.queue_snapshot.get(idx).cloned() {
                    let needs_update = self
                        .ui_state
                        .current_track_uri
                        .as_deref()
                        != Some(item.uri.as_str());
                    if needs_update {
                        self.apply_item_as_current(&item);
                    }
                }
            }
        }
    }

    fn peek_next_queue_item(
        &self,
        renderer_id: &pmocontrol::model::RendererId,
    ) -> Option<PlaybackItem> {
        self.control_point
            .get_queue_snapshot(renderer_id)
            .ok()
            .and_then(|queue| queue.into_iter().next())
    }

    fn get_renderer(&self) -> Result<pmocontrol::MusicRenderer> {
        let renderer_id = self
            .renderer_id()
            .ok_or_else(|| anyhow!("Renderer not selected"))?;
        self.control_point
            .music_renderer_by_id(&renderer_id)
            .ok_or_else(|| anyhow!("Renderer not found"))
    }

    fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Renderer(ev) => self.handle_renderer_event(ev),
            AppEvent::Media(ev) => self.handle_media_event(ev),
        }
    }

    fn handle_renderer_event(&mut self, event: RendererEvent) {
        let Some(renderer_id) = self.renderer_id() else {
            return;
        };
        match event {
            RendererEvent::StateChanged { id, state } => {
                if id == renderer_id {
                    self.ui_state.playback_state = Some(state.clone());
                    self.ui_state.set_status(format!("État: {:?}", state));
                }
            }
            RendererEvent::PositionChanged { id, position } => {
                if id == renderer_id {
                    let track_changed =
                        position.track_uri.as_ref() != self.ui_state.current_track_uri.as_ref();
                    self.ui_state.position = Some(position.clone());
                    if track_changed {
                        if let Some(uri) = position.track_uri.as_deref() {
                            self.update_metadata_from_uri(uri);
                            self.refresh_queue_snapshot(&renderer_id);
                        } else {
                            self.ui_state.current_track_uri = None;
                        }
                    }
                }
            }
            RendererEvent::VolumeChanged { id, volume } => {
                if id == renderer_id {
                    self.ui_state.volume = Some(volume);
                    self.ui_state.set_status(format!("Volume → {volume}"));
                }
            }
            RendererEvent::MuteChanged { id, mute } => {
                if id == renderer_id {
                    self.ui_state.mute = Some(mute);
                    self.ui_state.set_status(if mute {
                        "Mute activé"
                    } else {
                        "Mute désactivé"
                    });
                }
            }
            RendererEvent::MetadataChanged { id, metadata } => {
                if id == renderer_id {
                    self.ui_state.metadata = Some(metadata.clone());
                    self.ui_state.set_status(format_track_status(&metadata));
                    if let Some(uri) = self.ui_state.current_track_uri.clone() {
                        self.known_tracks.insert(uri, metadata);
                    }
                }
            }
            RendererEvent::QueueUpdated { id, queue_length } => {
                if id == renderer_id {
                    self.refresh_queue_snapshot(&renderer_id);
                    self.ui_state
                        .set_status(format!("File mise à jour ({queue_length})"));
                }
            }
        }
    }

    fn handle_media_event(&mut self, event: MediaServerEvent) {
        match event {
            MediaServerEvent::GlobalUpdated {
                server_id,
                system_update_id,
            } => {
                self.ui_state.set_status(format!(
                    "MAJ serveur {} (SystemUpdateID={})",
                    server_id.0,
                    system_update_id.unwrap_or(0)
                ));
            }
            MediaServerEvent::ContainersUpdated {
                server_id,
                container_ids,
            } => {
                let mut status = format!(
                    "Conteneurs mis à jour sur {}: {:?}",
                    server_id.0, container_ids
                );
                if let Some(renderer_id) = self.renderer_id() {
                    if let Some((bound_server, bound_container, _)) = self
                        .control_point
                        .current_queue_playlist_binding(&renderer_id)
                    {
                        if bound_server == server_id && container_ids.contains(&bound_container) {
                            status
                                .push_str(&format!(" | Playlist '{}' rafraîchie", bound_container));
                            self.refresh_queue_snapshot(&renderer_id);
                        }
                    }
                }
                self.ui_state.set_status(status);
            }
        }
    }
}

impl BrowserState {
    fn new(entries: Vec<MediaEntry>) -> Self {
        Self {
            nav_state: NavigationState::new("0".to_string(), "Root".to_string()),
            entries,
            selected_index: 0,
        }
    }

    fn current_entry(&self) -> Option<&MediaEntry> {
        self.entries.get(self.selected_index)
    }
}

fn run_app(mut app: App) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let (event_tx, event_rx) = mpsc::channel();
    let mut renderer_events_started = false;
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| app.draw(f))?;

        while let Ok(event) = event_rx.try_recv() {
            app.handle_app_event(event);
        }

        if !renderer_events_started {
            if app.renderer_id().is_some() {
                start_event_threads(Arc::clone(&app.control_point), event_tx.clone());
                renderer_events_started = true;
            }
        }

        let timeout = TICK_RATE
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.handle_key(key)? {
                    let _ = app.stop_current_renderer_playback();
                    break;
                }
            }
        }

        if last_tick.elapsed() >= TICK_RATE {
            last_tick = Instant::now();
        }
    }

    restore_terminal(&mut terminal)?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn start_event_threads(control_point: Arc<ControlPoint>, tx: Sender<AppEvent>) {
    let renderer_cp = Arc::clone(&control_point);
    let renderer_tx = tx.clone();
    thread::spawn(move || {
        let event_rx = renderer_cp.subscribe_events();
        for event in event_rx {
            if renderer_tx.send(AppEvent::Renderer(event)).is_err() {
                break;
            }
        }
    });

    thread::spawn(move || {
        let media_rx = control_point.subscribe_media_server_events();
        for event in media_rx {
            if tx.send(AppEvent::Media(event)).is_err() {
                break;
            }
        }
    });
}

/// Navigation state for ContentDirectory browsing.
struct NavigationState {
    path_stack: Vec<(String, String)>,
    current_container_id: String,
    current_container_title: String,
}

impl NavigationState {
    fn new(root_id: String, root_title: String) -> Self {
        Self {
            path_stack: Vec::new(),
            current_container_id: root_id,
            current_container_title: root_title,
        }
    }

    fn enter_container(&mut self, container_id: String, container_title: String) {
        self.path_stack.push((
            self.current_container_id.clone(),
            self.current_container_title.clone(),
        ));
        self.current_container_id = container_id;
        self.current_container_title = container_title;
    }

    fn go_back(&mut self) -> bool {
        if let Some((parent_id, parent_title)) = self.path_stack.pop() {
            self.current_container_id = parent_id;
            self.current_container_title = parent_title;
            true
        } else {
            false
        }
    }
}

/// Collect playable items from MediaEntry list (including nested containers).
fn collect_playable_items(
    server: &MusicServer,
    entries: &[MediaEntry],
) -> Result<Vec<PlaybackItem>> {
    let mut items = Vec::new();
    for entry in entries {
        if entry.is_container {
            match server.browse_children(&entry.id, 0, 100) {
                Ok(children) => {
                    items.extend(collect_playable_items(server, &children)?);
                }
                Err(err) => {
                    eprintln!(
                        "Warning: failed to browse container '{}': {err}",
                        entry.title
                    );
                }
            }
        } else if let Some(item) = playback_item_from_entry(server, entry) {
            items.push(item);
        }
    }
    Ok(items)
}

/// Convert MediaEntry to PlaybackItem.
fn playback_item_from_entry(server: &MusicServer, entry: &MediaEntry) -> Option<PlaybackItem> {
    let resource = entry.resources.iter().find(|res| is_audio_resource(res))?;
    let mut item = PlaybackItem::new(resource.uri.clone());
    item.title = Some(entry.title.clone());
    item.server_id = Some(server.id().clone());
    item.object_id = Some(entry.id.clone());
    item.artist = entry.artist.clone();
    item.album = entry.album.clone();
    item.genre = entry.genre.clone();
    item.album_art_uri = entry.album_art_uri.clone();
    item.date = entry.date.clone();
    item.track_number = entry.track_number.clone();
    item.creator = entry.creator.clone();
    Some(item)
}

fn playback_metadata_from_item(item: &PlaybackItem) -> Option<TrackMetadata> {
    let metadata = TrackMetadata {
        title: item.title.clone(),
        artist: item.artist.clone(),
        album: item.album.clone(),
        genre: item.genre.clone(),
        album_art_uri: item.album_art_uri.clone(),
        date: item.date.clone(),
        track_number: item.track_number.clone(),
        creator: item.creator.clone(),
    };

    if metadata.title.is_none()
        && metadata.artist.is_none()
        && metadata.album.is_none()
        && metadata.genre.is_none()
        && metadata.album_art_uri.is_none()
        && metadata.date.is_none()
        && metadata.track_number.is_none()
        && metadata.creator.is_none()
    {
        return None;
    }

    Some(metadata)
}

/// Check if MediaResource is audio.
fn is_audio_resource(res: &MediaResource) -> bool {
    let lower = res.protocol_info.to_ascii_lowercase();
    if lower.contains("audio/") {
        return true;
    }
    lower
        .split(':')
        .nth(2)
        .map(|mime| mime.starts_with("audio/"))
        .unwrap_or(false)
}

fn render_metadata_block(metadata: Option<&TrackMetadata>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(meta) = metadata {
        let title = meta
            .title
            .clone()
            .unwrap_or_else(|| "<Titre inconnu>".to_string());
        lines.push(Line::from(format!("Titre  : {title}")));
        if let Some(artist) = meta.artist.as_deref() {
            lines.push(Line::from(format!("Artiste: {artist}")));
        }
        if let Some(album) = meta.album.as_deref() {
            lines.push(Line::from(format!("Album  : {album}")));
        }
        if let Some(genre) = meta.genre.as_deref() {
            lines.push(Line::from(format!("Genre  : {genre}")));
        }
        if let Some(date) = meta.date.as_deref() {
            lines.push(Line::from(format!("Date   : {}", format_date(date))));
        }
        if let Some(track) = meta.track_number.as_deref() {
            lines.push(Line::from(format!("Piste  : {track}")));
        }
        if let Some(art) = meta.album_art_uri.as_deref() {
            lines.push(Line::from(format!("Cover  : {art}")));
        }
    } else {
        lines.push(Line::from("(En attente des métadonnées...)"));
    }
    lines
}

fn format_date(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('-').collect();
    if parts.len() == 3 && parts[1] == "01" && parts[2] == "01" {
        return parts[0].to_string();
    }
    raw.to_string()
}

fn build_progress_gauge(position: &PlaybackPositionInfo) -> Gauge<'static> {
    let rel_secs = position.rel_time.as_deref().and_then(parse_time_to_seconds);
    let dur_secs = position
        .track_duration
        .as_deref()
        .and_then(parse_time_to_seconds);

    let ratio = match (rel_secs, dur_secs) {
        (Some(rel), Some(dur)) if dur > 0 => rel as f64 / dur as f64,
        _ => 0.0,
    };
    let ratio = ratio.clamp(0.0, 1.0);

    let rel_label = position
        .rel_time
        .clone()
        .unwrap_or_else(|| "--:--".to_string());
    let dur_label = position
        .track_duration
        .clone()
        .unwrap_or_else(|| "--:--".to_string());

    let label = format!("{} / {}", rel_label, dur_label);
    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progression"))
        .gauge_style(Style::default().fg(Color::Magenta))
        .ratio(ratio)
        .label(label)
}

fn format_track_status(meta: &TrackMetadata) -> String {
    let title = meta.title.as_deref().unwrap_or("<Titre inconnu>");
    let artist = meta.artist.as_deref().unwrap_or("");
    if artist.is_empty() {
        format!("Lecture: {title}")
    } else {
        format!("Lecture: {artist} - {title}")
    }
}

fn parse_time_to_seconds(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return None;
    }
    if let Ok(seconds) = trimmed.parse::<u64>() {
        return Some(seconds);
    }

    let parts: Vec<&str> = trimmed.split(':').collect();
    match parts.len() {
        3 => {
            let hours = parts[0].parse::<u64>().ok()?;
            let minutes = parts[1].parse::<u64>().ok()?;
            let seconds = parse_seconds(parts[2])?;
            Some(hours * 3600 + minutes * 60 + seconds)
        }
        2 => {
            let minutes = parts[0].parse::<u64>().ok()?;
            let seconds = parse_seconds(parts[1])?;
            Some(minutes * 60 + seconds)
        }
        _ => None,
    }
}

fn parse_seconds(fragment: &str) -> Option<u64> {
    fragment
        .split('.')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
