use anyhow::{Result, anyhow};
use pmocontrol::{ControlPoint, DeviceRegistryRead, RendererInfo};
use std::env;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // Default values
    let default_uri =
        "https://audio-fb.radioparadise.com/chan/1/x/1117/4/g/1117-3.flac".to_string();
    let mut uri = default_uri.clone();
    let mut renderer_index: usize = 0;

    // Args:
    //   - si args[1] est un entier : index, args[2] éventuel = uri
    //   - sinon : args[1] = uri, args[2] éventuel = index
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let first = &args[1];
        if let Ok(idx) = first.parse::<usize>() {
            // cas: avtransport_demo 1 [URI]
            renderer_index = idx;
            if args.len() >= 3 {
                uri = args[2].clone();
            }
        } else {
            // cas: avtransport_demo URI [INDEX]
            uri = first.clone();
            if args.len() >= 3 {
                if let Ok(idx) = args[2].parse::<usize>() {
                    renderer_index = idx;
                }
            }
        }
    }

    println!("Using URI: {}", uri);
    println!("Requested renderer index: {}", renderer_index);

    // 1. Start control point and let discovery run a bit
    let cp = ControlPoint::spawn(5)?;
    thread::sleep(Duration::from_secs(5));

    // 2. Get registry snapshot
    let registry = cp.registry();
    let reg = registry.read().unwrap();
    let all_renderers = reg.list_renderers();

    // Filter out the in-dev PMOMusic renderer
    let renderers: Vec<RendererInfo> = all_renderers
        .into_iter()
        .filter(|r| {
            !r.friendly_name
                .to_ascii_lowercase()
                .contains("pmomusic audio renderer")
        })
        .collect();

    if renderers.is_empty() {
        println!("No valid UPnP MediaRenderer discovered.");
        return Ok(());
    }

    println!("Discovered MediaRenderers:");
    for (idx, r) in renderers.iter().enumerate() {
        println!(
            "  [{}] {} | model={} | udn={} | location={}",
            idx, r.friendly_name, r.model_name, r.udn, r.location,
        );
    }

    if renderer_index >= renderers.len() {
        return Err(anyhow!(
            "Renderer index {} out of range (0..={})",
            renderer_index,
            renderers.len().saturating_sub(1)
        ));
    }

    // 3. Selection by index
    let renderer: &RendererInfo = &renderers[renderer_index];

    println!("\nSelected renderer (index {}):", renderer_index);
    println!("  Name        : {}", renderer.friendly_name);
    println!("  Model       : {}", renderer.model_name);
    println!("  Manufacturer: {}", renderer.manufacturer);
    println!("  UDN         : {}", renderer.udn);
    println!("  Location    : {}", renderer.location);

    // 4. Get AVTransport client
    let avtransport = reg
        .avtransport_client_for_renderer(&renderer.id)
        .expect("Selected renderer has no AVTransport service");

    println!("  AVTransport control URL : {}", avtransport.control_url);
    println!("  AVTransport service type: {}", avtransport.service_type);

    // We are done with the registry lock
    drop(reg);

    // 5. Configure the URI
    let meta = ""; // or a full DIDL-Lite string

    println!("\nCalling SetAVTransportURI...");
    avtransport.set_av_transport_uri(&uri, meta)?;
    println!("  SetAVTransportURI: OK");

    // Helper closure to dump current TransportInfo
    let dump_info = |label: &str| -> Result<()> {
        let info = avtransport.get_transport_info(0)?;
        println!("\n[{}]", label);
        println!("  State  : {}", info.current_transport_state);
        println!("  Status : {}", info.current_transport_status);
        println!("  Speed  : {}", info.current_speed);
        Ok(())
    };

    dump_info("After SetAVTransportURI")?;

    // 6. Play
    println!("\nCalling Play (Speed=\"1\")...");
    avtransport.play(0, "1")?;
    println!("  Play: OK");
    thread::sleep(Duration::from_secs(20));
    dump_info("After Play")?;

    // 7. Optional: wait before Pause/Stop
    print!("\nPress ENTER to Pause...");
    io::stdout().flush().ok();
    let _ = io::stdin().read_line(&mut String::new());

    // 8. Pause
    println!("\nCalling Pause...");
    if let Err(e) = avtransport.pause(0) {
        println!("  Pause failed: {e}");
    } else {
        println!("  Pause: OK");
        thread::sleep(Duration::from_secs(2));
        dump_info("After Pause")?;
    }

    // 9. Stop
    println!("\nCalling Stop...");
    if let Err(e) = avtransport.stop(0) {
        println!("  Stop failed: {e}");
    } else {
        println!("  Stop: OK");
        dump_info("After Stop")?;
    }

    println!("\nDone.");
    Ok(())
}
