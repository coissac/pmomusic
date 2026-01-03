//! Live PMOMusic demo: binds a renderer queue to a dynamic "Live Playlist" container
//! and monitors ContentDirectory updates over an extended period (~30 minutes).

use std::collections::{HashSet, VecDeque};
use std::env;
use std::process;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use pmocontrol::model::TrackMetadata;
use pmocontrol::{
    ControlPoint, DeviceRegistryRead, MediaBrowser, MediaEntry, MediaServerEvent, UpnpMediaServer,
    MusicRendererBackend, UpnpMediaServer, PlaybackItem, PlaybackPosition, PlaybackPositionInfo, RendererInfo,
};

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_DISCOVERY_SECS: u64 = 5;
const DEFAULT_MAX_INITIAL_TRACKS: usize = 15;
const MONITOR_DURATION_SECS: u64 = 1800; // ~30 minutes
const MONITOR_POLL_SECS: u64 = 5;
const MAX_BROWSE_DEPTH: usize = 4;
const MAX_CONTAINERS_TO_EXPLORE: usize = 100;
const FALLBACK_MIN_TRACKS: usize = 5;

fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let config = CliConfig::parse_from_env().unwrap_or_else(|err| {
        eprintln!("Error parsing arguments: {err}");
        print_usage_and_exit();
    });

    println!(
        "Starting live_pmomusic_demo with timeout={}s discovery={}s max_initial_tracks={}",
        config.timeout_secs, config.discovery_secs, config.max_initial_tracks
    );

    // ControlPoint::spawn starts the HttpXmlDescriptionProvider + DiscoveryManager combo.
    let control_point =
        ControlPoint::spawn(config.timeout_secs).context("Failed to start control point")?;

    println!(
        "Discovery running for {} seconds before selecting devices...",
        config.discovery_secs
    );
    thread::sleep(Duration::from_secs(config.discovery_secs));

    let registry = control_point.registry();
    let (renderer, server_info) = {
        let reg = registry.read().expect("registry poisoned");
        let renderer_candidates: Vec<RendererInfo> = reg
            .list_renderers()
            .into_iter()
            .filter(|info| !is_pmomusic_renderer(info))
            .collect();
        let renderer = pick_renderer(renderer_candidates)
            .unwrap_or_else(|| no_renderer_and_exit("No suitable renderer found after discovery."));
        let server = pick_pmomusic_server(reg.list_servers())
            .unwrap_or_else(|| no_server_and_exit("No PMOMusic media server found."));
        (renderer, server)
    };

    println!(
        "Selected renderer \"{}\" (protocol={:?}, id={})",
        renderer.friendly_name, renderer.protocol, renderer.id.0
    );
    println!(
        "Selected media server \"{}\" at {} (id={})",
        server_info.friendly_name, server_info.location, server_info.id.0
    );

    let renderer_instance = MusicRendererBackend::from_renderer_info(renderer.clone(), &registry)
        .expect("Selected renderer is not usable by MusicRenderer façade");
    let supports_set_next = renderer_instance
        .as_upnp()
        .map(|upnp| upnp.supports_set_next())
        .unwrap_or(false);
    println!(
        "Renderer \"{}\": AVTransport present = {}, SetNextAVTransportURI supported = {}",
        renderer.friendly_name, renderer.capabilities.has_avtransport, supports_set_next
    );

    let timeout = Duration::from_secs(config.timeout_secs);
    let server =
        UpnpMediaServer::from_info(&server_info, timeout).context("Failed to init MusicServer")?;

    println!("Searching for a Live Playlist container in ContentDirectory...");
    let live_playlist_container = find_live_playlist_container(&server)
        .context("Failed to search for Live Playlist container")?;

    let live_playlist_container = match live_playlist_container {
        Some(container) => container,
        None => {
            println!(
                "No Live Playlist container found on server \"{}\". Exiting.",
                server_info.friendly_name
            );
            process::exit(1);
        }
    };

    println!(
        "✓ Found Live Playlist: '{}' (id: {}, class: {})",
        live_playlist_container.title, live_playlist_container.id, live_playlist_container.class
    );

    // Build initial queue from the live playlist container
    let playback_items = collect_playable_items_from_container(
        &server,
        &live_playlist_container.id,
        config.max_initial_tracks,
    )
    .context("Failed to collect playable items from Live Playlist container")?;

    if playback_items.is_empty() {
        println!(
            "Live Playlist container '{}' contains no playable tracks.",
            live_playlist_container.title
        );
        process::exit(1);
    }

    println!(
        "Discovered {} playable items from Live Playlist; enqueuing…",
        playback_items.len()
    );

    let renderer_id = renderer.id.clone();
    control_point
        .clear_queue(&renderer_id)
        .context("Failed to clear playback queue")?;
    control_point
        .enqueue_items(&renderer_id, playback_items)
        .context("Failed to enqueue playback items")?;

    // Attach queue to live playlist container
    control_point
        .attach_queue_to_playlist(
            &renderer_id,
            server_info.id.clone(),
            live_playlist_container.id.clone(),
        )
        .context("Failed to attach queue to live playlist container")?;
    println!(
        "✓ Queue attached to Live Playlist container '{}' (id: {}) on server '{}'",
        live_playlist_container.title, live_playlist_container.id, server_info.friendly_name
    );

    let snapshot = control_point
        .get_queue_snapshot(&renderer_id)
        .context("Failed to snapshot queue after enqueue")?;
    print_queue_snapshot(&snapshot);
    let mut planned_queue: VecDeque<PlaybackItem> = snapshot.clone().into();

    control_point
        .play_next_from_queue(&renderer_id)
        .context("Failed to start playback from queue")?;
    let mut current_track = planned_queue.pop_front();
    let remaining = control_point
        .get_queue_snapshot(&renderer_id)
        .context("Failed to snapshot queue after play_next_from_queue")?;
    planned_queue = remaining.clone().into();
    println!(
        "Playback started on \"{}\"; {} tracks remaining in queue.",
        renderer.friendly_name,
        remaining.len()
    );

    println!(
        "Monitoring Live Playlist queue for {} seconds (poll every {}s)…",
        MONITOR_DURATION_SECS, MONITOR_POLL_SECS
    );
    println!("This will observe ContentDirectory updates and auto-advance behavior.");

    // Subscribe to media server events to observe playlist updates
    let media_event_rx = control_point.subscribe_media_server_events();

    let poll_count = MONITOR_DURATION_SECS / MONITOR_POLL_SECS;
    for tick in 0..poll_count {
        thread::sleep(Duration::from_secs(MONITOR_POLL_SECS));

        // Drain any MediaServerEvent that arrived since last poll
        loop {
            match media_event_rx.try_recv() {
                Ok(MediaServerEvent::GlobalUpdated {
                    server_id,
                    system_update_id,
                }) => {
                    println!(
                        "  📢 MediaServer {} global update (SystemUpdateID={:?})",
                        server_id.0, system_update_id
                    );
                }
                Ok(MediaServerEvent::ContainersUpdated {
                    server_id,
                    container_ids,
                }) => {
                    println!(
                        "  📢 MediaServer {} containers updated: {:?}",
                        server_id.0, container_ids
                    );

                    // Check if our bound container was updated
                    if let Some((bound_server, bound_container, _)) =
                        control_point.current_queue_playlist_binding(&renderer_id)
                    {
                        if bound_server == server_id && container_ids.contains(&bound_container) {
                            println!(
                                "  🔄 Bound Live Playlist container '{}' was updated, queue refresh triggered automatically",
                                bound_container
                            );
                            // Take a fresh snapshot to observe changes
                            if let Ok(fresh_snapshot) =
                                control_point.get_queue_snapshot(&renderer_id)
                            {
                                println!(
                                    "  → Queue length after refresh: {} items",
                                    fresh_snapshot.len()
                                );
                                if let Some(first) = fresh_snapshot.first() {
                                    let label = first
                                        .metadata
                                        .as_ref()
                                        .and_then(|meta| meta.title.as_deref())
                                        .unwrap_or("<no title>");
                                    println!("  → First item: {label}");
                                }
                            }
                        }
                    }
                }
                Err(_) => break, // No more events, continue with normal monitoring
            }
        }

        let snapshot = control_point
            .get_queue_snapshot(&renderer_id)
            .context("Queue snapshot failed during monitoring loop")?;
        let new_plan: VecDeque<PlaybackItem> = snapshot.clone().into();

        // Detect queue changes (auto-advance)
        if planned_queue.len() > new_plan.len() {
            let removed = planned_queue.len() - new_plan.len();
            for _ in 0..removed {
                current_track = planned_queue.pop_front();
            }
        }
        planned_queue = new_plan;

        let playback_info = control_point
            .music_renderer_by_id(&renderer_id)
            .and_then(|renderer| renderer.playback_position().ok());

        let title = current_track_title(current_track.as_ref());
        if let Some(info) = playback_info {
            println!(
                "[tick {tick}] Queue length = {} | now playing: {} [{}]",
                snapshot.len(),
                title,
                format_playback_position(&info)
            );
        } else {
            println!(
                "[tick {tick}] Queue length = {} | now playing: {} [position unavailable]",
                snapshot.len(),
                title
            );
        }
    }

    println!(
        "Monitoring finished ({}s elapsed), exiting.",
        MONITOR_DURATION_SECS
    );
    Ok(())
}

