//! Full interactive control point CLI demo.
//!
//! This example demonstrates all capabilities of the ControlPoint:
//! - Interactive device selection (renderer + media server)
//! - ContentDirectory navigation with back/forward support
//! - Queue construction and playlist binding
//! - Full playback control (play/pause/stop/next/volume/mute)
//! - Real-time event monitoring (renderer and media server events)
//! - Live playlist observation and auto-refresh

use std::io::{self, BufRead, Write};
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use pmocontrol::{
    ControlPoint, DeviceRegistryRead, MediaBrowser, MediaEntry, MediaResource, MediaServerEvent,
    MediaServerInfo, MusicServer, PlaybackItem, PlaybackPosition, PlaybackStatus, RendererEvent,
    RendererInfo, TransportControl, VolumeControl,
};

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_DISCOVERY_SECS: u64 = 15;

fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    println!("=== Full Control Point Interactive Demo ===");
    println!("Starting control point with timeout={}s", DEFAULT_TIMEOUT_SECS);

    let control_point = ControlPoint::spawn(DEFAULT_TIMEOUT_SECS)
        .context("Failed to start control point")?;

    println!("Discovery running for {} seconds...", DEFAULT_DISCOVERY_SECS);
    thread::sleep(Duration::from_secs(DEFAULT_DISCOVERY_SECS));

    // Step 1: List and select renderer
    let registry = control_point.registry();
    let renderer_info = {
        let reg = registry.read().expect("registry poisoned");
        let renderers = reg.list_renderers();

        println!("\n=== Available Media Renderers ===");
        if renderers.is_empty() {
            eprintln!("No renderers discovered. Exiting.");
            process::exit(1);
        }

        print_renderers(&renderers);
        select_renderer(&renderers)?
    };

    println!("\n✓ Selected renderer: {} (id={})",
        renderer_info.friendly_name, renderer_info.id.0);

    // Step 2: List and select media server
    let server_info = {
        let reg = registry.read().expect("registry poisoned");
        let servers: Vec<MediaServerInfo> = reg.list_servers()
            .into_iter()
            .filter(|s| s.has_content_directory && s.content_directory_control_url.is_some())
            .collect();

        println!("\n=== Available Media Servers ===");
        if servers.is_empty() {
            eprintln!("No media servers with ContentDirectory discovered. Exiting.");
            process::exit(1);
        }

        print_servers(&servers);
        select_server(&servers)?
    };

    println!("\n✓ Selected server: {} (id={})",
        server_info.friendly_name, server_info.id.0);

    let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
    let server = MusicServer::from_info(&server_info, timeout)
        .context("Failed to initialize MusicServer")?;

    // Step 3: Navigate ContentDirectory and select a container for queue
    println!("\n=== ContentDirectory Navigation ===");
    println!("Commands: [number]=enter container, 'b'=back, 's'=select current as queue source, 'q'=quit navigation");

    let (selected_items, selected_container_id) = navigate_and_select(&server)?;

    if selected_items.is_empty() {
        println!("No playable items selected. Exiting.");
        process::exit(1);
    }

    println!("\n✓ Selected {} items for playback queue", selected_items.len());

    // Step 4: Build queue
    let renderer_id = renderer_info.id.clone();

    println!("\n=== Building Playback Queue ===");
    control_point.clear_queue(&renderer_id)
        .context("Failed to clear queue")?;
    control_point.enqueue_items(&renderer_id, selected_items)
        .context("Failed to enqueue items")?;

    println!("✓ Queue built with {} items",
        control_point.get_queue_snapshot(&renderer_id)?.len());

    // Step 5: Ask about playlist binding
    if let Some(container_id) = selected_container_id {
        println!("\nAttach this queue to playlist container '{}' for auto-refresh? (y/n)", container_id);
        if read_yes_no()? {
            control_point.attach_queue_to_playlist(
                &renderer_id,
                server_info.id.clone(),
                container_id.clone(),
            );
            println!("✓ Queue attached to playlist container '{}'", container_id);
        } else {
            println!("Queue will not be bound to playlist (no auto-refresh)");
        }
    }

    // Step 6: Start playback
    println!("\n=== Starting Playback ===");
    control_point.play_next_from_queue(&renderer_id)
        .context("Failed to start playback")?;
    println!("✓ Playback started");

    // Step 7: Spawn event monitoring threads
    let control_point_arc = Arc::new(control_point);
    spawn_renderer_event_thread(Arc::clone(&control_point_arc), renderer_id.clone());
    spawn_media_server_event_thread(Arc::clone(&control_point_arc), renderer_id.clone());

    // Step 8: Interactive control loop
    println!("\n=== Interactive Control ===");
    print_help();

    run_control_loop(Arc::clone(&control_point_arc), renderer_id)?;

    println!("\nExiting. Goodbye!");
    Ok(())
}

