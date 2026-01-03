use std::io;
use std::thread;
use std::time::Duration;

use pmocontrol::{
    ControlPoint, DeviceRegistryRead, MusicRendererBackend, PlaybackPosition, PlaybackPositionInfo,
    PlaybackState, PlaybackStatus, TransportControl, VolumeControl,
};

fn main() -> io::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    println!("Starting PMOMusic Arylic TCP demo...");

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
            "  [{}] {} | model={} | Arylic TCP={} | LinkPlay HTTP={}",
            idx,
            info.friendly_name,
            info.model_name,
            info.capabilities.has_arylic_tcp,
            info.capabilities.has_linkplay_http
        );
    }

    let arylic_renderers: Vec<MusicRendererBackend> = renderers
        .iter()
        .filter(|info| info.capabilities.has_arylic_tcp)
        .filter_map(|info| MusicRendererBackend::from_renderer_info(info.clone(), &registry))
        .collect();

    if arylic_renderers.is_empty() {
        println!("\nNo Arylic TCP-capable renderers detected.");
        return Ok(());
    }

    println!("\nArylic TCP renderer status:");
    for renderer in arylic_renderers {
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

        println!("    attempting pause/play test...");
        if let Err(err) = renderer.pause() {
            println!("      pause error: {}", err);
        } else {
            thread::sleep(Duration::from_millis(500));
            println!("      pause OK");
        }

        if let Err(err) = renderer.play() {
            println!("      play error: {}", err);
        } else {
            println!("      play OK");
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
