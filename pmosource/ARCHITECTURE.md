# PMOSource Architecture

This document describes the architecture and design decisions for the `pmosource` crate.

## Overview

`pmosource` provides a unified abstraction layer for all music sources in the PMOMusic ecosystem. It defines the `MusicSource` trait that all concrete music sources (Radio Paradise, Qobuz, local playlists, etc.) must implement.

## Design Goals

1. **Unified Interface**: Single trait for all music source types
2. **UPnP/OpenHome Compatible**: Support ContentDirectory browsing and DIDL-Lite
3. **Cache Integration**: Seamless integration with `pmoaudiocache` and `pmocovers`
4. **Change Tracking**: Support for UPnP event notifications via `update_id` and `last_change`
5. **FIFO Support**: Dynamic sources (radios) can manage track queues
6. **Thread Safety**: All sources must be `Send + Sync` for async servers
7. **No Network Code**: Pure abstraction layer, no HTTP/network implementation

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        PMOMusic Server                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │              MusicSource Registry                         │ │
│  │  - Manage multiple sources                                │ │
│  │  - Aggregate content for ContentDirectory                 │ │
│  │  - Handle browse/search requests                          │ │
│  └───────────────────────────────────────────────────────────┘ │
│                            │                                    │
│         ┌──────────────────┼──────────────────┐                │
│         │                  │                  │                │
│    ┌────▼────┐       ┌─────▼────┐      ┌─────▼────┐           │
│    │ Radio   │       │  Qobuz   │      │  Local   │           │
│    │Paradise │       │  Source  │      │ Playlist │           │
│    └────┬────┘       └─────┬────┘      └─────┬────┘           │
│         │                  │                  │                │
│         └──────────────────┼──────────────────┘                │
│                            │                                    │
│               implements MusicSource trait                      │
└─────────────────────────────┬───────────────────────────────────┘
                              │
         ┌────────────────────┴────────────────────┐
         │                                         │
    ┌────▼─────┐                            ┌─────▼──────┐
    │pmoplaylist│                            │  pmodidl   │
    │   FIFO    │                            │ DIDL-Lite  │
    └──────────┘                            └────────────┘
         │                                         │
    ┌────▼─────────┐                         ┌────▼────────┐
    │pmoaudiocache │                         │  pmocovers  │
    │ Audio files  │                         │   Images    │
    └──────────────┘                         └─────────────┘
```

## Core Trait: `MusicSource`

The `MusicSource` trait is divided into 5 logical sections:

### 1. Basic Information

```rust
fn name(&self) -> &str;
fn id(&self) -> &str;
fn default_image(&self) -> &[u8];
fn default_image_mime_type(&self) -> &str;
```

These methods provide basic metadata about the source:
- **name**: Human-readable display name
- **id**: Unique identifier for routing and container IDs
- **default_image**: Embedded WebP logo (300x300px)
- **default_image_mime_type**: Always "image/webp"

### 2. ContentDirectory Navigation

```rust
async fn root_container(&self) -> Result<Container>;
async fn browse(&self, object_id: &str) -> Result<BrowseResult>;
async fn resolve_uri(&self, object_id: &str) -> Result<String>;
```

These methods support UPnP ContentDirectory Service:
- **root_container**: Returns the top-level container for this source
- **browse**: Returns children of a given container (sub-containers or items)
- **resolve_uri**: Resolves the actual streaming URI for a track (checks caches)

### 3. FIFO Management

```rust
fn supports_fifo(&self) -> bool;
async fn append_track(&self, track: Item) -> Result<()>;
async fn remove_oldest(&self) -> Result<Option<Item>>;
```

For dynamic sources (radios, streaming services):
- **supports_fifo**: Indicates if source uses a FIFO queue
- **append_track**: Adds track to queue (auto-removes oldest if capacity reached)
- **remove_oldest**: Manually removes oldest track

### 4. Change Tracking

```rust
async fn update_id(&self) -> u32;
async fn last_change(&self) -> Option<SystemTime>;
```

For UPnP event notifications:
- **update_id**: Counter incremented on each change (wraps around)
- **last_change**: Timestamp of last modification

### 5. Pagination & Search

```rust
async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>>;
async fn search(&self, query: &str) -> Result<BrowseResult>;
```

For efficient browsing and searching:
- **get_items**: Paginated access to items
- **search**: Optional search (default: not supported)

## Source Types

### Dynamic Sources (with FIFO)

Examples: Radio Paradise, streaming radios, live playlists

**Characteristics:**
- `supports_fifo() = true`
- Uses `pmoplaylist::FifoPlaylist` internally
- `update_id` changes when tracks are added/removed
- Limited capacity (e.g., last 50 tracks)
- Items have dynamic URIs that may change

**Implementation Pattern:**

```rust
struct RadioSource {
    playlist: FifoPlaylist,
    track_cache: RwLock<HashMap<String, (String, Option<String>)>>,
}