#[derive(Debug)]
struct CliConfig {
    timeout_secs: u64,
    discovery_secs: u64,
    max_initial_tracks: usize,
}

impl CliConfig {
    fn parse_from_env() -> Result<Self, String> {
        let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
        let mut discovery_secs = DEFAULT_DISCOVERY_SECS;
        let mut max_initial_tracks = DEFAULT_MAX_INITIAL_TRACKS;

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--timeout-secs" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--timeout-secs requires a value".to_string())?;
                    timeout_secs = value.parse().map_err(|err| {
                        format!("Invalid value for --timeout-secs ({value}): {err}")
                    })?;
                }
                "--discovery-secs" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--discovery-secs requires a value".to_string())?;
                    discovery_secs = value.parse().map_err(|err| {
                        format!("Invalid value for --discovery-secs ({value}): {err}")
                    })?;
                }
                "--max-initial-tracks" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--max-initial-tracks requires a value".to_string())?;
                    max_initial_tracks = value.parse().map_err(|err| {
                        format!("Invalid value for --max-initial-tracks ({value}): {err}")
                    })?;
                }
                "--help" | "-h" => {
                    print_usage_and_exit();
                }
                unknown => {
                    return Err(format!("Unknown argument: {unknown}"));
                }
            }
        }

        Ok(Self {
            timeout_secs,
            discovery_secs,
            max_initial_tracks,
        })
    }
}