/// Print list of renderers with index.
fn print_renderers(renderers: &[RendererInfo]) {
    for (idx, info) in renderers.iter().enumerate() {
        println!("  [{}] {} | model={} | protocol={:?} | online={} | id={}",
            idx, info.friendly_name, info.model_name, info.protocol, info.online, info.id.0);
    }
}

/// Print list of servers with index.
fn print_servers(servers: &[MediaServerInfo]) {
    for (idx, info) in servers.iter().enumerate() {
        println!("  [{}] {} | model={} | manufacturer={} | online={} | id={}",
            idx, info.friendly_name, info.model_name, info.manufacturer, info.online, info.id.0);
    }
}

/// Interactive renderer selection.
fn select_renderer(renderers: &[RendererInfo]) -> Result<RendererInfo> {
    loop {
        print!("\nSelect renderer (index 0-{}): ", renderers.len() - 1);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().parse::<usize>() {
            Ok(idx) if idx < renderers.len() => {
                return Ok(renderers[idx].clone());
            }
            _ => {
                println!("Invalid selection. Please enter a number between 0 and {}", renderers.len() - 1);
            }
        }
    }
}

/// Interactive server selection.
fn select_server(servers: &[MediaServerInfo]) -> Result<MediaServerInfo> {
    loop {
        print!("\nSelect media server (index 0-{}): ", servers.len() - 1);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().parse::<usize>() {
            Ok(idx) if idx < servers.len() => {
                return Ok(servers[idx].clone());
            }
            _ => {
                println!("Invalid selection. Please enter a number between 0 and {}", servers.len() - 1);
            }
        }
    }
}

