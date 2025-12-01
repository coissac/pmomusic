//! End-to-end queue demo that prefers the PMOMusic media server and exercises
//! the ControlPoint playback queue API.

use std::collections::VecDeque;
use std::env;
use std::process;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use pmocontrol::{
    ControlPoint, DeviceRegistryRead, MediaBrowser, MediaEntry, MediaResource, MediaServerInfo,
    MusicRenderer, MusicServer, PlaybackItem, PlaybackPosition, PlaybackPositionInfo, RendererInfo,
    RendererProtocol,
};

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_DISCOVERY_SECS: u64 = 5;
const DEFAULT_MAX_TRACKS: usize = 3;
const MONITOR_DURATION_SECS: u64 = 600;
const MONITOR_POLL_SECS: u64 = 5;
const MAX_BROWSE_DEPTH: usize = 2;

fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let config = CliConfig::parse_from_env().unwrap_or_else(|err| {
        eprintln!("Error parsing arguments: {err}");
        print_usage_and_exit();
    });

    if config.max_tracks == 0 {
        eprintln!("--max-tracks must be >= 1");
        process::exit(1);
    }

    println!(
        "Starting queue_pmomusic_demo with timeout={}s discovery={}s max_tracks={}",
        config.timeout_secs, config.discovery_secs, config.max_tracks
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
        let server = pick_media_server(reg.list_servers())
            .unwrap_or_else(|| no_server_and_exit("No media server with ContentDirectory."));
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

    let renderer_instance = MusicRenderer::from_registry_info(renderer.clone(), &registry)
        .expect("Selected renderer is not usable by MusicRenderer façade");
    let supports_set_next = renderer_instance
        .as_upnp()
        .map(|upnp| upnp.supports_set_next())
        .unwrap_or(false);
    println!(
        "Renderer \"{}\": AVTransport present = {}, SetNextAVTransportURI supported = {}",
        renderer.friendly_name,
        renderer.capabilities.has_avtransport,
        supports_set_next
    );

    let timeout = Duration::from_secs(config.timeout_secs);
    let server =
        MusicServer::from_info(&server_info, timeout).context("Failed to init MusicServer")?;

    let root_entries = server
        .browse_root()
        .context("Failed to browse ContentDirectory root")?;
    println!("Root returned {} entries", root_entries.len());

    let playback_items = collect_playable_items(&server, &root_entries, config.max_tracks)
        .context("Failed to derive playable items from ContentDirectory root/children")?;

    if playback_items.is_empty() {
        println!("No playable tracks were found on the selected server.");
        process::exit(1);
    }

    println!(
        "Discovered {} playable items; enqueuing…",
        playback_items.len()
    );

    let mut planned_queue: VecDeque<PlaybackItem>;
    let renderer_id = renderer.id.clone();
    control_point
        .clear_queue(&renderer_id)
        .context("Failed to clear playback queue")?;
    control_point
        .enqueue_items(&renderer_id, playback_items)
        .context("Failed to enqueue playback items")?;

    let snapshot = control_point
        .get_queue_snapshot(&renderer_id)
        .context("Failed to snapshot queue after enqueue")?;
    print_queue_snapshot(&snapshot);
    planned_queue = snapshot.clone().into();

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
        "Monitoring queue auto-advance for {} seconds (poll every {}s)…",
        MONITOR_DURATION_SECS, MONITOR_POLL_SECS
    );
    let poll_count = MONITOR_DURATION_SECS / MONITOR_POLL_SECS;
    for tick in 0..poll_count {
        thread::sleep(Duration::from_secs(MONITOR_POLL_SECS));
        let snapshot = control_point
            .get_queue_snapshot(&renderer_id)
            .context("Queue snapshot failed during monitoring loop")?;
        let new_plan: VecDeque<PlaybackItem> = snapshot.clone().into();
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

    println!("Monitoring finished, exiting.");
    Ok(())
}

#[derive(Debug)]
struct CliConfig {
    timeout_secs: u64,
    discovery_secs: u64,
    max_tracks: usize,
}

impl CliConfig {
    fn parse_from_env() -> Result<Self, String> {
        let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
        let mut discovery_secs = DEFAULT_DISCOVERY_SECS;
        let mut max_tracks = DEFAULT_MAX_TRACKS;

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
                "--max-tracks" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--max-tracks requires a value".to_string())?;
                    max_tracks = value.parse().map_err(|err| {
                        format!("Invalid value for --max-tracks ({value}): {err}")
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
            max_tracks,
        })
    }
}