fn pick_renderer(renderers: Vec<RendererInfo>) -> Option<RendererInfo> {
    let mut candidates: Vec<RendererInfo> = renderers;

    if candidates.is_empty() {
        return None;
    }

    println!("Renderer candidates:");
    for (idx, info) in candidates.iter().enumerate() {
        println!(
            "  [{}] {} | model={} | location={} | online={}",
            idx, info.friendly_name, info.model_name, info.location, info.online
        );
    }

    let selected = candidates.remove(0);
    println!(
        "Automatically selecting renderer index 0: {}",
        selected.friendly_name
    );
    Some(selected)
}

fn pick_pmomusic_server(servers: Vec<UpnpMediaServer>) -> Option<UpnpMediaServer> {
    let mut candidates: Vec<UpnpMediaServer> = servers
        .into_iter()
        .filter(|info| info.has_content_directory)
        .filter(|info| info.content_directory_control_url.is_some())
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Prioritize PMOMusic servers
    if let Some(idx) = candidates.iter().position(is_pmomusic_server) {
        let server = candidates.remove(idx);
        println!(
            "✓ Selected PMOMusic server \"{}\" (model: {}, manufacturer: {}).",
            server.friendly_name, server.model_name, server.manufacturer
        );
        return Some(server);
    }

    // Fallback to first ContentDirectory server if no PMOMusic found
    println!("No PMOMusic server discovered; falling back to first ContentDirectory server.");
    Some(candidates.remove(0))
}

fn is_pmomusic_server(info: &UpnpMediaServer) -> bool {
    let name = info.friendly_name.to_ascii_lowercase();
    let model = info.model_name.to_ascii_lowercase();
    let manufacturer = info.manufacturer.to_ascii_lowercase();
    name.contains("pmomusic") || model.contains("pmomusic") || manufacturer.contains("pmomusic")
}

fn is_pmomusic_renderer(info: &RendererInfo) -> bool {
    info.friendly_name
        .to_ascii_lowercase()
        .contains("pmomusic audio renderer")
}

