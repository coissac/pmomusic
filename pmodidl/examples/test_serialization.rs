use pmodidl::{DIDLLite, Item, Resource};

fn main() {
    let item1 = Item {
        id: "test1".to_string(),
        parent_id: "root".to_string(),
        restricted: Some("1".to_string()),
        title: "Test Song".to_string(),
        creator: Some("Test Artist".to_string()),
        class: "object.item.audioItem.musicTrack".to_string(),
        artist: Some("Test Artist".to_string()),
        album: None, // Pas d'album
        genre: None,
        album_art: None, // Pas d'albumArtURI
        album_art_pk: None,
        date: None,
        original_track_number: None,
        resources: vec![Resource {
            protocol_info: "http-get:*:audio/flac:*".to_string(),
            bits_per_sample: Some("16".to_string()),
            sample_frequency: Some("44100".to_string()),
            nr_audio_channels: Some("2".to_string()),
            duration: Some("0:03:00".to_string()),
            url: "http://example.com/test.flac".to_string(),
        }],
        descriptions: vec![],
    };

    let didl = DIDLLite {
        xmlns: "urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/".to_string(),
        xmlns_upnp: Some("urn:schemas-upnp-org:metadata-1-0/upnp/".to_string()),
        xmlns_dc: Some("http://purl.org/dc/elements/1.1/".to_string()),
        xmlns_dlna: Some("urn:schemas-dlna-org:metadata-1-0/".to_string()),
        xmlns_pv: None,
        xmlns_sec: None,
        containers: vec![],
        items: vec![item1],
    };

    let xml = quick_xml::se::to_string(&didl).expect("Serialization failed");

    println!("=== Output from quick_xml::se::to_string() ===");
    println!("{}", xml);
    println!("\n=== Length: {} bytes ===", xml.len());
    println!(
        "\n=== Starts with '<?xml' ? {} ===",
        xml.starts_with("<?xml")
    );
    println!("\n=== With manual XML declaration ===");
    let with_decl = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{}", xml);
    println!("{}", with_decl);
}