/// Navigation state for ContentDirectory browsing.
struct NavigationState {
    /// Stack of (container_id, container_title) for back navigation.
    path_stack: Vec<(String, String)>,
    /// Current container ID being browsed.
    current_container_id: String,
    /// Current container title.
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

/// Navigate ContentDirectory and let user select a container for queue.
fn navigate_and_select(server: &MusicServer) -> Result<(Vec<PlaybackItem>, Option<String>)> {
    let root_entries = server.browse_root()
        .context("Failed to browse root")?;

    let mut nav_state = NavigationState::new("0".to_string(), "Root".to_string());

    loop {
        println!("\n--- Browsing: {} (id: {}) ---",
            nav_state.current_container_title, nav_state.current_container_id);

        let entries = if nav_state.current_container_id == "0" {
            root_entries.clone()
        } else {
            server.browse_children(&nav_state.current_container_id, 0, 100)
                .context("Failed to browse container")?
        };

        if entries.is_empty() {
            println!("(empty container)");
        } else {
            print_entries(&entries);
        }

        print!("\nCommand [0-{} to enter container / b=back / s=select / q=quit]: ",
            entries.len().saturating_sub(1));
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        match input {
            "q" => {
                println!("Navigation cancelled.");
                return Ok((Vec::new(), None));
            }
            "b" => {
                if !nav_state.go_back() {
                    println!("Already at root.");
                }
            }
            "s" => {
                // Select current container
                println!("Collecting items from '{}'...", nav_state.current_container_title);
                let items = collect_playable_items(server, &entries)?;
                if items.is_empty() {
                    println!("No playable items found in this container.");
                    continue;
                }
                return Ok((items, Some(nav_state.current_container_id.clone())));
            }
            _ => {
                // Try to parse as index
                match input.parse::<usize>() {
                    Ok(idx) if idx < entries.len() => {
                        let entry = &entries[idx];
                        if entry.is_container {
                            nav_state.enter_container(entry.id.clone(), entry.title.clone());
                        } else {
                            println!("Entry {} is an audio item, not a container. Use 's' to select the current container for playback.", idx);
                        }
                    }
                    _ => {
                        println!("Invalid command. Enter a number (0-{}), 'b' to go back, 's' to select, or 'q' to quit.",
                            entries.len().saturating_sub(1));
                    }
                }
            }
        }
    }
}

/// Print MediaEntry list.
fn print_entries(entries: &[MediaEntry]) {
    for (idx, entry) in entries.iter().enumerate() {
        if entry.is_container {
            println!("  [{}] \u{1F4C1} {} (id: {}, class: {})",
                idx, entry.title, entry.id, entry.class);
        } else {
            let has_audio = entry.resources.iter().any(is_audio_resource);
            let icon = if has_audio { "\u{266B}" } else { "\u{1F4C4}" };
            println!("  [{}] {} {} (id: {}, class: {})",
                idx, icon, entry.title, entry.id, entry.class);
        }
    }
}

/// Collect playable items from MediaEntry list (including nested containers).
fn collect_playable_items(server: &MusicServer, entries: &[MediaEntry]) -> Result<Vec<PlaybackItem>> {
    let mut items = Vec::new();

    for entry in entries {
        if entry.is_container {
            // Recursively collect from container
            match server.browse_children(&entry.id, 0, 100) {
                Ok(children) => {
                    items.extend(collect_playable_items(server, &children)?);
                }
                Err(err) => {
                    eprintln!("Warning: failed to browse container '{}': {}", entry.title, err);
                }
            }
        } else {
            if let Some(item) = playback_item_from_entry(server, entry) {
                items.push(item);
            }
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

/// Read yes/no from stdin.
fn read_yes_no() -> Result<bool> {
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please enter 'y' or 'n'"),
        }
    }
}

/// Spawn thread to display renderer events in real-time.
fn spawn_renderer_event_thread(
    control_point: Arc<ControlPoint>,
    renderer_id: pmocontrol::model::RendererId,
) {
    let event_rx = control_point.subscribe_events();

    thread::spawn(move || {
        loop {
            match event_rx.recv() {
                Ok(event) => {
                    match event {
                        RendererEvent::StateChanged { id, state } => {
                            if id == renderer_id {
                                println!("\n[EVENT] Renderer state changed: {:?}", state);
                                print!("> ");
                                io::stdout().flush().ok();
                            }
                        }
                        RendererEvent::PositionChanged { id, position } => {
                            if id == renderer_id {
                                let rel = position.rel_time.as_deref().unwrap_or("-");
                                let dur = position.track_duration.as_deref().unwrap_or("-");
                                println!("\n[EVENT] Position: {} / {}", rel, dur);
                                print!("> ");
                                io::stdout().flush().ok();
                            }
                        }
                        RendererEvent::VolumeChanged { id, volume } => {
                            if id == renderer_id {
                                println!("\n[EVENT] Volume changed: {}", volume);
                                print!("> ");
                                io::stdout().flush().ok();
                            }
                        }
                        RendererEvent::MuteChanged { id, mute } => {
                            if id == renderer_id {
                                println!("\n[EVENT] Mute changed: {}", mute);
                                print!("> ");
                                io::stdout().flush().ok();
                            }
                        }
                        RendererEvent::MetadataChanged { id, metadata } => {
                            if id == renderer_id {
                                println!("\n[EVENT] Metadata changed:");
                                if let Some(title) = &metadata.title {
                                    println!("  Title: {}", title);
                                }
                                if let Some(artist) = &metadata.artist {
                                    println!("  Artist: {}", artist);
                                }
                                if let Some(album) = &metadata.album {
                                    println!("  Album: {}", album);
                                }
                                if let Some(genre) = &metadata.genre {
                                    println!("  Genre: {}", genre);
                                }
                                if let Some(date) = &metadata.date {
                                    println!("  Date: {}", date);
                                }
                                if let Some(track_number) = &metadata.track_number {
                                    println!("  Track: {}", track_number);
                                }
                                if let Some(album_art_uri) = &metadata.album_art_uri {
                                    println!("  Album Art: {}", album_art_uri);
                                }
                                print!("> ");
                                io::stdout().flush().ok();
                            }
                        }
                    }
                }
                Err(_) => {
                    eprintln!("\n[EVENT] Renderer event channel closed");
                    break;
                }
            }
        }
    });
}

/// Spawn thread to display media server events in real-time.
fn spawn_media_server_event_thread(
    control_point: Arc<ControlPoint>,
    renderer_id: pmocontrol::model::RendererId,
) {
    let media_event_rx = control_point.subscribe_media_server_events();

    thread::spawn(move || {
        loop {
            match media_event_rx.recv() {
                Ok(event) => {
                    match event {
                        MediaServerEvent::GlobalUpdated { server_id, system_update_id } => {
                            println!("\n[MEDIA EVENT] Server {} global update (SystemUpdateID={})",
                                server_id.0, system_update_id.unwrap_or(0));
                            print!("> ");
                            io::stdout().flush().ok();
                        }
                        MediaServerEvent::ContainersUpdated { server_id, container_ids } => {
                            println!("\n[MEDIA EVENT] Server {} containers updated: {:?}",
                                server_id.0, container_ids);

                            // Check if bound container was updated
                            if let Some((bound_server, bound_container, _)) =
                                control_point.current_queue_playlist_binding(&renderer_id)
                            {
                                if bound_server == server_id && container_ids.contains(&bound_container) {
                                    println!("[MEDIA EVENT] → Bound playlist '{}' was updated!", bound_container);
                                }
                            }

                            print!("> ");
                            io::stdout().flush().ok();
                        }
                    }
                }
                Err(_) => {
                    eprintln!("\n[MEDIA EVENT] Media server event channel closed");
                    break;
                }
            }
        }
    });
}

/// Print help message for control commands.
fn print_help() {
    println!("Available commands:");
    println!("  p       - Pause");
    println!("  r       - Resume/Play");
    println!("  s       - Stop");
    println!("  n       - Next track");
    println!("  +       - Volume +5");
    println!("  -       - Volume -5");
    println!("  m       - Toggle mute");
    println!("  i       - Show renderer info (state, position, volume)");
    println!("  k       - Show current queue");
    println!("  b       - Show playlist binding");
    println!("  h       - Show this help");
    println!("  q       - Quit gracefully");
    println!("  Q       - Quit immediately");
}

/// Main interactive control loop.
fn run_control_loop(
    control_point: Arc<ControlPoint>,
    renderer_id: pmocontrol::model::RendererId,
) -> Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        reader.read_line(&mut line)?;
        let cmd = line.trim();

        if cmd.is_empty() {
            continue;
        }

        match cmd {
            "q" => {
                println!("Quitting...");
                break;
            }
            "Q" => {
                println!("Quitting immediately!");
                process::exit(0);
            }
            "h" => {
                print_help();
            }
            "p" => {
                if let Err(err) = pause_renderer(&control_point, &renderer_id) {
                    eprintln!("Pause failed: {}", err);
                } else {
                    println!("✓ Paused");
                }
            }
            "r" => {
                if let Err(err) = resume_renderer(&control_point, &renderer_id) {
                    eprintln!("Resume failed: {}", err);
                } else {
                    println!("✓ Resumed");
                }
            }
            "s" => {
                if let Err(err) = stop_renderer(&control_point, &renderer_id) {
                    eprintln!("Stop failed: {}", err);
                } else {
                    println!("✓ Stopped");
                }
            }
            "n" => {
                if let Err(err) = control_point.play_next_from_queue(&renderer_id) {
                    eprintln!("Next track failed: {}", err);
                } else {
                    println!("✓ Playing next track");
                }
            }
            "+" => {
                if let Err(err) = adjust_volume(&control_point, &renderer_id, 5) {
                    eprintln!("Volume adjustment failed: {}", err);
                } else {
                    println!("✓ Volume +5");
                }
            }
            "-" => {
                if let Err(err) = adjust_volume(&control_point, &renderer_id, -5) {
                    eprintln!("Volume adjustment failed: {}", err);
                } else {
                    println!("✓ Volume -5");
                }
            }
            "m" => {
                if let Err(err) = toggle_mute(&control_point, &renderer_id) {
                    eprintln!("Mute toggle failed: {}", err);
                } else {
                    println!("✓ Mute toggled");
                }
            }
            "i" => {
                if let Err(err) = show_renderer_info(&control_point, &renderer_id) {
                    eprintln!("Failed to get renderer info: {}", err);
                }
            }
            "k" => {
                if let Err(err) = show_queue(&control_point, &renderer_id) {
                    eprintln!("Failed to get queue: {}", err);
                }
            }
            "b" => {
                show_binding(&control_point, &renderer_id);
            }
            _ => {
                println!("Unknown command '{}'. Type 'h' for help.", cmd);
            }
        }
    }