/// Search for a container whose title contains "Live Playlist" using BFS.
fn find_live_playlist_container(server: &UpnpMediaServer) -> Result<Option<MediaEntry>> {
    let root_entries = server
        .browse_root()
        .context("Failed to browse ContentDirectory root")?;

    println!(
        "Root returned {} entries, starting BFS search for Live Playlist...",
        root_entries.len()
    );

    // BFS queue: (entry, depth)
    let mut queue: VecDeque<(MediaEntry, usize)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut containers_explored = 0;

    // Initialize with root entries
    for entry in root_entries {
        if entry.is_container {
            queue.push_back((entry, 0));
        }
    }

    while let Some((container, depth)) = queue.pop_front() {
        // Check exploration limits
        if depth > MAX_BROWSE_DEPTH {
            continue;
        }
        if containers_explored >= MAX_CONTAINERS_TO_EXPLORE {
            println!(
                "Reached max containers to explore ({}), stopping search.",
                MAX_CONTAINERS_TO_EXPLORE
            );
            break;
        }

        // Skip already visited
        if visited.contains(&container.id) {
            continue;
        }
        visited.insert(container.id.clone());
        containers_explored += 1;

        let title_lower = container.title.to_ascii_lowercase();

        // Check if this container is a Live Playlist
        if title_lower.contains("live playlist") {
            println!(
                "✓ Found Live Playlist at depth {}: '{}' (id: {})",
                depth, container.title, container.id
            );
            return Ok(Some(container));
        }

        // Browse children and add containers to queue
        match server.browse_children(&container.id, 0, 100) {
            Ok(children) => {
                for child in children {
                    if child.is_container && !visited.contains(&child.id) {
                        queue.push_back((child, depth + 1));
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    container_id = container.id.as_str(),
                    error = %err,
                    "Failed to browse container during Live Playlist search"
                );
            }
        }
    }

    println!(
        "No Live Playlist container found after exploring {} containers.",
        containers_explored
    );
    Ok(None)
}

/// Collect playable items from a specific container.
/// Retries with fewer items if the initial browse times out.
fn collect_playable_items_from_container(
    server: &UpnpMediaServer,
    container_id: &str,
    max_tracks: usize,
) -> Result<Vec<PlaybackItem>> {
    println!(
        "Attempting to browse Live Playlist container (requesting {} items)...",
        max_tracks
    );

    // First attempt with requested count
    let children = match server.browse_children(container_id, 0, max_tracks as u32) {
        Ok(children) => children,
        Err(err) => {
            let err_str = err.to_string().to_lowercase();
            if err_str.contains("timeout") && max_tracks > FALLBACK_MIN_TRACKS {
                println!(
                    "Browse timed out, retrying with fewer items ({})...",
                    FALLBACK_MIN_TRACKS
                );
                // Fallback: try with minimal number of items
                server
                    .browse_children(container_id, 0, FALLBACK_MIN_TRACKS as u32)
                    .context(
                        "Failed to browse Live Playlist container even with minimal item count",
                    )?
            } else {
                return Err(err).context("Failed to browse Live Playlist container children");
            }
        }
    };

    println!(
        "Browse returned {} entries from Live Playlist",
        children.len()
    );

    let mut items = Vec::new();
    for entry in &children {
        if !entry.is_container {
            if let Some(item) = playback_item_from_entry(server, entry) {
                items.push(item);
                if items.len() >= max_tracks {
                    break;
                }
            }
        }
    }

    println!(
        "Extracted {} playable items from Live Playlist",
        items.len()
    );
    Ok(items)
}

fn playback_item_from_entry(server: &UpnpMediaServer, entry: &MediaEntry) -> Option<PlaybackItem> {
    // Skip live streams (we're looking for regular tracks in a live playlist)
    if entry.title.to_ascii_lowercase().contains("live stream") {
        return None;
    }
    let resource = entry.resources.iter().find(|res| res.is_audio())?;
    let metadata = TrackMetadata {
        title: Some(entry.title.clone()),
        artist: entry.artist.clone(),
        album: entry.album.clone(),
        genre: entry.genre.clone(),
        album_art_uri: entry.album_art_uri.clone(),
        date: entry.date.clone(),
        track_number: entry.track_number.clone(),
        creator: entry.creator.clone(),
    };
    Some(PlaybackItem {
        media_server_id: server.id().clone(),
        didl_id: entry.id.clone(),
        uri: resource.uri.clone(),
        protocol_info: resource.protocol_info.clone(),
        metadata: Some(metadata),
    })
}

fn print_queue_snapshot(items: &[PlaybackItem]) {
    println!("Current queue snapshot ({} items):", items.len());
    for (idx, item) in items.iter().take(10).enumerate() {
        let label = item
            .metadata
            .as_ref()
            .and_then(|meta| meta.title.as_deref())
            .unwrap_or_else(|| item.uri.as_str());
        println!("  [{}] {}", idx, label);
    }
    if items.len() > 10 {
        println!("  ... and {} more items", items.len() - 10);
    }
    if items.is_empty() {
        println!("  <queue is empty>");
    }
}

fn current_track_title(item: Option<&PlaybackItem>) -> String {
    match item {
        Some(track) => track
            .metadata
            .as_ref()
            .and_then(|meta| meta.title.as_deref())
            .unwrap_or_else(|| track.uri.as_str())
            .to_string(),
        None => "<unknown>".to_string(),
    }
}

fn format_playback_position(info: &PlaybackPositionInfo) -> String {
    let rel = info.rel_time.as_deref().unwrap_or("-");
    let dur = info.track_duration.as_deref().unwrap_or("-");
    format!("{rel} / {dur}")
}

fn no_renderer_and_exit(message: &str) -> ! {
    println!("{message}");
    process::exit(1);
}

fn no_server_and_exit(message: &str) -> ! {
    println!("{message}");
    process::exit(1);
}

fn print_usage_and_exit() -> ! {
    println!(
        "Usage: cargo run -p pmocontrol --example live_pmomusic_demo -- [--timeout-secs N] [--discovery-secs N] [--max-initial-tracks N]"
    );
    process::exit(1);
}
