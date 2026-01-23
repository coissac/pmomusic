use std::collections::HashSet;
use std::env;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use pmocontrol::{
    DeviceDescriptionProvider, DiscoveredEndpoint, HttpXmlDescriptionProvider,
    MusicRendererBackend, RendererInfo,
    control_point::ControlPoint,
    openhome_client::{
        OPENHOME_PLAYLIST_HEAD_ID, OhInfoClient, OhPlaylistClient, OhProductClient, OhRadioClient,
        OhTimeClient, OhVolumeClient, parse_track_metadata_from_didl,
    },
};

fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let args: Vec<String> = env::args().collect();

    let renderer = match args.len() {
        1 => auto_discover_renderer()
            .context("Unable to auto-discover the OpenHome renderer; rerun with explicit UDN and description URL")?,
        3 => {
            let udn = args[1].to_ascii_lowercase();
            let description_url = args[2].clone();
            renderer_from_description(&udn, &description_url)?
        }
        _ => {
            eprintln!(
                "Usage:\n  {0}            # auto-discover the single OpenHome renderer\n  {0} <renderer_udn> <description_url>",
                args[0]
            );
            std::process::exit(1);
        }
    };

    println!(
        "Renderer: {} ({})",
        renderer.friendly_name, renderer.model_name
    );
    println!("UDN: {}", renderer.udn);
    println!(
        "OpenHome services -> playlist:{} info:{} time:{} volume:{} radio:{} product:{}",
        renderer.oh_playlist_control_url.is_some(),
        renderer.oh_info_control_url.is_some(),
        renderer.oh_time_control_url.is_some(),
        renderer.oh_volume_control_url.is_some(),
        renderer.oh_radio_control_url.is_some(),
        renderer.oh_product_control_url.is_some(),
    );

    if let Some(result) = test_product(&renderer) {
        result?;
    }
    if let Some(result) = test_playlist(&renderer) {
        result?;
    }
    if let Some(result) = test_info(&renderer) {
        result?;
    }
    if let Some(result) = test_time(&renderer) {
        result?;
    }
    if let Some(result) = test_volume(&renderer) {
        result?;
    }
    if let Some(result) = test_radio(&renderer) {
        result?;
    }

    Ok(())
}

fn auto_discover_renderer() -> Result<RendererInfo> {
    println!("Auto-discovering OpenHome renderer via SSDP…");
    let control_point = ControlPoint::spawn(5).context("Failed to start control point")?;
    let deadline = Instant::now() + Duration::from_secs(12);

    while Instant::now() < deadline {
        let renderers = control_point.list_music_renderers();
        let mut seen = HashSet::new();
        let mut openhome_infos = Vec::new();

        for renderer in renderers {
            if let MusicRendererBackend::OpenHome(oh) = renderer {
                if seen.insert(oh.id().0.clone()) {
                    openhome_infos.push(oh.info.clone());
                }
            }
        }

        match openhome_infos.len() {
            0 => {
                thread::sleep(Duration::from_millis(500));
            }
            1 => {
                let info = openhome_infos.remove(0);
                println!(
                    "Discovered OpenHome renderer '{}' ({})",
                    info.friendly_name, info.udn
                );
                return Ok(info);
            }
            _ => {
                println!("Detected multiple OpenHome renderers:");
                for info in &openhome_infos {
                    println!("  - {} ({})", info.friendly_name, info.udn);
                }
                return Err(anyhow!(
                    "Multiple OpenHome renderers present; rerun with explicit UDN + description URL."
                ));
            }
        }
    }

    Err(anyhow!("No OpenHome renderer discovered on the network."))
}

fn renderer_from_description(udn: &str, description_url: &str) -> Result<RendererInfo> {
    let endpoint = DiscoveredEndpoint::new(
        udn.to_ascii_lowercase(),
        description_url.to_string(),
        "openhome-tester/1.0".into(),
        1800,
    );
    let provider = HttpXmlDescriptionProvider::new(5);
    provider
        .build_renderer_info(&endpoint)
        .with_context(|| format!("Device {} is not a usable renderer", description_url))
}

fn test_product(info: &RendererInfo) -> Option<Result<()>> {
    let control_url = info.oh_product_control_url.as_ref()?;
    let service_type = info.oh_product_service_type.as_ref()?;
    let client = OhProductClient::new(control_url.clone(), service_type.clone());

    Some((|| {
        println!("\n[Product] Listing sources…");
        let sources = client.source_xml()?;
        for (idx, source) in sources.iter().enumerate() {
            println!(
                "  #{idx} {} (type={}, visible={})",
                source.name, source.source_type, source.visible
            );
        }
        let current = client.source_index()?;
        println!("[Product] Current source index: {current}");
        client.ensure_playlist_source_selected()?;
        println!("[Product] Playlist source is now selected");
        Ok(())
    })())
}

