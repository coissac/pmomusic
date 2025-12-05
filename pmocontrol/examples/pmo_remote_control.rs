//! Remote REST-based control point demo using Ratatui.
//!
//! This example mirrors the UX of `full_control_point_demo.rs` but drives a
//! remote PMOMusic server exclusively through the `/api/control` REST API.

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Stdout, Write};
use std::process;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph};
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use ureq::http;
use ureq::{Agent, Body};

const DEFAULT_BASE_URL: &str = "http://localhost:8080/api/control";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const TICK_RATE: Duration = Duration::from_millis(200);
const ROOT_CONTAINERS: &[&str] = &["0", "0$"];

fn main() -> Result<()> {
    // Install panic handler to restore terminal even on panic
    std::panic::set_hook(Box::new(|panic_info| {
        // Force terminal restoration
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        eprintln!("\n\n❌ Application panicked: {:?}", panic_info);
        eprintln!("Terminal has been restored. You can now close this window safely.");
    }));

    init_tracing();
    let options = resolve_options()?;
    info!(
        base_url = %options.base_url,
        timeout_ms = options.timeout.as_millis(),
        "Démarrage du client REST"
    );
    println!("PMO Remote Control demo");
    println!("Using Control API at {}", options.base_url);

    let client = RestClient::new(&options.base_url, options.timeout)?;
    let renderers = client
        .list_renderers()
        .context("Impossible de récupérer la liste des renderers")?;
    if renderers.is_empty() {
        eprintln!(
            "Aucun renderer disponible via {}. Lancement annulé.",
            options.base_url
        );
        return Ok(());
    }

    let app = App::new(client, renderers);
    if let Err(err) = run_app(app) {
        eprintln!("Application fermée avec erreur: {err}");
    }

    println!("\nAu revoir !");
    Ok(())
}

struct AppOptions {
    base_url: String,
    timeout: Duration,
}

fn resolve_options() -> Result<AppOptions> {
    let mut args = env::args().skip(1);
    let mut cli_base: Option<String> = None;
    let mut cli_timeout: Option<u64> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base-url" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("--base-url requiert une valeur"))?;
                cli_base = Some(value);
            }
            "--timeout-ms" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("--timeout-ms requiert une valeur"))?;
                let millis: u64 = value
                    .parse()
                    .with_context(|| format!("Valeur invalide pour --timeout-ms: {value}"))?;
                cli_timeout = Some(millis);
            }
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            other => bail!("Argument inconnu: {other}. Utilise --help pour l'aide."),
        }
    }
    let base = cli_base
        .or_else(|| env::var("PMO_REMOTE_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let timeout_ms = cli_timeout
        .or_else(|| {
            env::var("PMO_REMOTE_TIMEOUT_MS")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or_else(|| DEFAULT_TIMEOUT.as_millis() as u64);
    let timeout = Duration::from_millis(timeout_ms.max(1));
    Ok(AppOptions {
        base_url: base,
        timeout,
    })
}

fn print_usage() {
    println!(
        "Usage: cargo run -p pmocontrol --example pmo_remote_control [-- --base-url <url> --timeout-ms <ms>]"
    );
    println!("Variables d'environnement:");
    println!(
        "  PMO_REMOTE_BASE_URL  Override la base de l'API REST (par défaut {DEFAULT_BASE_URL})"
    );
    println!(
        "  PMO_REMOTE_TIMEOUT_MS  Timeout HTTP global en millisecondes (par défaut {})",
        DEFAULT_TIMEOUT.as_millis()
    );
    println!(
        "  PMO_REMOTE_LOG_FILE  Écrit les logs tracing dans ce fichier (append) au lieu de stderr"
    );
    println!(
        "  RUST_LOG              Active le filtrage tracing/log (ex: pmocontrol=debug,ureq=debug)"
    );
}

fn init_tracing() {
    let _ = tracing_log::LogTracer::init();
    let writer = log_writer();
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(writer)
        .try_init();
}

fn log_writer() -> BoxMakeWriter {
    if let Ok(path) = env::var("PMO_REMOTE_LOG_FILE") {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => {
                let shared = SharedLogWriter::new(file);
                let writer = BoxMakeWriter::new(move || shared.clone());
                return writer;
            }
            Err(err) => {
                eprintln!(
                    "Impossible d'ouvrir {path} pour les logs tracing: {err}. Retour à stderr"
                );
            }
        }
    }
    BoxMakeWriter::new(io::stderr)
}

#[derive(Clone)]
struct SharedLogWriter {
    inner: Arc<Mutex<File>>,
}

impl SharedLogWriter {
    fn new(file: File) -> Self {
        Self {
            inner: Arc::new(Mutex::new(file)),
        }
    }
}

impl Write for SharedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        guard.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        guard.flush()
    }
}

