use std::io;
use std::thread;
use std::time::Duration;

use pmocontrol::{
    ControlPoint, DeviceRegistryRead, MusicRendererBackend, PlaybackPosition, PlaybackPositionInfo,
    PlaybackState, PlaybackStatus, VolumeControl,
};

fn main() -> io::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    println!("Starting PMOMusic LinkPlay demo...");

    let cp = ControlPoint::spawn(5)?;
    println!("Waiting 5 seconds for SSDP discovery...");
    thread::sleep(Duration::from_secs(5));

    let registry = cp.registry();
    let renderers = {
        let reg = registry.read().unwrap();
        reg.list_renderers()
    };

    if renderers.is_empty() {
        println!("No renderers discovered.");
        return Ok(());
    }

    println!("Discovered renderers:");
    for (idx, info) in renderers.iter().enumerate() {
        println!(
            "  [{}] {} | model={} | LinkPlay HTTP={}",
            idx, info.friendly_name, info.model_name, info.capabilities.has_linkplay_http
        );
    }

    let linkplay_renderers: Vec<MusicRendererBackend> = renderers
        .iter()
        .filter(|info| info.capabilities.has_linkplay_http)
        .filter_map(|info| MusicRendererBackend::from_renderer_info(info.clone(), &registry))
        .collect();

    if linkplay_renderers.is_empty() {
        println!("\nNo LinkPlay-capable renderers detected.");
        return Ok(());
    }

    println!("\nLinkPlay renderer status:");
    for renderer in linkplay_renderers {
        println!("- {} ({})", renderer.friendly_name(), renderer.id().0);

        match renderer.playback_state() {
            Ok(state) => println!("    state: {}", format_playback_state(&state)),
            Err(err) => println!("    state: error: {}", err),
        }

        match renderer.playback_position() {
            Ok(pos) => println!("    position: {}", format_position(&pos)),
            Err(err) => println!("    position: error: {}", err),
        }

        match renderer.volume() {
            Ok(vol) => println!("    volume: {}", vol),
            Err(err) => println!("    volume: error: {}", err),
        }

        match renderer.mute() {
            Ok(mute) => println!("    mute: {}", mute),
            Err(err) => println!("    mute: error: {}", err),
        }

        println!();
    }

    Ok(())
}

fn format_playback_state(state: &PlaybackState) -> String {
    match state {
        PlaybackState::Stopped => "Stopped".to_string(),
        PlaybackState::Playing => "Playing".to_string(),
        PlaybackState::Paused => "Paused".to_string(),
        PlaybackState::Transitioning => "Transitioning".to_string(),
        PlaybackState::NoMedia => "NoMedia".to_string(),
        PlaybackState::Unknown(raw) => format!("Unknown({})", raw),
    }
}

fn format_position(pos: &PlaybackPositionInfo) -> String {
    let rel = pos.rel_time.as_deref().unwrap_or("-");
    let dur = pos.track_duration.as_deref().unwrap_or("-");
    let track = pos
        .track
        .map(|t| t.to_string())
        .unwrap_or_else(|| "-".to_string());

    format!("track={} rel_time={} duration={}", track, rel, dur)
}