    Ok(())
}

/// Pause the renderer.
fn pause_renderer(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) -> Result<()> {
    let renderer = control_point.music_renderer_by_id(renderer_id)
        .ok_or_else(|| anyhow!("Renderer not found"))?;
    renderer.pause()?;
    Ok(())
}

/// Resume/play the renderer.
fn resume_renderer(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) -> Result<()> {
    let renderer = control_point.music_renderer_by_id(renderer_id)
        .ok_or_else(|| anyhow!("Renderer not found"))?;
    renderer.play()?;
    Ok(())
}

/// Stop the renderer.
fn stop_renderer(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) -> Result<()> {
    let renderer = control_point.music_renderer_by_id(renderer_id)
        .ok_or_else(|| anyhow!("Renderer not found"))?;
    renderer.stop()?;
    Ok(())
}

/// Adjust volume by delta (-100 to +100).
fn adjust_volume(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
    delta: i32,
) -> Result<()> {
    let renderer = control_point.music_renderer_by_id(renderer_id)
        .ok_or_else(|| anyhow!("Renderer not found"))?;

    let current = renderer.volume()?;
    let new_volume = (current as i32 + delta).clamp(0, 100) as u16;
    renderer.set_volume(new_volume)?;

    Ok(())
}

/// Toggle mute.
fn toggle_mute(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) -> Result<()> {
    let renderer = control_point.music_renderer_by_id(renderer_id)
        .ok_or_else(|| anyhow!("Renderer not found"))?;

    let current_mute = renderer.mute()?;
    renderer.set_mute(!current_mute)?;

    Ok(())
}