impl MusicSource for RadioSource {
    fn supports_fifo(&self) -> bool {
        true
    }

    async fn append_track(&self, track: Item) -> Result<()> {
        // Convert Item to pmoplaylist::Track
        // Add to playlist
        self.playlist.append_track(pmo_track).await;
        Ok(())
    }

    async fn update_id(&self) -> u32 {
        self.playlist.update_id().await
    }
}
```

### Static Sources (without FIFO)

Examples: Local albums, fixed playlists, Qobuz albums

**Characteristics:**
- `supports_fifo() = false`
- `append_track()` returns `FifoNotSupported` error
- `update_id` is constant (0)
- `last_change()` may be None
- Items have stable URIs

**Implementation Pattern:**

```rust
struct AlbumSource {
    items: Vec<Item>,
}

impl MusicSource for AlbumSource {
    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _: Item) -> Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        0  // Never changes
    }
}
```

## Integration with PMOMusic Ecosystem

### pmoplaylist Integration

`pmoplaylist` provides the `FifoPlaylist` struct for managing dynamic track lists:

```rust
use pmoplaylist::{FifoPlaylist, Track};

let playlist = FifoPlaylist::new(
    "radio-id".to_string(),
    "Radio Name".to_string(),
    50,  // capacity
    DEFAULT_IMAGE,
);

// Add tracks
playlist.append_track(Track::new("id", "title", "uri")).await;

// Get tracks
let tracks = playlist.get_items(0, 10).await;

// Track changes
let update_id = playlist.update_id().await;
let last_change = playlist.last_change().await;
```

**Benefits:**
- Automatic capacity management (FIFO behavior)
- Built-in change tracking
- Thread-safe (Arc<RwLock<>>)

### pmodidl Integration

All sources use `pmodidl` for DIDL-Lite generation:

```rust
use pmodidl::{Container, Item, Resource};

// Containers for browsing
let container = Container {
    id: "source-id".to_string(),
    parent_id: "0".to_string(),
    title: "My Source".to_string(),
    class: "object.container.playlistContainer".to_string(),
    child_count: Some("10".to_string()),
    containers: vec![],
    items: vec![],
};

// Items for tracks
let item = Item {
    id: "track-1".to_string(),
    parent_id: "source-id".to_string(),
    title: "Track Title".to_string(),
    artist: Some("Artist".to_string()),
    class: "object.item.audioItem.musicTrack".to_string(),
    resources: vec![Resource {
        url: "http://server/audio/track-1".to_string(),
        protocol_info: "http-get:*:audio/flac:*".to_string(),
        duration: Some("0:03:45".to_string()),
        ..Default::default()
    }],
    ..Default::default()
};
```

### pmoaudiocache Integration

Sources can use `pmoaudiocache` to cache audio files locally:

```rust
async fn resolve_uri(&self, object_id: &str) -> Result<String> {
    // Check if track is cached
    if let Some(cached_pk) = self.get_cached_pk(object_id).await {
        // Return cached URI (local FLAC file)
        Ok(format!("{}/audio/cache/{}", self.cache_base_url, cached_pk))
    } else {
        // Return original streaming URI
        Ok(self.get_original_uri(object_id))
    }
}
```

**Benefits:**
- Local caching of streamed audio
- Automatic FLAC conversion
- Metadata extraction and merging
- Reduced bandwidth usage

### pmocovers Integration

Sources can use `pmocovers` to cache album art:

```rust
// Store cover art PK in track metadata
let album_art_url = format!("{}/covers/images/{}", base_url, cover_pk);

