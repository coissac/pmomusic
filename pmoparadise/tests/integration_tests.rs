//! Integration tests for pmoparadise

use pmoparadise::{Block, RadioParadiseClient};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a mock Block JSON response
fn mock_block_json(event: u64, end_event: u64) -> serde_json::Value {
    json!({
        "event": event,
        "end_event": end_event,
        "length": 900000,
        "url": format!("https://apps.radioparadise.com/blocks/chan/0/4/{}-{}.flac", event, end_event),
        "image_base": "https://img.radioparadise.com/covers/l/",
        "song": {
            "0": {
                "artist": "Miles Davis",
                "title": "So What",
                "album": "Kind of Blue",
                "year": 1959,
                "elapsed": 0,
                "duration": 540000,
                "cover": "B00000I0JF.jpg",
                "rating": 9.2
            },
            "1": {
                "artist": "John Coltrane",
                "title": "Giant Steps",
                "album": "Giant Steps",
                "year": 1960,
                "elapsed": 540000,
                "duration": 360000,
                "cover": "B000002I4U.jpg",
                "rating": 9.5
            }
        }
    })
}

#[tokio::test]
async fn test_get_current_block() {
    // Start mock server
    let mock_server = MockServer::start().await;

    // Setup mock response
    Mock::given(method("GET"))
        .and(path("/api/get_block"))
        .and(query_param("bitrate", "4"))
        .and(query_param("info", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_block_json(1234, 5678)))
        .mount(&mock_server)
        .await;

    // Create client with mock server URL
    let client = RadioParadiseClient::builder()
        .api_base(format!("{}/api", mock_server.uri()))
        .build()
        .await
        .unwrap();

    // Test get_block
    let block = client.get_block(None).await.unwrap();

    assert_eq!(block.event, 1234);
    assert_eq!(block.end_event, 5678);
    assert_eq!(block.length, 900000);
    assert_eq!(block.song_count(), 2);

    // Check songs
    let songs = block.songs_ordered();
    assert_eq!(songs.len(), 2);
    assert_eq!(songs[0].1.artist, "Miles Davis");
    assert_eq!(songs[1].1.artist, "John Coltrane");
}

#[tokio::test]
async fn test_get_specific_block() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/get_block"))
        .and(query_param("bitrate", "4"))
        .and(query_param("info", "true"))
        .and(query_param("event", "5678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_block_json(5678, 9012)))
        .mount(&mock_server)
        .await;

    let client = RadioParadiseClient::builder()
        .api_base(format!("{}/api", mock_server.uri()))
        .build()
        .await
        .unwrap();

    let block = client.get_block(Some(5678)).await.unwrap();

    assert_eq!(block.event, 5678);
    assert_eq!(block.end_event, 9012);
}

#[tokio::test]
async fn test_now_playing() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/get_block"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_block_json(1234, 5678)))
        .mount(&mock_server)
        .await;

    let client = RadioParadiseClient::builder()
        .api_base(format!("{}/api", mock_server.uri()))
        .build()
        .await
        .unwrap();

    let now_playing = client.now_playing().await.unwrap();

    assert_eq!(now_playing.block.event, 1234);
    assert_eq!(now_playing.current_song_index, Some(0));
    assert!(now_playing.current_song.is_some());

    if let Some(song) = &now_playing.current_song {
        assert_eq!(song.artist, "Miles Davis");
        assert_eq!(song.title, "So What");
    }
}




#[tokio::test]
async fn test_prefetch_next() {
    let mock_server = MockServer::start().await;

    // First block
    Mock::given(method("GET"))
        .and(query_param("event", "1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_block_json(1234, 5678)))
        .mount(&mock_server)
        .await;

    // Next block
    Mock::given(method("GET"))
        .and(query_param("event", "5678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_block_json(5678, 9012)))
        .mount(&mock_server)
        .await;

    let mut client = RadioParadiseClient::builder()
        .api_base(format!("{}/api", mock_server.uri()))
        .build()
        .await
        .unwrap();

    let current_block = client.get_block(Some(1234)).await.unwrap();
    assert_eq!(current_block.end_event, 5678);

    client.prefetch_next(&current_block).await.unwrap();

    let next_url = client.next_block_url().unwrap();
    assert!(next_url.contains("5678-9012.flac"));
}

#[tokio::test]
async fn test_block_parse_url_events() {
    let json = mock_block_json(1234, 5678);
    let block: Block = serde_json::from_value(json).unwrap();

    let (start, end) = block.parse_url_events().unwrap();
    assert_eq!(start, 1234);
    assert_eq!(end, 5678);
}

#[tokio::test]
async fn test_song_timing() {
    let json = mock_block_json(1234, 5678);
    let block: Block = serde_json::from_value(json).unwrap();

    // Find song at 0ms (should be first song)
    let (idx, song) = block.song_at_timestamp(0).unwrap();
    assert_eq!(idx, 0);
    assert_eq!(song.title, "So What");

    // Find song at 600000ms (should be second song)
    let (idx, song) = block.song_at_timestamp(600000).unwrap();
    assert_eq!(idx, 1);
    assert_eq!(song.title, "Giant Steps");

    // Timestamp beyond block
    assert!(block.song_at_timestamp(1000000).is_none());
}

#[tokio::test]
async fn test_song_cover_url() {
    let json = mock_block_json(1234, 5678);
    let block: Block = serde_json::from_value(json).unwrap();

    let song = block.get_song(0).unwrap();
    let cover_url = block.cover_url(song.cover.as_ref().unwrap()).unwrap();

    assert_eq!(
        cover_url,
        "https://img.radioparadise.com/covers/l/B00000I0JF.jpg"
    );
}

#[cfg(feature = "per-track")]
#[tokio::test]
async fn test_track_position_seconds() {
    let client = RadioParadiseClient::new().await.unwrap();
    let json = mock_block_json(1234, 5678);
    let block: Block = serde_json::from_value(json).unwrap();

    let (start, duration) = client.track_position_seconds(&block, 0).unwrap();
    assert_eq!(start, 0.0);
    assert_eq!(duration, 540.0);

    let (start, duration) = client.track_position_seconds(&block, 1).unwrap();
    assert_eq!(start, 540.0);
    assert_eq!(duration, 360.0);
}
