// examples/music_renderer_demo.rs
//
// End-to-end demo using the `MusicRenderer` façade:
//  - SSDP discovery via `ControlPoint`
//  - selection of a renderer
//  - generic `TransportControl` + `VolumeControl` + `PlaybackStatus` + `PlaybackPosition`
//  - optional UPnP-specific inspection (TransportInfo, ConnectionManager)
//
// Build and run (from pmocontrol crate root):
//   cargo run --example music_renderer_demo -- [index] [uri]
//
//   index: optional 0-based renderer index (default: 0)
//   uri  : optional URI to play (default: Radio Paradise FLAC)

use anyhow::Result;
use pmocontrol::PlaybackPosition;
use pmocontrol::{
    ControlPoint, MusicRenderer, PlaybackState, PlaybackStatus, RendererCapabilities,
    RendererProtocol, TransportControl, VolumeControl,
};
use std::env;
use std::thread;
use std::time::Duration;

// Default URI if none is provided on the CLI.
const DEFAULT_TEST_URI: &str = "https://audio-fb.radioparadise.com/chan/1/x/1117/4/g/1117-3.flac";

// Extra wait after play_uri() so slow renderers (e.g. Arylic H50) have time
// to prefetch and actually start playback.
const AFTER_PLAY_WAIT_SECS: u64 = 15;

