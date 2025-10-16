# PMOSource Examples

This directory contains example implementations of the `MusicSource` trait.

## Available Examples

### radio_paradise.rs

A complete implementation of a streaming radio source demonstrating:

- **FIFO Management**: Using `pmoplaylist::FifoPlaylist` for dynamic track management
- **Cache Integration**: Simulated integration with `pmoaudiocache` and `pmocovers`
- **DIDL-Lite Export**: Proper conversion between `pmoplaylist::Track` and `pmodidl::Item`
- **Change Tracking**: `update_id` and `last_change` for UPnP notifications
- **URI Resolution**: Dynamic URI resolution with cache support
- **Pagination**: Efficient browsing with `get_items(offset, count)`

#### Running the Example

```bash
cargo run --example radio_paradise
```

#### Expected Output

```
Radio Paradise Source Example
==============================

Source: Radio Paradise
ID: radio-paradise
Supports FIFO: true
Default image size: 9774 bytes

Adding sample tracks...
Added 3 tracks

Root Container:
  ID: radio-paradise
  Title: Radio Paradise
  Child Count: Some("3")

Browsing tracks:
  - Wish You Were Here by Pink Floyd (Wish You Were Here)
  - Bohemian Rhapsody by Queen (A Night at the Opera)
  - Hotel California by Eagles (Hotel California)

Resolving URIs:
  rp-001: http://stream.radioparadise.com/rp-001.mp3
  rp-002: http://stream.radioparadise.com/rp-002.mp3
  rp-003: http://stream.radioparadise.com/rp-003.mp3

Change Tracking:
  Update ID: 3
  Last Change: SystemTime { ... }

Simulating cache for rp-001...
  Cached URI: http://localhost:8080/audio/cache/cached-abc123

Pagination (get items 1-2):
  - Bohemian Rhapsody
  - Hotel California

Removing oldest track...
  Removed: Wish You Were Here
  New Update ID: 4

Browsing after removal:
  Tracks remaining: 2
    - Bohemian Rhapsody
    - Hotel California
```

## Creating Your Own Source

### 1. Define the Source Structure

```rust
use pmosource::{async_trait, MusicSource, BrowseResult, Result};
use pmodidl::{Container, Item};
use pmoplaylist::FifoPlaylist;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct MySource {
    inner: Arc<MySourceInner>,
}

struct MySourceInner {
    // For dynamic sources:
    playlist: FifoPlaylist,

    // For static sources:
    // items: Vec<Item>,

    // Other fields as needed
}
```

### 2. Implement Basic Information

```rust
#[async_trait]
impl MusicSource for MySource {
    fn name(&self) -> &str {
        "My Source Name"
    }

    fn id(&self) -> &str {
        "my-source"
    }

    fn default_image(&self) -> &[u8] {
        include_bytes!("../assets/my-source.webp")
    }
}
```

### 3. Implement ContentDirectory Methods

```rust
    async fn root_container(&self) -> Result<Container> {
        Ok(Container {
            id: self.id().to_string(),
            parent_id: "0".to_string(),
            title: self.name().to_string(),
            class: "object.container.playlistContainer".to_string(),
            child_count: Some("0".to_string()),
            containers: vec![],
            items: vec![],
        })
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        // Return items for this container
        Ok(BrowseResult::Items(vec![]))
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        // Return URI for track
        Ok(format!("http://example.com/{}", object_id))
    }
```

### 4. Implement FIFO Methods (if applicable)

```rust
    fn supports_fifo(&self) -> bool {
        true  // or false for static sources
    }

    async fn append_track(&self, track: Item) -> Result<()> {
        // For dynamic sources: convert and add to playlist
        // For static sources: return FifoNotSupported error
        Ok(())
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        // For dynamic sources: remove from playlist
        // For static sources: return FifoNotSupported error
        Ok(None)
    }
```

### 5. Implement Change Tracking

```rust
    async fn update_id(&self) -> u32 {
        // For dynamic sources: delegate to playlist
        // For static sources: return 0
        0
    }

    async fn last_change(&self) -> Option<std::time::SystemTime> {
        // Return timestamp of last modification
        None
    }
```

### 6. Implement Pagination

```rust
    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        // Return paginated items
        Ok(vec![])
    }
```

### 7. Implement Search (optional)

```rust
    async fn search(&self, query: &str) -> Result<BrowseResult> {
        // If search is not supported:
        Err(pmosource::MusicSourceError::SearchNotSupported)

        // If search is supported:
        // let results = self.search_items(query)?;
        // Ok(BrowseResult::Items(results))
    }
```

## Best Practices

### Thread Safety

Always use `Arc<RwLock<>>` for mutable state:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

struct MySourceInner {
    state: RwLock<HashMap<String, Track>>,
}
```

### Error Handling

Use appropriate error types:

```rust
if object_id_not_found {
    return Err(MusicSourceError::ObjectNotFound(object_id.to_string()));
}
```

### Manual Debug Implementation

If your source contains non-Debug types, implement Debug manually:

```rust
impl std::fmt::Debug for MySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySource")
            .field("name", &self.name())
            .finish()
    }
}
```

### Testing

Create comprehensive tests:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let source = MySource::new();

    // Test basic info
    println!("Source: {}", source.name());

    // Test browsing
    let result = source.browse("root").await?;
    println!("Items: {}", result.count());

    // Test URI resolution
    let uri = source.resolve_uri("track-1").await?;
    println!("URI: {}", uri);

    Ok(())
}
```

## Further Reading

- [Main README](../README.md): Overview and quick start
- [ARCHITECTURE.md](../ARCHITECTURE.md): Detailed architecture documentation
- [CHANGELOG.md](../CHANGELOG.md): Version history and changes