struct App {
    client: RestClient,
    renderers: Vec<RendererSummaryClient>,
    renderer_index: usize,
    selected_renderer: Option<RendererSummaryClient>,
    servers: Vec<MediaServerSummaryClient>,
    server_index: usize,
    selected_server: Option<MediaServerSummaryClient>,
    browser: Option<BrowserState>,
    mode: Mode,
    ui_state: UiState,
    queue_snapshot: Vec<QueueItemClient>,
    queue_current_index: Option<usize>,
    binding_info: Option<AttachedPlaylistInfoClient>,
    show_queue_overlay: bool,
    show_help_overlay: bool,
    pending_binding: Option<PendingBinding>,
    status_line: String,
    binding_worker: Option<mpsc::Receiver<BindingWorkerMessage>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    SelectRenderer,
    SelectServer,
    Browse,
    BindingPrompt,
    Control,
}

#[derive(Clone)]
struct UiState {
    renderer_name: String,
    server_name: Option<String>,
    transport_state: Option<String>,
    progress: Option<PlaybackProgress>,
    volume: Option<u8>,
    mute: Option<bool>,
    metadata: Option<TrackMetadata>,
    last_status: Option<String>,
}

#[derive(Clone)]
struct PlaybackProgress {
    position_ms: Option<u64>,
    duration_ms: Option<u64>,
}

#[derive(Clone)]
struct TrackMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    album_art_uri: Option<String>,
}

struct PendingBinding {
    container_id: String,
    container_title: String,
}

enum BindingWorkerMessage {
    Success { container_title: String },
    Failure { error: String },
}

struct BrowserState {
    server_id: String,
    nav_state: NavigationState,
    entries: Vec<ContainerEntryClient>,
    selected_index: usize,
}

struct NavigationState {
    path_stack: Vec<(String, String)>,
    current_container_id: String,
    current_container_title: String,
}

impl App {
    fn new(client: RestClient, renderers: Vec<RendererSummaryClient>) -> Self {
        Self {
            client,
            renderers,
            renderer_index: 0,
            selected_renderer: None,
            servers: Vec::new(),
            server_index: 0,
            selected_server: None,
            browser: None,
            mode: Mode::SelectRenderer,
            ui_state: UiState::placeholder(),
            queue_snapshot: Vec::new(),
            queue_current_index: None,
            binding_info: None,
            show_queue_overlay: false,
            show_help_overlay: false,
            pending_binding: None,
            status_line: "Sélectionne un renderer avec ↑/↓ puis Entrée".to_string(),
            binding_worker: None,
        }
    }