fn main() -> Result<()> {
    // 1. Start control point and let discovery run a bit
    let cp = ControlPoint::spawn(5)?;
    thread::sleep(Duration::from_secs(5));

    // 2. Snapshot of logical music renderers
    let mut renderers: Vec<MusicRenderer> = cp.list_music_renderers();

    // Filter out the in-dev PMOMusic renderer (if present)
    renderers.retain(|r| {
        let name = r.friendly_name().to_ascii_lowercase();
        !name.contains("pmomusic audio renderer")
    });

    if renderers.is_empty() {
        println!("No valid music renderer discovered.");
        return Ok(());
    }

    // 3. CLI args: [index] [uri]
    let args: Vec<String> = env::args().skip(1).collect();

    let selected_index: usize = if !args.is_empty() {
        args[0].parse().unwrap_or(0)
    } else {
        0
    };

    let maybe_uri: Option<String> = if args.len() >= 2 {
        Some(args[1].clone())
    } else {
        None
    };

    // Bounds check
    if selected_index >= renderers.len() {
        println!(
            "Renderer index {} out of range ({} available).",
            selected_index,
            renderers.len()
        );
        return Ok(());
    }

    // 4. List renderers with basic info
    println!("Discovered MusicRenderers:");
    for (idx, r) in renderers.iter().enumerate() {
        let info = r.info();
        println!(
            "  [{}] {} | model={} | udn={} | location={}",
            idx, info.friendly_name, info.model_name, info.udn, info.location
        );
        print_backend("      ", r);
        print_capabilities("      ", &info.capabilities, &info.protocol);
    }

    // 5. Select renderer
    let renderer = &renderers[selected_index];
    let info = renderer.info();

    println!("\nSelected renderer (index {}):", selected_index);
    println!("  Name        : {}", info.friendly_name);
    println!("  Model       : {}", info.model_name);
    println!("  Manufacturer: {}", info.manufacturer);
    println!("  UDN         : {}", info.udn);
    println!("  Location    : {}", info.location);
    println!("  Protocol    : {:?}", info.protocol);
    print_backend("  ", renderer);
    print_capabilities("  ", &info.capabilities, &info.protocol);

    if let Some(upnp) = renderer.as_upnp() {
        println!(
            "  [UPnP] AVTransport control URL : {}",
            upnp.info
                .avtransport_control_url
                .as_deref()
                .unwrap_or("<none>")
        );
        println!(
            "  [UPnP] AVTransport service type: {}",
            upnp.info
                .avtransport_service_type
                .as_deref()
                .unwrap_or("<none>")
        );
        println!(
            "  [UPnP] RenderingControl control URL : {}",
            upnp.info
                .rendering_control_control_url
                .as_deref()
                .unwrap_or("<none>")
        );
        println!(
            "  [UPnP] RenderingControl service type: {}",
            upnp.info
                .rendering_control_service_type
                .as_deref()
                .unwrap_or("<none>")
        );
        println!(
            "  [UPnP] ConnectionManager control URL : {}",
            upnp.info
                .connection_manager_control_url
                .as_deref()
                .unwrap_or("<none>")
        );
        println!(
            "  [UPnP] ConnectionManager service type: {}",
            upnp.info
                .connection_manager_service_type
                .as_deref()
                .unwrap_or("<none>")
        );
    }

    // 6. Initial state dump (generic + UPnP-specific)
    dump_renderer_state(renderer, "Initial state")?;

    // 7. Play URI via logical façade
    let uri = maybe_uri.unwrap_or_else(|| DEFAULT_TEST_URI.to_string());
    let meta = ""; // or full DIDL-Lite

    println!("\nCalling play_uri on music renderer...");
    if let Err(e) = renderer.play_uri(&uri, meta) {
        println!("  play_uri failed: {e}");
    } else {
        println!("  play_uri: OK");
    }

    println!(
        "Waiting {}s to let the renderer prefetch and start playback...",
        AFTER_PLAY_WAIT_SECS
    );
    thread::sleep(Duration::from_secs(AFTER_PLAY_WAIT_SECS));
    dump_renderer_state(renderer, "After play_uri")?;

    // 8. Short progress polling using the PlaybackPosition façade
    progress_monitor(renderer, "Progress while playing", 8, 3);

    // 9. Seek (if supported)
    println!("\nCalling seek_rel_time(\"00:01:00\")...");
    if let Err(e) = renderer.seek_rel_time("00:01:00") {
        println!("  seek_rel_time failed: {e}");
    } else {
        println!("  seek_rel_time: OK");
        thread::sleep(Duration::from_secs(2));
        dump_renderer_state(renderer, "After seek_rel_time")?;

        // 9b. Second progress loop after seek: verify RelTime advances
        progress_monitor(renderer, "Progress after seek", 8, 3);
    }

    // 10. Volume dance (if supported)
    println!("\nProbing volume control via music façade...");
    if let Err(e) = volume_demo(renderer) {
        println!("  Volume control not fully usable: {e}");
    }

    // 11. Pause then Stop
    println!("\nCalling pause() via music façade...");
    if let Err(e) = renderer.pause() {
        println!("  pause failed: {e}");
    } else {
        println!("  pause: OK");
        thread::sleep(Duration::from_secs(2));
        dump_renderer_state(renderer, "After pause")?;
    }

    println!("\nCalling stop() via music façade...");
    if let Err(e) = renderer.stop() {
        println!("  stop failed: {e}");
    } else {
        println!("  stop: OK");
        dump_renderer_state(renderer, "After stop")?;
    }

    println!("\nDone.");
    Ok(())
}

fn print_capabilities(prefix: &str, caps: &RendererCapabilities, proto: &RendererProtocol) {
    println!("{prefix}Capabilities:");
    println!("{prefix}  Protocol      : {:?}", proto);
    println!("{prefix}  AVTransport   : {}", caps.has_avtransport);
    println!("{prefix}  RendControl   : {}", caps.has_rendering_control);
    println!("{prefix}  ConnManager   : {}", caps.has_connection_manager);
    println!("{prefix}  LinkPlay HTTP : {}", caps.has_linkplay_http);
    println!("{prefix}  Arylic TCP    : {}", caps.has_arylic_tcp);
    println!("{prefix}  OH Playlist   : {}", caps.has_oh_playlist);
    println!("{prefix}  OH Volume     : {}", caps.has_oh_volume);
    println!("{prefix}  OH Info       : {}", caps.has_oh_info);
    println!("{prefix}  OH Time       : {}", caps.has_oh_time);
    println!("{prefix}  OH Radio      : {}", caps.has_oh_radio);
}