let item = Item {
    album_art: Some(album_art_url),
    ..Default::default()
};
```

**Benefits:**
- Local caching of album art
- Automatic WebP conversion
- Multiple size variants
- Optimized delivery

## Error Handling

All fallible operations return `pmosource::Result<T>`:

```rust
pub enum MusicSourceError {
    ImageLoadError(String),
    InvalidImageFormat(String),
    SourceUnavailable(String),
    ObjectNotFound(String),
    BrowseError(String),
    SearchNotSupported,
    FifoNotSupported,
    CacheError(String),
    UriResolutionError(String),
}
```

**Guidelines:**
- Use `ObjectNotFound` for invalid object IDs
- Use `BrowseError` for general browsing failures
- Use `SearchNotSupported` for sources without search
- Use `FifoNotSupported` for static sources
- Use `CacheError` for cache-related issues

## Thread Safety

All `MusicSource` implementations must be `Send + Sync`:

```rust
pub trait MusicSource: Debug + Send + Sync {
    // ...
}
```

**Reasoning:**
- Sources may be shared across multiple async tasks
- UPnP server handles concurrent requests
- `Arc<dyn MusicSource>` enables efficient sharing

**Implementation:**
- Use `Arc<RwLock<>>` for mutable state
- Use `tokio::sync::RwLock` for async operations
- Avoid `Rc`, `RefCell`, or other non-thread-safe types

## Testing Strategy

### Unit Tests

Test each method independently:

```rust
#[tokio::test]
async fn test_root_container() {
    let source = MySource::new();
    let root = source.root_container().await.unwrap();
    assert_eq!(root.id, "my-source");
}
```

### Integration Tests

Test complete workflows:

```rust
#[tokio::test]
async fn test_browse_and_resolve() {
    let source = MySource::new();
    let result = source.browse("container-1").await.unwrap();
    for item in result.items() {
        let uri = source.resolve_uri(&item.id).await.unwrap();
        assert!(uri.starts_with("http://"));
    }
}
```

### Example Tests

Run examples as integration tests:

```bash
cargo run --example radio_paradise
```

## Future Enhancements

Potential additions to the trait:

1. **Authentication**:
   ```rust
   async fn authenticate(&mut self, credentials: Credentials) -> Result<()>;
   fn is_authenticated(&self) -> bool;
   ```

2. **Quality Levels**:
   ```rust
   fn available_qualities(&self) -> Vec<Quality>;
   async fn set_quality(&mut self, quality: Quality) -> Result<()>;
   ```

3. **Favorites/Bookmarks**:
   ```rust
   async fn add_favorite(&self, object_id: &str) -> Result<()>;
   async fn list_favorites(&self) -> Result<Vec<Item>>;
   ```

4. **Recommendations**:
   ```rust
   async fn get_recommendations(&self) -> Result<Vec<Item>>;
   ```

## Design Decisions

### Why async-trait?

- Native async traits don't support trait objects yet
- `async-trait` provides a clean macro-based solution
- Minimal performance overhead with good compiler optimizations

### Why separate FIFO methods?

- Clear distinction between dynamic and static sources
- Static sources can return `FifoNotSupported` immediately
- Allows future optimizations for FIFO-specific operations

### Why BrowseResult enum?

- Different sources return different types of results
- Some return only containers, some only items, some mixed
- Enum provides type-safe representation of all cases

### Why separate resolve_uri?

- Caching is a cross-cutting concern
- Separating resolution from browsing allows flexible caching strategies
- URI resolution may be expensive (check cache, fallback to original)

## Performance Considerations

1. **Caching**: Always check local caches before streaming
2. **Pagination**: Use `get_items(offset, count)` for large collections
3. **Lazy Loading**: Don't load all metadata upfront
4. **Arc Sharing**: Use `Arc<dyn MusicSource>` to avoid cloning
5. **RwLock Usage**: Prefer read locks when possible

## Versioning

The crate follows Semantic Versioning:

- **MAJOR**: Breaking changes to `MusicSource` trait
- **MINOR**: New trait methods (with default implementations)
- **PATCH**: Bug fixes, documentation, internal changes

Current version: **0.2.0**