fn test_playlist(info: &RendererInfo) -> Option<Result<()>> {
    let control_url = info.oh_playlist_control_url.as_ref()?;
    let service_type = info.oh_playlist_service_type.as_ref()?;
    let client = OhPlaylistClient::new(control_url.clone(), service_type.clone());

    Some((|| {
        println!("\n[Playlist] TracksMax={}", client.tracks_max()?);
        let ids = client.id_array()?;
        println!(
            "[Playlist] IdArray contains {} entries (showing up to 5)",
            ids.len()
        );
        if ids.is_empty() {
            println!("[Playlist] Playlist is empty");
        } else {
            let preview = &ids[..ids.len().min(5)];
            let entries = client.read_list(preview)?;
            for entry in entries {
                println!(
                    "  - id={} uri={} title={}",
                    entry.id,
                    entry.uri,
                    parse_track_metadata_from_didl(&entry.metadata_xml)
                        .and_then(|m| m.title)
                        .unwrap_or_else(|| "<unknown>".into())
                );
            }
        }

        exercise_playlist_mutations(&client)?;
        Ok(())
    })())
}

fn exercise_playlist_mutations(client: &OhPlaylistClient) -> Result<()> {
    println!("\n[Playlist] Exercising DeleteAll → Insert → DeleteAll");
    client.delete_all()?;
    println!("  DeleteAll succeeded");
    std::thread::sleep(Duration::from_millis(200));
    let _ = client.id_array()?;

    let test_uri = env::var("OPENHOME_TEST_URI")
        .unwrap_or_else(|_| "http://ice1.somafm.com/groovesalad-128-mp3".into());
    let metadata = build_sample_metadata_xml(&test_uri);

    println!(
        "  Inserting sample track at head (after_id = {:#x})…",
        OPENHOME_PLAYLIST_HEAD_ID
    );
    let new_id = match client.insert(OPENHOME_PLAYLIST_HEAD_ID, &test_uri, &metadata) {
        Ok(id) => {
            println!("  Insert succeeded with id {}", id);
            id
        }
        Err(err) => {
            println!(
                "  Insert with sentinel failed: {}. Retrying with aAfterId=0…",
                err
            );
            let id = client.insert(0, &test_uri, &metadata)?;
            println!("  Insert with aAfterId=0 succeeded with id {}", id);
            id
        }
    };

    let ids = client.id_array()?;
    println!("  Playlist now reports {} id(s): {:?}", ids.len(), ids);

    let entries = client.read_list(&[new_id])?;
    if let Some(entry) = entries.first() {
        println!("  ReadList confirms id={} uri={}", entry.id, entry.uri);
    }

    client.delete_all()?;
    println!("  Playlist restored to empty state");
    Ok(())
}

fn build_sample_metadata_xml(uri: &str) -> String {
    format!(
        concat!(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" "#,
            r#"xmlns:dc="http://purl.org/dc/elements/1.1/" "#,
            r#"xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
            r#"<item id="pmo:test:track" parentID="0" restricted="0">"#,
            r#"<dc:title>PMO Test Track</dc:title>"#,
            r#"<res protocolInfo="http-get:*:audio/mpeg:*">"#,
            "{uri}",
            r#"</res>"#,
            r#"<upnp:class>object.item.audioItem.musicTrack</upnp:class>"#,
            r#"</item></DIDL-Lite>"#
        ),
        uri = uri
    )
}

fn test_info(info: &RendererInfo) -> Option<Result<()>> {
    let control_url = info.oh_info_control_url.as_ref()?;
    let service_type = info.oh_info_service_type.as_ref()?;
    let client = OhInfoClient::new(control_url.clone(), service_type.clone());

    Some((|| {
        println!("\n[Info] Reading current track…");
        let track = client.track()?;
        println!("  URI: {}", track.uri);
        if let Some(metadata) = track.metadata() {
            println!(
                "  Metadata: {} – {}",
                metadata.artist.as_deref().unwrap_or("<unknown artist>"),
                metadata.title.as_deref().unwrap_or("<unknown title>")
            );
        }
        match client.transport_state() {
            Ok(state) => println!("  Transport state: {}", state),
            Err(err) => println!("  TransportState call failed: {err}"),
        }
        Ok(())
    })())
}

fn test_time(info: &RendererInfo) -> Option<Result<()>> {
    let control_url = info.oh_time_control_url.as_ref()?;
    let service_type = info.oh_time_service_type.as_ref()?;
    let client = OhTimeClient::new(control_url.clone(), service_type.clone());

    Some((|| {
        println!("\n[Time] Querying position…");
        let pos = client.position()?;
        println!(
            "  Tracks={} duration={}s elapsed={}s",
            pos.track_count, pos.duration_secs, pos.elapsed_secs
        );
        Ok(())
    })())
}

fn test_volume(info: &RendererInfo) -> Option<Result<()>> {
    let control_url = info.oh_volume_control_url.as_ref()?;
    let service_type = info.oh_volume_service_type.as_ref()?;
    let client = OhVolumeClient::new(control_url.clone(), service_type.clone());

    Some((|| {
        println!("\n[Volume] Current volume: {}", client.volume()?);
        println!("[Volume] Muted: {}", client.mute()?);
        Ok(())
    })())
}

fn test_radio(info: &RendererInfo) -> Option<Result<()>> {
    let control_url = info.oh_radio_control_url.as_ref()?;
    let service_type = info.oh_radio_service_type.as_ref()?;
    let client = OhRadioClient::new(control_url.clone(), service_type.clone());

    Some((|| {
        println!("\n[Radio] Fetching channel #0 metadata…");
        let channel = client.channel(0)?;
        println!("  URI: {}", channel.uri);
        if let Some(meta) = channel.metadata_xml {
            println!("  Metadata XML: {}", meta);
        }
        Ok(())
    })())
}
