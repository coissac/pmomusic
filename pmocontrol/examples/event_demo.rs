// examples/events_demo.rs
//
// Demo temps réel des RendererEvent émis par le runtime de ControlPoint :
//   - SSDP discovery via `ControlPoint`
//   - sélection d'un renderer (facultatif)
//   - abonnement à `subscribe_events()`
//   - affichage continu des événements avec horodatage HH:MM:SS
//
// Build et run (depuis la racine du crate pmocontrol) :
//   cargo run --example events_demo --            # écoute tous les renderers
//   cargo run --example events_demo -- 0          # filtre sur renderer index 0
//   cargo run --example events_demo -- 1          # filtre sur renderer index 1, etc.
//
// Ctrl-C pour quitter.

use std::env;
use std::io;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pmocontrol::{
    ControlPoint, DeviceRegistryRead, PlaybackPositionInfo, PlaybackState, RendererEvent,
    RendererId, RendererInfo,
};

fn main() -> io::Result<()> {
    // Logging simple (tracing_subscriber est déjà utilisé dans les autres exemples)
    let _ = tracing_subscriber::fmt::try_init();
    println!("Starting PMOMusic renderer events demo...");

    // 1. Lance le ControlPoint (timeout HTTP pour les descriptions UPnP)
    let cp = ControlPoint::spawn(5)?;

    // 2. Laisse la découverte tourner un peu avant de lister les renderers
    println!("Waiting 5 seconds for SSDP discovery...");
    thread::sleep(Duration::from_secs(5));

    let registry = cp.registry();
    let renderers: Vec<RendererInfo> = {
        let reg = registry.read().unwrap();
        reg.list_renderers()
    };

    if renderers.is_empty() {
        println!("No renderers discovered. Make sure your devices are on and reachable.");
        return Ok(());
    }

    println!("\nDiscovered renderers:");
    for (idx, info) in renderers.iter().enumerate() {
        println!(
            "  [{}] {} | model={} | udn={} | location={} | online={}",
            idx, info.friendly_name, info.model_name, info.udn, info.location, info.online
        );
    }

    // 3. Optionnel : sélection d'un renderer par index (filtrage des événements)
    let args: Vec<String> = env::args().collect();
    let selected_id: Option<RendererId> = if args.len() >= 2 {
        match args[1].parse::<usize>() {
            Ok(idx) if idx < renderers.len() => {
                let info = &renderers[idx];
                println!(
                    "\nFiltering events on renderer [{}] {} (id={})",
                    idx, info.friendly_name, info.id.0
                );
                Some(info.id.clone())
            }
            Ok(idx) => {
                eprintln!(
                    "\nRenderer index {} is out of range (0..{}), listening to all renderers.",
                    idx,
                    renderers.len().saturating_sub(1)
                );
                None
            }
            Err(e) => {
                eprintln!(
                    "\nArgument '{}' is not a valid index (error: {}), listening to all renderers.",
                    args[1], e
                );
                None
            }
        }
    } else {
        println!("\nNo renderer index provided, listening to events from all renderers.");
        None
    };

    // 4. Abonnement aux événements du runtime
    let rx = cp.subscribe_events();

    println!("\nSubscribed to renderer events.");
    println!("Press Ctrl-C to quit.\n");

    // 5. Boucle bloquante sur les événements
    loop {
        match rx.recv() {
            Ok(event) => {
                if let Some(ref id) = selected_id {
                    // Filtre : on ignore les événements des autres renderers
                    if !event_matches_id(&event, id) {
                        continue;
                    }
                }

                print_event(&event);
            }
            Err(err) => {
                eprintln!("Event channel closed: {}. Exiting.", err);
                break;
            }
        }
    }

    Ok(())
}

/// Vérifie si un événement concerne un RendererId donné.
fn event_matches_id(event: &RendererEvent, id: &RendererId) -> bool {
    match event {
        RendererEvent::StateChanged { id: eid, .. } => eid == id,
        RendererEvent::PositionChanged { id: eid, .. } => eid == id,
        RendererEvent::VolumeChanged { id: eid, .. } => eid == id,
        RendererEvent::MuteChanged { id: eid, .. } => eid == id,
    }
}

/// Format HH:MM:SS basé sur l'heure système (UTC mod 24h).
fn now_hms() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    let total = now.as_secs() % 86_400;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

/// Affichage lisible d'un PlaybackState.
fn format_playback_state(state: &PlaybackState) -> String {
    match state {
        PlaybackState::Stopped => "Stopped".to_string(),
        PlaybackState::Playing => "Playing".to_string(),
        PlaybackState::Paused => "Paused".to_string(),
        PlaybackState::Transitioning => "Transitioning".to_string(),
        PlaybackState::NoMedia => "NoMedia".to_string(),
        PlaybackState::Unknown(s) => format!("Unknown({})", s),
    }
}

/// Affichage lisible d'un PlaybackPositionInfo.
fn format_position(pos: &PlaybackPositionInfo) -> String {
    let track = pos.track.map(|t| t.to_string()).unwrap_or_else(|| "-".to_string());
    let rel = pos
        .rel_time
        .as_deref()
        .unwrap_or("-")
        .to_string();
    let dur = pos
        .track_duration
        .as_deref()
        .unwrap_or("-")
        .to_string();

    format!("track={} rel_time={} duration={}", track, rel, dur)
}

/// Affiche un RendererEvent avec horodatage.
fn print_event(event: &RendererEvent) {
    let ts = now_hms();
    match event {
        RendererEvent::StateChanged { id, state } => {
            println!(
                "[{}] [{}] StateChanged: {}",
                ts,
                id.0,
                format_playback_state(state)
            );
        }
        RendererEvent::PositionChanged { id, position } => {
            println!(
                "[{}] [{}] PositionChanged: {}",
                ts,
                id.0,
                format_position(position)
            );
        }
        RendererEvent::VolumeChanged { id, volume } => {
            println!("[{}] [{}] VolumeChanged: {}", ts, id.0, volume);
        }
        RendererEvent::MuteChanged { id, mute } => {
            println!("[{}] [{}] MuteChanged: {}", ts, id.0, mute);
        }
    }
}