fn print_backend(prefix: &str, renderer: &MusicRenderer) {
    let backend = match renderer {
        MusicRenderer::Upnp(_) => "UpnpRenderer (UPnP AV / DLNA)",
        MusicRenderer::LinkPlay(_) => "LinkPlayRenderer (LinkPlay HTTP)",
        MusicRenderer::ArylicTcp(_) => "ArylicTcpRenderer  (ARylic TCP Protocol)",
        MusicRenderer::HybridUpnpArylic{..} => "Hybrid UpnpArylicRenderer (UPnP AV / DLNA + ARylic TCP Protocol)",
    };
    println!("{prefix}Backend       : {backend}");
}

fn dump_renderer_state(renderer: &MusicRenderer, label: &str) -> Result<()> {
    println!("\n[{label}]");

    if let Ok(state) = renderer.playback_state() {
        println!("  Playback state (music): {:?}", state);
    } else {
        println!("  Playback state (music): <unavailable>");
    }

    // Generic volume façade
    match renderer.volume() {
        Ok(v) => println!("  Volume (music)   : {}", v),
        Err(e) => println!("  Volume not available: {e}"),
    }

    match renderer.mute() {
        Ok(m) => println!("  Mute               : {}", m),
        Err(e) => println!("  Mute state unknown : {e}"),
    }

    if let Ok(pos) = renderer.playback_position() {
        println!("  Position info:");
        println!("    Track       : {:?}", pos.track);
        println!("    Duration    : {:?}", pos.track_duration);
        println!("    RelTime     : {:?}", pos.rel_time);
        println!("    AbsTime     : {:?}", pos.abs_time);
    } else {
        println!("  Position info: <unavailable>");
    }

    // Optional UPnP-specific TransportInfo
    if let Some(upnp) = renderer.as_upnp() {
        if upnp.has_avtransport() {
            match upnp.avtransport() {
                Ok(avt) => match avt.get_transport_info(0) {
                    Ok(info) => {
                        println!("  [UPnP] TransportInfo:");
                        println!("    State  : {}", info.current_transport_state);
                        println!("    Status : {}", info.current_transport_status);
                        println!("    Speed  : {}", info.current_speed);
                    }
                    Err(e) => {
                        println!("  [UPnP] TransportInfo unavailable: {e}");
                    }
                },
                Err(e) => println!("  [UPnP] No AVTransport client: {e}"),
            }
        } else {
            println!("  [UPnP] AVTransport not present on this renderer.");
        }
    }

    Ok(())
}

fn progress_monitor(renderer: &MusicRenderer, label: &str, iterations: usize, interval_secs: u64) {
    println!(
        "\n[{label}] polling playback state/position {} times (every {} s)...",
        iterations, interval_secs
    );

    for i in 0..iterations {
        if let Ok(state) = renderer.playback_state() {
            print!("  Sample {:02}: state={:?}", i + 1, state);
        } else {
            print!("  Sample {:02}: state=<unavailable>", i + 1);
        }

        if let Ok(pos) = renderer.playback_position() {
            println!(
                " | track={:?}, rel={:?}, dur={:?}",
                pos.track, pos.rel_time, pos.track_duration
            );
        } else {
            println!(" | position=<unavailable>");
        }

        thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn volume_demo(renderer: &MusicRenderer) -> Result<()> {
    // Try to get current volume
    let original = renderer.volume()?;
    println!("  Current music volume  : {}", original);

    // Try mute toggle
    let muted = renderer.mute()?;
    println!("  Current mute state     : {}", muted);

    println!("  Setting mute = true...");
    renderer.set_mute(true)?;
    thread::sleep(Duration::from_secs(1));
    println!("  Mute now: {}", renderer.mute()?);

    println!("  Restoring mute = {}", muted);
    renderer.set_mute(muted)?;
    thread::sleep(Duration::from_millis(500));

    // Small volume bump if possible
    let new_volume = original.saturating_add(10).min(u16::MAX);
    println!("  Bumping volume to      : {}", new_volume);
    renderer.set_volume(new_volume)?;
    println!("  Volume after bump      : {}", renderer.volume()?);
    thread::sleep(Duration::from_secs(5));

    println!("  Restoring original volume: {}", original);
    println!("  Volume after reset      : {}", renderer.volume()?);
    renderer.set_volume(original)?;
    thread::sleep(Duration::from_secs(5));

    Ok(())
}
