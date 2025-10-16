# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-01-16

### Added

- Extended `MusicSource` trait with comprehensive async methods:
  - `root_container()`: Get root container for ContentDirectory
  - `browse(object_id)`: Browse containers and items
  - `resolve_uri(object_id)`: Resolve audio URIs (with cache support)
  - `supports_fifo()`: Indicate FIFO support
  - `append_track(track)`: Add track to FIFO
  - `remove_oldest()`: Remove oldest track from FIFO
  - `update_id()`: Get current update counter
  - `last_change()`: Get last modification timestamp
  - `get_items(offset, count)`: Paginated browsing
  - `search(query)`: Optional search functionality

- New types:
  - `BrowseResult`: Enum for browse results (Containers, Items, or Mixed)
  - Extended `MusicSourceError` with more error variants

- Dependencies:
  - `async-trait`: For async trait methods
  - `tokio`: Async runtime
  - `pmodidl`: DIDL-Lite support
  - `pmoplaylist`: FIFO playlist management
  - `pmoaudiocache` (optional): Audio caching
  - `pmocovers` (optional): Cover art caching

- Complete Radio Paradise example (`examples/radio_paradise.rs`) demonstrating:
  - FIFO management using `pmoplaylist`
  - Cache integration simulation
  - DIDL-Lite generation
  - Change tracking
  - Full trait implementation

- Comprehensive documentation:
  - Updated README with architecture diagrams
  - Usage examples for static and dynamic sources
  - Integration guides for PMOMusic ecosystem
  - Thread safety notes

### Changed

- `MusicSource` trait is now async (requires `#[async_trait]`)
- All implementations must be `Send + Sync`
- Trait is now much more comprehensive and ready for UPnP/OpenHome integration

### Removed

- Outdated `show_sources.rs` example

## [0.1.0] - Initial Release

### Added

- Basic `MusicSource` trait with:
  - `name()`: Human-readable name
  - `id()`: Unique identifier
  - `default_image()`: Embedded WebP logo
  - `default_image_mime_type()`: MIME type
- Basic error types
- Standard image size constant (300x300px)