/// Show renderer info (state, position, volume, mute).
fn show_renderer_info(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) -> Result<()> {
    let renderer = control_point.music_renderer_by_id(renderer_id)
        .ok_or_else(|| anyhow!("Renderer not found"))?;

    println!("\n=== Renderer Info ===");
    println!("Name: {}", renderer.info().friendly_name);

    match renderer.playback_state() {
        Ok(state) => println!("State: {:?}", state),
        Err(err) => println!("State: <error: {}>", err),
    }

    match renderer.playback_position() {
        Ok(pos) => {
            let rel = pos.rel_time.as_deref().unwrap_or("-");
            let dur = pos.track_duration.as_deref().unwrap_or("-");
            println!("Position: {} / {}", rel, dur);
        }
        Err(err) => println!("Position: <error: {}>", err),
    }

    match renderer.volume() {
        Ok(vol) => println!("Volume: {}", vol),
        Err(err) => println!("Volume: <error: {}>", err),
    }

    match renderer.mute() {
        Ok(mute) => println!("Mute: {}", mute),
        Err(err) => println!("Mute: <error: {}>", err),
    }

    Ok(())
}

/// Show current queue.
fn show_queue(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) -> Result<()> {
    let queue = control_point.get_queue_snapshot(renderer_id)?;

    println!("\n=== Queue ({} items) ===", queue.len());
    if queue.is_empty() {
        println!("  <empty>");
    } else {
        for (idx, item) in queue.iter().enumerate() {
            let title = item.title.as_deref().unwrap_or("<no title>");
            let artist = item.artist.as_deref().unwrap_or("");
            let album = item.album.as_deref().unwrap_or("");

            if !artist.is_empty() && !album.is_empty() {
                println!("  [{}] {} - {} ({})", idx, artist, title, album);
            } else if !artist.is_empty() {
                println!("  [{}] {} - {}", idx, artist, title);
            } else {
                println!("  [{}] {}", idx, title);
            }
        }
    }

    Ok(())
}

/// Show playlist binding info.
fn show_binding(
    control_point: &ControlPoint,
    renderer_id: &pmocontrol::model::RendererId,
) {
    match control_point.current_queue_playlist_binding(renderer_id) {
        Some((server_id, container_id, has_seen_update)) => {
            println!("\n=== Playlist Binding ===");
            println!("Server ID: {}", server_id.0);
            println!("Container ID: {}", container_id);
            println!("Has seen update: {}", has_seen_update);
        }
        None => {
            println!("\n=== Playlist Binding ===");
            println!("  <no binding>");
        }
    }
}
