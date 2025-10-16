# pmosource - Music Source Abstraction for PMOMusic

Common traits and types for PMOMusic sources.

This crate provides the foundational abstractions for different music sources in the PMOMusic ecosystem, such as Radio Paradise, Qobuz, local playlists, etc.

## Features

- **FIFO Support**: Dynamic audio sources using `pmoplaylist` for streaming
- **Container/Item Navigation**: Browse and search using DIDL-Lite format (`pmodidl`)
- **Cache Integration**: Automatic URI resolution with `pmoaudiocache` and `pmocovers`
- **Change Tracking**: `update_id` and `last_change` for UPnP notifications
- **Send + Sync**: Ready for async servers

## Architecture

The `MusicSource` trait provides a unified interface for all music sources:

```
┌─────────────────────────────────────┐
│       MusicSource Trait             │
├─────────────────────────────────────┤
│ • Basic Info (name, id, image)      │
│ • ContentDirectory (browse, search) │
│ • URI Resolution (with caching)     │
│ • FIFO Management                   │
│ • Change Tracking                   │
└─────────────────────────────────────┘
         ▲         ▲         ▲
         │         │         │
    ┌────┴───┐ ┌──┴────┐ ┌──┴─────┐
    │ Radio  │ │ Qobuz │ │ Local  │
    │Paradise│ │       │ │Playlist│
    └────────┘ └───────┘ └────────┘
```

## Quick Start

### Implementing a Music Source

```rust
use pmosource::{async_trait, MusicSource, BrowseResult, Result};
use pmodidl::{Container, Item};
use pmoplaylist::FifoPlaylist;
use std::time::SystemTime;

#[derive(Debug)]
pub struct MyRadioSource {
    playlist: FifoPlaylist,
    // ... other fields
}

#[async_trait]
impl MusicSource for MyRadioSource {
    fn name(&self) -> &str {
        "My Radio"
    }

    fn id(&self) -> &str {
        "my-radio"
    }

    fn default_image(&self) -> &[u8] {
        include_bytes!("../assets/my-radio.webp")
    }

    async fn root_container(&self) -> Result<Container> {
        Ok(self.playlist.as_container().await)
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        // Return items from FIFO
        let tracks = self.playlist.get_items(0, 100).await;
        // Convert tracks to Items...
        Ok(BrowseResult::Items(items))
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        // Return cached URI if available, or original URI
        Ok(format!("http://cache-server/audio/{}", object_id))
    }

    fn supports_fifo(&self) -> bool {
        true
    }

    async fn append_track(&self, track: Item) -> Result<()> {
        // Convert Item to Track and add to playlist
        self.playlist.append_track(pmo_track).await;
        Ok(())
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        if let Some(track) = self.playlist.remove_oldest().await {
            // Convert Track to Item and return
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    async fn update_id(&self) -> u32 {
        self.playlist.update_id().await
    }

    async fn last_change(&self) -> Option<SystemTime> {
        Some(self.playlist.last_change().await)
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        let tracks = self.playlist.get_items(offset, count).await;
        // Convert tracks to Items...
        Ok(items)
    }
}
```

### Using a Music Source

```rust
use pmosource::MusicSource;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let source = MyRadioSource::new("http://localhost:8080");

    // Get source info
    println!("Source: {}", source.name());
    println!("ID: {}", source.id());

    // Get root container for ContentDirectory
    let root = source.root_container().await?;
    println!("Root: {} ({})", root.title, root.id);

    // Browse items
    let result = source.browse(&root.id).await?;
    for item in result.items() {
        println!("Track: {}", item.title);
    }

    // Resolve audio URI
    let uri = source.resolve_uri("track-123").await?;
    println!("Stream from: {}", uri);

    // Track changes
    println!("Update ID: {}", source.update_id().await);

    Ok(())
}
```

## Trait Methods

### Basic Information

- `name() -> &str`: Human-readable name
- `id() -> &str`: Unique identifier (e.g., "radio-paradise")
- `default_image() -> &[u8]`: Embedded WebP logo (300x300px)
- `default_image_mime_type() -> &str`: MIME type (default: "image/webp")

### ContentDirectory Navigation

- `root_container() -> Container`: Root container for UPnP ContentDirectory
- `browse(object_id: &str) -> BrowseResult`: Browse containers/items
- `resolve_uri(object_id: &str) -> String`: Get audio URI (cached or original)

### FIFO Support (Dynamic Sources)

- `supports_fifo() -> bool`: Whether this source uses a FIFO
- `append_track(track: Item)`: Add track to FIFO (auto-removes oldest if full)
- `remove_oldest() -> Option<Item>`: Remove oldest track from FIFO

### Change Tracking

- `update_id() -> u32`: Increments on each change (for UPnP notifications)
- `last_change() -> Option<SystemTime>`: Timestamp of last modification

### Pagination & Search

- `get_items(offset: usize, count: usize) -> Vec<Item>`: Paginated browsing
- `search(query: &str) -> BrowseResult`: Search (optional, default: not supported)

## Integration with PMOMusic Ecosystem

### With pmoplaylist

Sources that support FIFO (radios, streaming services) use `pmoplaylist::FifoPlaylist` to manage dynamic track lists:

```rust
use pmoplaylist::{FifoPlaylist, Track};

let playlist = FifoPlaylist::new(
    "my-radio".to_string(),
    "My Radio".to_string(),
    50, // capacity
    DEFAULT_IMAGE,
);

// Add tracks
playlist.append_track(Track::new("id", "title", "uri")).await;

// Tracks automatically removed when capacity reached
```

### With pmoaudiocache

When the `cache` feature is enabled, sources can integrate with `pmoaudiocache` to:
- Cache audio files locally (with FLAC conversion)
- Serve from local cache instead of re-streaming
- Extract and merge metadata

```rust
// Resolve URI checks cache first
async fn resolve_uri(&self, object_id: &str) -> Result<String> {
    if let Some(cached_pk) = self.get_cached_pk(object_id).await {
        Ok(format!("{}/audio/cache/{}", self.cache_base_url, cached_pk))
    } else {
        Ok(self.get_original_uri(object_id))
    }
}
```

### With pmocovers

When the `cache` feature is enabled, sources can integrate with `pmocovers` to:
- Cache album art locally (with WebP conversion)
- Generate multiple size variants
- Serve optimized images

### With pmodidl

All sources use `pmodidl` for DIDL-Lite generation compatible with UPnP/DLNA.

## Examples

### Radio Paradise

See [examples/radio_paradise.rs](examples/radio_paradise.rs) for a complete implementation of a streaming radio source with:
- FIFO management using `pmoplaylist`
- Simulated cache integration
- Full DIDL-Lite export
- Change tracking

Run the example:

```bash
cargo run --example radio_paradise
```

## Design Patterns

### Static Sources (Albums, Local Playlists)

```rust
impl MusicSource for LocalAlbum {
    fn supports_fifo(&self) -> bool {
        false  // Static content
    }

    async fn append_track(&self, _: Item) -> Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        0  // Never changes
    }
}
```

### Dynamic Sources (Radios, Streaming Services)

```rust
impl MusicSource for RadioSource {
    fn supports_fifo(&self) -> bool {
        true  // Dynamic content
    }

    async fn append_track(&self, track: Item) -> Result<()> {
        // Add to pmoplaylist::FifoPlaylist
        self.playlist.append_track(converted_track).await;
        Ok(())
    }

    async fn update_id(&self) -> u32 {
        self.playlist.update_id().await
    }
}
```

## Thread Safety

All `MusicSource` implementations must be `Send + Sync` for use in async servers.

## License

MIT OR Apache-2.0
