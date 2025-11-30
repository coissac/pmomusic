// pmocontrol/examples/renderer_demo.rs

use anyhow::{anyhow, Result};
use pmocontrol::{ControlPoint, Renderer};
use std::env;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // -------------------------------------------------------------------------
    // CLI arguments
    //
    // Usage:
    //   renderer_demo
    //     -> URI par défaut, renderer index 0
    //
    //   renderer_demo 1
    //     -> URI par défaut, renderer index 1
    //
    //   renderer_demo https://.../track.flac
    //     -> cette URI, renderer index 0
    //
    //   renderer_demo https://.../track.flac 1
    //     -> cette URI, renderer index 1
    // -------------------------------------------------------------------------
    let default_uri =
        "https://audio-fb.radioparadise.com/chan/1/x/1117/4/g/1117-3.flac".to_string();
    let mut uri = default_uri.clone();
    let mut renderer_index: usize = 0;

    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let first = &args[1];
        if let Ok(idx) = first.parse::<usize>() {
            // cas: renderer_demo 1 [URI]
            renderer_index = idx;
            if args.len() >= 3 {
                uri = args[2].clone();
            }
        } else {
            // cas: renderer_demo URI [INDEX]
            uri = first.clone();
            if args.len() >= 3 {
                if let Ok(idx) = args[2].parse::<usize>() {
                    renderer_index = idx;
                }
            }
        }
    }

    println!("Using URI                  : {}", uri);
    println!("Requested renderer index   : {}", renderer_index);

    // -------------------------------------------------------------------------
    // 1. Démarrer le control point et laisser la découverte tourner un peu
    // -------------------------------------------------------------------------
    let cp = ControlPoint::spawn(5)?;
    thread::sleep(Duration::from_secs(5));

    // -------------------------------------------------------------------------
    // 2. Récupérer la liste des renderers (handles haut niveau)
    // -------------------------------------------------------------------------
    let mut renderers: Vec<Renderer> = cp.list_renderer_handles();

    // Filtre : on ignore le renderer PMOMusic interne en développement
    renderers.retain(|r| {
        !r.friendly_name()
            .to_ascii_lowercase()
            .contains("pmomusic audio renderer")
    });

    if renderers.is_empty() {
        println!("No valid UPnP MediaRenderer discovered.");
        return Ok(());
    }

    println!("\nDiscovered MediaRenderers:");
    for (idx, r) in renderers.iter().enumerate() {
        println!(
            "  [{}] {} | model={} | manufacturer={} | has_avt={} | has_rc={}",
            idx,
            r.friendly_name(),
            r.info.model_name,
            r.info.manufacturer,
            r.has_avtransport(),
            r.has_rendering_control(),
        );
    }

    if renderer_index >= renderers.len() {
        return Err(anyhow!(
            "Renderer index {} out of range (0..={})",
            renderer_index,
            renderers.len().saturating_sub(1)
        ));
    }

    // -------------------------------------------------------------------------
    // 3. Sélection du renderer
    // -------------------------------------------------------------------------
    let renderer = &renderers[renderer_index];

    println!("\nSelected renderer (index {}):", renderer_index);
    println!("  Name        : {}", renderer.friendly_name());
    println!("  Model       : {}", renderer.info.model_name);
    println!("  Manufacturer: {}", renderer.info.manufacturer);
    println!("  UDN         : {}", renderer.info.udn);
    println!("  Location    : {}", renderer.info.location);
    println!("  has_avt     : {}", renderer.has_avtransport());
    println!("  has_rc      : {}", renderer.has_rendering_control());

    // -------------------------------------------------------------------------
    // 4. Si RenderingControl dispo : tester get/set volume + mute
    //    (set_* remet juste la même valeur pour ne rien changer en pratique)
    // -------------------------------------------------------------------------
    if renderer.has_rendering_control() {
        println!("\n[RenderingControl]");
        match renderer.get_master_volume() {
            Ok(vol) => {
                println!("  Current master volume: {}", vol);
                if let Err(e) = renderer.set_master_volume(vol) {
                    println!("  set_master_volume({}) failed: {}", vol, e);
                } else {
                    println!("  set_master_volume({}) OK (no-op)", vol);
                }
            }
            Err(e) => {
                println!("  get_master_volume() failed: {}", e);
            }
        }

        match renderer.get_master_mute() {
            Ok(muted) => {
                println!("  Current master mute   : {}", muted);
                if let Err(e) = renderer.set_master_mute(muted) {
                    println!("  set_master_mute({}) failed: {}", muted, e);
                } else {
                    println!("  set_master_mute({}) OK (no-op)", muted);
                }
            }
            Err(e) => {
                println!("  get_master_mute() failed: {}", e);
            }
        }
    } else {
        println!("\n[RenderingControl]");
        println!("  Renderer has no RenderingControl service.");
    }

    // -------------------------------------------------------------------------
    // 5. Si AVTransport dispo : tester play_uri / seek / pause / stop
    // -------------------------------------------------------------------------
    if !renderer.has_avtransport() {
        println!("\n[AVTransport]");
        println!("  Renderer has no AVTransport service, skipping playback tests.");
        return Ok(());
    }

    println!("\n[AVTransport]");

    // Helper pour afficher l'état courant
    let dump_info = |label: &str| -> Result<()> {
        let info = renderer
            .avtransport()
            .and_then(|_| renderer.avtransport().unwrap().get_transport_info(0))?;
        println!("\n  [{}]", label);
        println!("    State  : {}", info.current_transport_state);
        println!("    Status : {}", info.current_transport_status);
        println!("    Speed  : {}", info.current_speed);
        Ok(())
    };

    // Set + Play
    println!("\n  Calling play_uri(...)");
    renderer.play_uri(&uri, "")?;
    println!("  play_uri: OK");
    thread::sleep(Duration::from_secs(8));
    let _ = dump_info("After play_uri");

    // Seek (si supporté)
    println!("\n  Calling seek_rel_time(\"00:01:00\")...");
    match renderer.seek_rel_time("00:01:00") {
        Ok(()) => {
            println!("  seek_rel_time: OK");
            thread::sleep(Duration::from_secs(3));
            let _ = dump_info("After seek_rel_time");
        }
        Err(e) => {
            println!("  seek_rel_time failed: {}", e);
        }
    }

    // Pause (ENTER pour laisser jouer)
    print!("\nPress ENTER to Pause...");
    io::stdout().flush().ok();
    let _ = io::stdin().read_line(&mut String::new());

    println!("\n  Calling pause()...");
    match renderer.pause() {
        Ok(()) => {
            println!("  pause: OK");
            thread::sleep(Duration::from_secs(2));
            let _ = dump_info("After pause");
        }
        Err(e) => {
            println!("  pause failed: {}", e);
        }
    }

    // Stop
    print!("\nPress ENTER to Stop...");
    io::stdout().flush().ok();
    let _ = io::stdin().read_line(&mut String::new());

    println!("\n  Calling stop()...");
    match renderer.stop() {
        Ok(()) => {
            println!("  stop: OK");
            let _ = dump_info("After stop");
        }
        Err(e) => {
            println!("  stop failed: {}", e);
        }
    }

    println!("\nDone.");
    Ok(())
}