    fn draw(&self, f: &mut ratatui::Frame<'_>) {
        match self.mode {
            Mode::SelectRenderer => self.draw_renderer_selection(f),
            Mode::SelectServer => self.draw_server_selection(f),
            Mode::Browse => self.draw_browser(f),
            _ => self.draw_control_screen(f),
        };

        if self.show_queue_overlay {
            self.draw_queue_overlay(f);
        }
        if self.show_help_overlay {
            self.draw_help_overlay(f);
        }
        if matches!(self.mode, Mode::BindingPrompt) {
            self.draw_binding_prompt(f);
        }
        self.draw_status_line(f);
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
                let status = if info.online {
                    "[en ligne]"
                } else {
                    "[hors ligne]"
                };
                let text = format!("{status} {} | {}", info.friendly_name, info.model_name);
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
        if !self.renderers.is_empty() {
            state.select(Some(
                self.renderer_index
                    .min(self.renderers.len().saturating_sub(1)),
            ));
        }
        f.render_stateful_widget(list, area, &mut state);
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
                let status = if info.online {
                    "[en ligne]"
                } else {
                    "[hors ligne]"
                };
                let text = format!("{status} {} | {}", info.friendly_name, info.model_name);
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
        if !self.servers.is_empty() {
            state.select(Some(
                self.server_index.min(self.servers.len().saturating_sub(1)),
            ));
        }
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_browser(&self, f: &mut ratatui::Frame<'_>) {
        let area = f.size();
        let Some(browser) = &self.browser else {
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
                let text = format!("{icon} {}", entry.title);
                ListItem::new(text)
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
        if !browser.entries.is_empty() {
            state.select(Some(
                browser
                    .selected_index
                    .min(browser.entries.len().saturating_sub(1)),
            ));
        }
        f.render_stateful_widget(list, area, &mut state);
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
        self.draw_help_strip(f, chunks[2]);
    }

    fn draw_header(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let ui = &self.ui_state;
        let renderer = &ui.renderer_name;
        let server = ui.server_name.as_deref().unwrap_or("<serveur?>");
        let state = ui
            .transport_state
            .as_deref()
            .unwrap_or("État inconnu")
            .to_string();
        let volume = ui
            .volume
            .map(|v| v.to_string())
            .unwrap_or_else(|| "--".to_string());
        let mute = match ui.mute {
            Some(true) => "ON",
            Some(false) => "OFF",
            None => "??",
        };

        let binding = self
            .binding_info
            .as_ref()
            .map(|info| format!("{} / {}", info.server_id, info.container_id))
            .unwrap_or_else(|| "<aucun binding>".to_string());

        let text = vec![
            Line::from(vec![Span::styled(
                format!("Renderer : {renderer}"),
                Style::default().fg(Color::Yellow),
            )]),
            Line::from(vec![Span::raw(format!("Serveur  : {server}"))]),
            Line::from(vec![Span::raw(format!(
                "État     : {state}  Volume {volume} | Mute {mute}"
            ))]),
            Line::from(vec![Span::raw(format!("Playlist : {binding}"))]),
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

        let gauge = match self.ui_state.progress.as_ref() {
            Some(progress) => build_progress_gauge(progress),
            None => Gauge::default()
                .block(Block::default().borders(Borders::ALL).title("Progression"))
                .label("en attente...")
                .ratio(0.0),
        };
        f.render_widget(gauge, chunks[1]);
    }

    fn draw_help_strip(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let lines = vec![
            Line::from("Commandes: h=Aide | R=Renderer | S=Serveur | B=Browse"),
            Line::from("           Espace=Play/Pause s=Stop n=Next m=Mute +/-=Volume k=Queue"),
            Line::from("           b=Binding dans le browser | q=Quit | ESC ferme les overlays"),
        ];
        let paragraph =
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Raccourcis"));
        f.render_widget(paragraph, area);
    }

    fn draw_help_overlay(&self, f: &mut ratatui::Frame<'_>) {
        let area = centered_rect(70, 60, f.size());
        let lines = vec![
            Line::from("Raccourcis disponibles:"),
            Line::from("  R / S          : re-sélectionner renderer / serveur"),
            Line::from("  B              : revenir au navigateur depuis l'écran principal"),
            Line::from("  ↑/↓ ou +/-     : ajuster le volume (via REST)"),
            Line::from("  k / h          : toggle queue ou aide"),
            Line::from(
                "  Browser        : Entrée=ouvrir, ←/Backspace ou r=retour, s ou b=sélectionner",
            ),
            Line::from("  Binding prompt : y=confirmer, n=annuler"),
            Line::from("  ESC            : fermer overlay courant"),
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
        let Some(binding) = &self.pending_binding else {
            return;
        };
        let renderer = self.ui_state.renderer_name.clone();
        let server = self
            .selected_server
            .as_ref()
            .map(|s| s.friendly_name.clone())
            .unwrap_or_else(|| "<serveur?>".to_string());
        let lines = vec![
            Line::from("Attacher ce conteneur comme playlist distante ?"),
            Line::from(format!("Renderer: {renderer}")),
            Line::from(format!("Serveur : {server}")),
            Line::from(format!(
                "Container: {} ({})",
                binding.container_title, binding.container_id
            )),
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
            KeyCode::Esc => return Ok(true),
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
                let previous_renderer_id = self
                    .selected_renderer
                    .as_ref()
                    .map(|renderer| renderer.id.clone());
                let mut stop_error: Option<String> = None;
                if let Some(prev_id) = previous_renderer_id {
                    if prev_id != info.id {
                        if let Err(err) = self.client.stop(&prev_id) {
                            stop_error = Some(format!(
                                "Renderer sélectionné mais arrêt de l'ancien impossible: {err}"
                            ));
                        }
                    }
                }
                self.selected_renderer = Some(info.clone());
                self.ui_state = UiState::new(info.friendly_name.clone());
                self.queue_snapshot.clear();
                self.queue_current_index = None;
                self.binding_info = None;
                self.selected_server = None;
                self.browser = None;
                self.show_help_overlay = false;
                self.show_queue_overlay = false;
                self.pending_binding = None;
                self.mode = Mode::SelectServer;
                self.status_line = "Sélectionne un serveur avec ↑/↓ puis Entrée".to_string();
                self.load_servers();
                self.refresh_renderer_state();
                self.refresh_queue();
                self.refresh_binding_info();
                if let Some(message) = stop_error {
                    self.ui_state.set_status(message);
                } else {
                    self.ui_state.set_status("Renderer sélectionné");
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_server_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Esc => {
                self.mode = Mode::SelectRenderer;
                self.status_line = "Sélectionne un renderer avec ↑/↓ puis Entrée".to_string();
            }
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
                if self.servers.is_empty() {
                    self.ui_state
                        .set_status("Aucun serveur disponible. Vérifie le backend.");
                    return Ok(false);
                }
                let info = self.servers[self.server_index].clone();
                self.selected_server = Some(info.clone());
                self.ui_state.server_name = Some(info.friendly_name.clone());
                match self.load_browser_for_server(&info) {
                    Ok(_) => {
                        self.mode = Mode::Browse;
                        self.status_line =
                            "Navigue avec ↑/↓, Entrée pour ouvrir, ←/Backspace ou r pour remonter, s pour sélectionner"
                                .to_string();
                        self.ui_state.set_status("Serveur sélectionné");
                    }
                    Err(err) => {
                        self.ui_state
                            .set_status(format!("Navigation impossible: {err}"));
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
        if self.browser.is_none() {
            return Ok(false);
        }

        match key.code {
            KeyCode::Up => {
                if let Some(browser) = self.browser.as_mut() {
                    if browser.selected_index > 0 {
                        browser.selected_index -= 1;
                    }
                }
            }
            KeyCode::Down => {
                if let Some(browser) = self.browser.as_mut() {
                    if browser.selected_index + 1 < browser.entries.len() {
                        browser.selected_index += 1;
                    }
                }
            }
            KeyCode::Enter => {
                let entry = self
                    .browser
                    .as_ref()
                    .and_then(|b| b.current_entry().cloned());
                if let Some(entry) = entry {
                    if entry.is_container {
                        self.enter_container(entry)?;
                    } else {
                        self.ui_state
                            .set_status("Sélectionne un dossier pour le binding.");
                    }
                }
            }
            KeyCode::Char('s') | KeyCode::Char('b') => {
                let entry = self
                    .browser
                    .as_ref()
                    .and_then(|b| b.current_entry().cloned());
                if let Some(entry) = entry {
                    if entry.is_container {
                        self.pending_binding = Some(PendingBinding {
                            container_id: entry.id,
                            container_title: entry.title,
                        });
                        self.mode = Mode::BindingPrompt;
                        self.ui_state.set_status("Confirme le binding (y/n)");
                    } else {
                        self.ui_state
                            .set_status("Impossible de binder un item individuel.");
                    }
                }
            }
            KeyCode::Left | KeyCode::Backspace => {
                self.navigate_browser_up();
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.navigate_browser_up();
            }
            KeyCode::Char('h') => {
                self.show_help_overlay = !self.show_help_overlay;
            }
            KeyCode::Esc => {
                self.mode = Mode::SelectServer;
                self.status_line = "Sélectionne un serveur avec ↑/↓ puis Entrée".to_string();
                self.show_help_overlay = false;
                self.show_queue_overlay = false;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_binding_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') => {
                self.show_help_overlay = false;
                self.show_queue_overlay = false;
                self.attach_binding(true)?;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.show_help_overlay = false;
                self.show_queue_overlay = false;
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
            KeyCode::Char('B') => {
                if self.browser.is_some() {
                    self.mode = Mode::Browse;
                    self.status_line =
                            "Navigue avec ↑/↓, Entrée pour ouvrir, ←/Backspace ou r pour remonter, s pour sélectionner"
                                .to_string();
                } else {
                    self.ui_state
                        .set_status("Pas de navigateur actif. Reprends la sélection serveur.");
                }
            }
            KeyCode::Char('h') => {
                self.show_help_overlay = !self.show_help_overlay;
            }
            KeyCode::Char('k') => {
                self.show_queue_overlay = !self.show_queue_overlay;
            }
            KeyCode::Esc => {
                self.show_queue_overlay = false;
                self.show_help_overlay = false;
            }
            KeyCode::Char(' ') => {
                self.toggle_play_pause()?;
            }
            KeyCode::Char('p') => {
                self.pause_renderer()?;
            }
            KeyCode::Char('s') => {
                self.stop_renderer()?;
            }
            KeyCode::Char('n') => {
                self.play_next()?;
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => {
                self.volume_up()?;
            }
            KeyCode::Char('-') | KeyCode::Down => {
                self.volume_down()?;
            }
            KeyCode::Char('m') => {
                self.toggle_mute()?;
            }
            _ => {}
        }
        Ok(false)
    }

    fn load_servers(&mut self) {
        match self.client.list_servers() {
            Ok(list) => {
                self.servers = list;
                self.server_index = 0;
                if self.servers.is_empty() {
                    self.ui_state
                        .set_status("Aucun serveur disponible. Vérifie PMOMusic.");
                }
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Erreur REST serveurs: {err}"));
            }
        }
    }

    fn load_browser_for_server(&mut self, info: &MediaServerSummaryClient) -> Result<()> {
        let (root_id, entries) = self.fetch_root_entries(&info.id)?;
        self.browser = Some(BrowserState::new(
            info.id.clone(),
            root_id,
            entries,
            info.friendly_name.clone(),
        ));
        Ok(())
    }

    fn fetch_root_entries(&self, server_id: &str) -> Result<(String, Vec<ContainerEntryClient>)> {
        let mut last_err: Option<anyhow::Error> = None;
        for &candidate in ROOT_CONTAINERS {
            match self.client.browse_container(server_id, candidate) {
                Ok(resp) => return Ok((resp.container_id, resp.entries)),
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap_or_else(|| {
            anyhow!("Impossible de parcourir la racine pour le serveur {server_id}")
        }))
    }

    fn enter_container(&mut self, entry: ContainerEntryClient) -> Result<()> {
        let Some(browser) = self.browser.as_mut() else {
            return Ok(());
        };
        let container_id = entry.id.clone();
        let container_title = entry.title.clone();
        match self
            .client
            .browse_container(&browser.server_id, &container_id)
        {
            Ok(resp) => {
                browser
                    .nav_state
                    .enter_container(container_id, container_title);
                browser.entries = resp.entries;
                browser.selected_index = 0;
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Impossible d'ouvrir: {err}"));
            }
        }
        Ok(())
    }

    fn navigate_browser_up(&mut self) {
        let (server_id, container_id) = {
            let Some(browser) = self.browser.as_mut() else {
                return;
            };
            if browser.nav_state.go_back() {
                (
                    browser.server_id.clone(),
                    browser.nav_state.current_container_id.clone(),
                )
            } else {
                self.ui_state.set_status("Déjà à la racine.");
                return;
            }
        };
        match self.client.browse_container(&server_id, &container_id) {
            Ok(resp) => {
                if let Some(browser) = self.browser.as_mut() {
                    browser.entries = resp.entries;
                    browser.selected_index = 0;
                }
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Retour impossible: {err}"));
            }
        }
    }

    fn attach_binding(&mut self, attach: bool) -> Result<()> {
        if attach {
            if self.binding_worker.is_some() {
                self.ui_state
                    .set_status("Binding déjà en cours. Patiente quelques secondes...");
                return Ok(());
            }
            let Some(renderer) = self.selected_renderer.as_ref() else {
                self.ui_state.set_status("Choisis un renderer en premier.");
                return Ok(());
            };
            let Some(server) = self.selected_server.as_ref() else {
                self.ui_state.set_status("Choisis un serveur en premier.");
                return Ok(());
            };
            let Some(binding) = self.pending_binding.take() else {
                return Ok(());
            };
            let client = self.client.clone();
            let renderer_id = renderer.id.clone();
            let server_id = server.id.clone();
            let container_id = binding.container_id.clone();
            let container_title = binding.container_title.clone();
            let (tx, rx) = mpsc::channel();
            self.binding_worker = Some(rx);
            self.mode = Mode::Browse;
            self.status_line =
                "Binding en cours... patiente pendant la préparation de la playlist".to_string();
            self.show_help_overlay = false;
            self.show_queue_overlay = false;
            self.ui_state
                .set_status(format!("Association en cours: {}", container_title));
            thread::spawn(move || {
                let outcome = (|| -> Result<()> {
                    client.attach_playlist(&renderer_id, &server_id, &container_id)?;
                    // Préparer la lecture en sélectionnant le premier item
                    client.next(&renderer_id)?;
                    Ok(())
                })();
                let message = match outcome {
                    Ok(_) => BindingWorkerMessage::Success { container_title },
                    Err(err) => BindingWorkerMessage::Failure {
                        error: err.to_string(),
                    },
                };
                let _ = tx.send(message);
            });
        } else {
            self.pending_binding = None;
            self.mode = Mode::Browse;
            self.status_line =
                    "Navigue avec ↑/↓, Entrée pour ouvrir, ←/Backspace ou r pour remonter, s pour sélectionner".to_string();
            self.ui_state.set_status("Binding annulé");
        }
        Ok(())
    }

    fn toggle_play_pause(&mut self) -> Result<()> {
        let Some(renderer) = self.selected_renderer.as_ref() else {
            return Ok(());
        };
        let current = self
            .ui_state
            .transport_state
            .as_deref()
            .unwrap_or("")
            .to_string();
        if current.eq_ignore_ascii_case("PLAYING") {
            self.client.pause(&renderer.id)?;
            self.ui_state.set_status("Pause envoyée");
        } else {
            self.client.play(&renderer.id)?;
            self.ui_state.set_status("Lecture envoyée");
        }
        Ok(())
    }

    fn pause_renderer(&mut self) -> Result<()> {
        if let Some(renderer) = self.selected_renderer.as_ref() {
            self.client.pause(&renderer.id)?;
            self.ui_state.set_status("Pause envoyée");
        }
        Ok(())
    }

    fn stop_renderer(&mut self) -> Result<()> {
        if let Some(renderer) = self.selected_renderer.as_ref() {
            self.client.stop(&renderer.id)?;
            self.ui_state.set_status("Stop envoyé");
        }
        Ok(())
    }

    fn play_next(&mut self) -> Result<()> {
        if let Some(renderer) = self.selected_renderer.as_ref() {
            self.client.next(&renderer.id)?;
            self.ui_state.set_status("Piste suivante demandée");
        }
        Ok(())
    }

    fn volume_up(&mut self) -> Result<()> {
        if let Some(renderer) = self.selected_renderer.as_ref() {
            self.client.volume_up(&renderer.id)?;
            self.ui_state.set_status("Volume +");
        }
        Ok(())
    }

    fn volume_down(&mut self) -> Result<()> {
        if let Some(renderer) = self.selected_renderer.as_ref() {
            self.client.volume_down(&renderer.id)?;
            self.ui_state.set_status("Volume -");
        }
        Ok(())
    }

    fn toggle_mute(&mut self) -> Result<()> {
        if let Some(renderer) = self.selected_renderer.as_ref() {
            self.client.toggle_mute(&renderer.id)?;
            self.ui_state.set_status("Mute togglé");
        }
        Ok(())
    }

    fn open_renderer_menu(&mut self) {
        match self.client.list_renderers() {
            Ok(list) => {
                self.renderers = list;
                self.renderer_index = 0;
                self.mode = Mode::SelectRenderer;
                self.status_line = "Sélectionne un renderer avec ↑/↓ puis Entrée".to_string();
                self.show_help_overlay = false;
                self.show_queue_overlay = false;
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Impossible de rafraîchir les renderers: {err}"));
            }
        }
    }

    fn open_server_menu(&mut self) {
        if self.selected_renderer.is_none() {
            self.ui_state.set_status("Sélectionne d'abord un renderer.");
            return;
        }
        self.load_servers();
        self.mode = Mode::SelectServer;
        self.status_line = "Sélectionne un serveur avec ↑/↓ puis Entrée".to_string();
        self.show_help_overlay = false;
        self.show_queue_overlay = false;
    }

    fn refresh_renderer_state(&mut self) {
        let Some(renderer) = self.selected_renderer.as_ref() else {
            return;
        };
        match self.client.get_renderer_state(&renderer.id) {
            Ok(state) => {
                self.ui_state.transport_state = Some(state.transport_state.clone());
                self.ui_state.volume = state.volume;
                self.ui_state.mute = state.mute;
                self.ui_state.progress = Some(PlaybackProgress {
                    position_ms: state.position_ms,
                    duration_ms: state.duration_ms,
                });
                if let Some(info) = state.attached_playlist {
                    self.binding_info = Some(info);
                }
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Erreur REST renderer: {err}"));
            }
        }
    }

    fn refresh_queue(&mut self) {
        let Some(renderer) = self.selected_renderer.as_ref() else {
            return;
        };
        let current_signature = self.capture_current_queue_signature();
        match self.client.get_renderer_queue(&renderer.id) {
            Ok(snapshot) => {
                let next_items = snapshot.items;
                let mut next_index = snapshot.current_index;
                if !Self::is_valid_queue_index(next_index, &next_items) {
                    next_index = current_signature
                        .and_then(|sig| Self::find_queue_index_by_signature(&next_items, &sig));
                }
                self.queue_snapshot = next_items;
                self.queue_current_index = next_index;
                self.update_current_track_metadata();
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Erreur REST queue: {err}"));
            }
        }
    }

    fn capture_current_queue_signature(&self) -> Option<QueueItemSignature> {
        let idx = self.queue_current_index?;
        let item = self.queue_snapshot.get(idx)?;
        Some(QueueItemSignature::from_item(item))
    }

    fn is_valid_queue_index(index: Option<usize>, items: &[QueueItemClient]) -> bool {
        match index {
            Some(idx) => idx < items.len(),
            None => false,
        }
    }

    fn find_queue_index_by_signature(
        items: &[QueueItemClient],
        signature: &QueueItemSignature,
    ) -> Option<usize> {
        items.iter().enumerate().find_map(|(idx, item)| {
            if signature.matches(item) {
                Some(idx)
            } else {
                None
            }
        })
    }

    fn refresh_binding_info(&mut self) {
        let Some(renderer) = self.selected_renderer.as_ref() else {
            return;
        };
        match self.client.get_renderer_binding(&renderer.id) {
            Ok(binding) => {
                self.binding_info = binding;
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Erreur REST binding: {err}"));
            }
        }
    }

    fn update_current_track_metadata(&mut self) {
        if let Some(idx) = self.queue_current_index {
            if let Some(item) = self.queue_snapshot.get(idx) {
                self.ui_state.metadata = Some(TrackMetadata {
                    title: item.title.clone(),
                    artist: item.artist.clone(),
                    album: item.album.clone(),
                    album_art_uri: item.album_art_uri.clone(),
                });
                return;
            }
        }
        self.ui_state.metadata = None;
    }

    fn on_tick(&mut self) {
        self.poll_binding_worker();
        if self.mode != Mode::Control {
            return;
        }
        self.refresh_renderer_state();
        self.refresh_queue();
        self.refresh_binding_info();
    }

    fn poll_binding_worker(&mut self) {
        let Some(receiver) = self.binding_worker.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(BindingWorkerMessage::Success { container_title }) => {
                self.binding_worker = None;
                self.finish_binding_success(container_title);
            }
            Ok(BindingWorkerMessage::Failure { error }) => {
                self.binding_worker = None;
                self.finish_binding_failure(error);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.binding_worker = None;
                self.finish_binding_failure("Worker binding interrompu (canal fermé)".to_string());
            }
        }
    }

    fn finish_binding_success(&mut self, container_title: String) {
        self.mode = Mode::Control;
        self.status_line = "Espace=Play/Pause, n=Next, +/- volume, k=Queue, B=Browse".to_string();
        self.show_help_overlay = false;
        self.show_queue_overlay = false;
        self.ui_state
            .set_status(format!("Lecture lancée depuis {container_title}"));
        self.refresh_binding_info();
        self.refresh_queue();
        self.refresh_renderer_state();
    }

    fn finish_binding_failure(&mut self, error: String) {
        self.mode = Mode::Browse;
        self.status_line =
            "Navigue avec ↑/↓, Entrée pour ouvrir, ←/Backspace ou r pour remonter, s pour sélectionner".to_string();
        self.show_help_overlay = false;
        self.show_queue_overlay = false;
        self.ui_state
            .set_status(format!("Binding impossible: {error}"));
    }
}

impl UiState {
    fn new(renderer_name: String) -> Self {
        Self {
            renderer_name,
            server_name: None,
            transport_state: None,
            progress: None,
            volume: None,
            mute: None,
            metadata: None,
            last_status: Some("Interface initialisée.".to_string()),
        }
    }

    fn placeholder() -> Self {
        Self::new("<renderer non sélectionné>".to_string())
    }

    fn set_status<S: Into<String>>(&mut self, status: S) {
        self.last_status = Some(status.into());
    }
}

impl BrowserState {
    fn new(
        server_id: String,
        root_container_id: String,
        entries: Vec<ContainerEntryClient>,
        friendly_name: String,
    ) -> Self {
        Self {
            server_id,
            nav_state: NavigationState::new(root_container_id, friendly_name),
            entries,
            selected_index: 0,
        }
    }

    fn current_entry(&self) -> Option<&ContainerEntryClient> {
        self.entries.get(self.selected_index)
    }
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

fn run_app(mut app: App) -> Result<()> {
    let terminal = setup_terminal()?;
    let mut guard = TerminalGuard { terminal };
    let mut last_tick = Instant::now();

    let result = (|| -> Result<()> {
        loop {
            guard.terminal.draw(|f| app.draw(f))?;

            let timeout = TICK_RATE
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if app.handle_key(key)? {
                        break;
                    }
                }
            }

            if last_tick.elapsed() >= TICK_RATE {
                app.on_tick();
                last_tick = Instant::now();
            }
        }
        Ok(())
    })();

    // Restauration garantie via Drop de TerminalGuard
    // On tente un stop avec timeout court
    if let Some(renderer) = app.selected_renderer.as_ref() {
        // Utiliser un timeout très court pour ne pas bloquer le shutdown
        let _ = app.client.stop(&renderer.id);
    }

    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
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
        if let Some(art) = meta.album_art_uri.as_deref() {
            lines.push(Line::from(format!("Cover  : {art}")));
        }
    } else {
        lines.push(Line::from("(En attente des métadonnées...)"));
    }
    lines
}

fn build_progress_gauge(progress: &PlaybackProgress) -> Gauge<'static> {
    let ratio = match (progress.position_ms, progress.duration_ms) {
        (Some(pos), Some(dur)) if dur > 0 => (pos as f64 / dur as f64).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let label = format!(
        "{} / {}",
        progress
            .position_ms
            .and_then(format_time_ms)
            .unwrap_or_else(|| "--:--".to_string()),
        progress
            .duration_ms
            .and_then(format_time_ms)
            .unwrap_or_else(|| "--:--".to_string())
    );
    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progression"))
        .gauge_style(Style::default().fg(Color::Magenta))
        .ratio(ratio)
        .label(label)
}

fn format_time_ms(ms: u64) -> Option<String> {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        Some(format!("{hours:02}:{minutes:02}:{seconds:02}"))
    } else {
        Some(format!("{minutes:02}:{seconds:02}"))
    }
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

/// RAII guard pour garantir la restauration du terminal même en cas d'erreur ou de panic
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Force la restauration du terminal, même si les appels échouent
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

// ============================================================================
// REST DTOs
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct RendererSummaryClient {
    id: String,
    friendly_name: String,
    model_name: String,
    protocol: String,
    online: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct RendererStateClient {
    id: String,
    friendly_name: String,
    transport_state: String,
    position_ms: Option<u64>,
    duration_ms: Option<u64>,
    volume: Option<u8>,
    mute: Option<bool>,
    queue_len: usize,
    attached_playlist: Option<AttachedPlaylistInfoClient>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct AttachedPlaylistInfoClient {
    server_id: String,
    container_id: String,
    has_seen_update: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct QueueItemClient {
    index: usize,
    uri: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    server_id: Option<String>,
    object_id: Option<String>,
    album_art_uri: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct QueueSnapshotClient {
    renderer_id: String,
    #[serde(default, deserialize_with = "deserialize_nullable_vec")]
    items: Vec<QueueItemClient>,
    current_index: Option<usize>,
}

#[derive(Debug, Clone)]
struct QueueItemSignature {
    server_id: Option<String>,
    object_id: Option<String>,
    uri: String,
}

impl QueueItemSignature {
    fn from_item(item: &QueueItemClient) -> Self {
        Self {
            server_id: item.server_id.clone(),
            object_id: item.object_id.clone(),
            uri: item.uri.clone(),
        }
    }

    fn matches(&self, other: &QueueItemClient) -> bool {
        if let (Some(sig_obj), Some(other_obj)) = (&self.object_id, &other.object_id) {
            if sig_obj == other_obj {
                if let (Some(sig_server), Some(other_server)) = (&self.server_id, &other.server_id)
                {
                    return sig_server == other_server;
                }
                return true;
            }
        }
        self.uri == other.uri
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct MediaServerSummaryClient {
    id: String,
    friendly_name: String,
    model_name: String,
    online: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ContainerEntryClient {
    id: String,
    title: String,
    class: String,
    is_container: bool,
    child_count: Option<u32>,
    artist: Option<String>,
    album: Option<String>,
    album_art_uri: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct BrowseResponseClient {
    container_id: String,
    #[serde(default, deserialize_with = "deserialize_nullable_vec")]
    entries: Vec<ContainerEntryClient>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct VolumeSetRequest {
    volume: u8,
}

#[derive(Debug, Serialize)]
struct AttachPlaylistRequest<'a> {
    server_id: &'a str,
    container_id: &'a str,
}

fn deserialize_nullable_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let opt = Option::<Vec<T>>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

// ============================================================================
// REST CLIENT
// ============================================================================

struct RestClient {
    base_url: String,
    agent: Agent,
}

impl Clone for RestClient {
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            agent: self.agent.clone(),
        }
    }
}

impl RestClient {
    fn new(base_url: &str, timeout: Duration) -> Result<Self> {
        let mut builder = Agent::config_builder();
        builder = builder.timeout_global(Some(timeout));
        builder = builder.http_status_as_error(false);
        let config = builder.build();
        let agent: Agent = config.into();
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent,
        })
    }

    fn list_renderers(&self) -> Result<Vec<RendererSummaryClient>> {
        self.get_json(&["renderers"])
    }

    fn get_renderer_state(&self, id: &str) -> Result<RendererStateClient> {
        self.get_json(&["renderers", id])
    }

    fn get_renderer_queue(&self, id: &str) -> Result<QueueSnapshotClient> {
        self.get_json(&["renderers", id, "queue"])
    }

    fn get_renderer_binding(&self, id: &str) -> Result<Option<AttachedPlaylistInfoClient>> {
        self.get_json(&["renderers", id, "binding"])
    }

    fn play(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "play"])
    }

    fn pause(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "pause"])
    }

    fn stop(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "stop"])
    }

    fn next(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "next"])
    }

    #[allow(dead_code)]
    fn set_volume(&self, id: &str, volume: u8) -> Result<()> {
        let payload = VolumeSetRequest { volume };
        self.post_json(&["renderers", id, "volume", "set"], &payload)
    }

    fn volume_up(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "volume", "up"])
    }

    fn volume_down(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "volume", "down"])
    }