fn pick_renderer(renderers: Vec<RendererInfo>) -> Option<RendererInfo> {
    let mut candidates: Vec<RendererInfo> = renderers
        .into_iter()
        .filter(|info| match info.protocol {
            RendererProtocol::OpenHomeOnly => false,
            RendererProtocol::UpnpAvOnly | RendererProtocol::Hybrid => true,
        })
        .collect();

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

fn pick_media_server(servers: Vec<MediaServerInfo>) -> Option<MediaServerInfo> {
    let mut candidates: Vec<MediaServerInfo> = servers
        .into_iter()
        .filter(|info| info.has_content_directory)
        .filter(|info| info.content_directory_control_url.is_some())
        .collect();

    if candidates.is_empty() {
        return None;
    }

    if let Some(idx) = candidates.iter().position(is_pmomusic_server) {
        let server = candidates.remove(idx);
        println!(
            "Preferring PMOMusic server \"{}\" (server header: {}).",
            server.friendly_name, server.server_header
        );
        Some(server)
    } else {
        println!("No PMOMusic server discovered; falling back to first ContentDirectory server.");
        Some(candidates.remove(0))
    }
}

fn is_pmomusic_server(info: &MediaServerInfo) -> bool {
    let name = info.friendly_name.to_ascii_lowercase();
    let header = info.server_header.to_ascii_lowercase();
    name.contains("pmomusic") || header.contains("pmomusic")
}

fn is_pmomusic_renderer(info: &RendererInfo) -> bool {
    info.friendly_name
        .to_ascii_lowercase()
        .contains("pmomusic audio renderer")
}

fn collect_playable_items(
    server: &MusicServer,
    entries: &[MediaEntry],
    max_tracks: usize,
) -> Result<Vec<PlaybackItem>> {
    let mut items = Vec::new();
    for entry in entries {
        gather_items_from_entry(server, entry, max_tracks, 0, &mut items)?;
        if items.len() >= max_tracks {
            break;
        }
    }
    Ok(items)
}

fn gather_items_from_entry(
    server: &MusicServer,
    entry: &MediaEntry,
    max_tracks: usize,
    depth: usize,
    out: &mut Vec<PlaybackItem>,
) -> Result<()> {
    if out.len() >= max_tracks {
        return Ok(());
    }

    if entry.is_container {
        if depth >= MAX_BROWSE_DEPTH {
            return Ok(());
        }

        match server.browse_children(&entry.id, 0, 50) {
            Ok(children) => {
                for child in children {
                    gather_items_from_entry(server, &child, max_tracks, depth + 1, out)?;
                    if out.len() >= max_tracks {
                        break;
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    container_id = entry.id.as_str(),
                    error = %err,
                    "Failed to browse child container"
                );
            }
        }
        return Ok(());
    }

    if let Some(item) = playback_item_from_entry(server, entry) {
        out.push(item);
    }
    Ok(())
}

fn playback_item_from_entry(server: &MusicServer, entry: &MediaEntry) -> Option<PlaybackItem> {
    if entry.title.to_ascii_lowercase().contains("live stream") {
        return None;
    }
    let resource = entry.resources.iter().find(|res| is_audio_resource(res))?;
    let mut item = PlaybackItem::new(resource.uri.clone());
    item.title = Some(entry.title.clone());
    item.server_id = Some(server.id().clone());
    item.object_id = Some(entry.id.clone());
    Some(item)
}

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

fn print_queue_snapshot(items: &[PlaybackItem]) {
    println!("Current queue snapshot ({} items):", items.len());
    for (idx, item) in items.iter().enumerate() {
        let label = item.title.as_deref().unwrap_or_else(|| item.uri.as_str());
        println!("  [{}] {} -> {}", idx, label, item.uri);
    }
    if items.is_empty() {
        println!("  <queue is empty>");
    }
}

fn current_track_title(item: Option<&PlaybackItem>) -> String {
    match item {
        Some(track) => track
            .title
            .as_deref()
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
        "Usage: cargo run -p pmocontrol --example queue_pmomusic_demo -- [--timeout-secs N] [--discovery-secs N] [--max-tracks N]"
    );
    process::exit(1);
}