    fn toggle_mute(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "mute", "toggle"])
    }

    fn attach_playlist(&self, id: &str, server_id: &str, container_id: &str) -> Result<()> {
        let payload = AttachPlaylistRequest {
            server_id,
            container_id,
        };
        self.post_json(&["renderers", id, "binding", "attach"], &payload)
    }

    #[allow(dead_code)]
    fn detach_playlist(&self, id: &str) -> Result<()> {
        self.post_empty(&["renderers", id, "binding", "detach"])
    }

    fn list_servers(&self) -> Result<Vec<MediaServerSummaryClient>> {
        self.get_json(&["servers"])
    }

    fn browse_container(
        &self,
        server_id: &str,
        container_id: &str,
    ) -> Result<BrowseResponseClient> {
        self.get_json(&["servers", server_id, "containers", container_id])
    }

    fn get_json<T>(&self, segments: &[&str]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = self.build_url_segments(segments);
        let response = self.agent.get(&url).call();
        let mut response = Self::handle_response(response)?;
        let text = response
            .body_mut()
            .read_to_string()
            .with_context(|| format!("Échec de lecture JSON depuis {url}"))?;
        let value = serde_json::from_str(&text)
            .with_context(|| format!("Échec de parsing JSON depuis {url}"))?;
        Ok(value)
    }

    fn post_empty(&self, segments: &[&str]) -> Result<()> {
        let url = self.build_url_segments(segments);
        let response = self.agent.post(&url).send_empty();
        Self::handle_response(response)?;
        Ok(())
    }

    fn post_json<T>(&self, segments: &[&str], payload: &T) -> Result<()>
    where
        T: Serialize,
    {
        let url = self.build_url_segments(segments);
        let body = serde_json::to_vec(payload)?;
        let response = self
            .agent
            .post(&url)
            .header("content-type", "application/json")
            .send(body);
        Self::handle_response(response)?;
        Ok(())
    }

    fn build_url_segments(&self, segments: &[&str]) -> String {
        let mut url = self.base_url.clone();
        for segment in segments {
            url.push('/');
            url.push_str(&utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string());
        }
        url
    }

    fn handle_response(
        response: Result<http::Response<Body>, ureq::Error>,
    ) -> Result<http::Response<Body>> {
        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    let mut resp = resp;
                    let status = resp.status();
                    let body = resp
                        .body_mut()
                        .read_to_string()
                        .unwrap_or_else(|_| "<body illisible>".into());
                    Err(anyhow!("HTTP {}: {}", status, body))
                }
            }
            Err(err) => Err(anyhow!(err)),
        }
    }
}
