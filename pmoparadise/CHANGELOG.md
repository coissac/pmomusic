# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2024-10-12

### Added

- Initial release of pmoparadise
- Core HTTP client for Radio Paradise API
- Block metadata fetching with `get_block()` and `now_playing()`
- Five quality levels: MP3 128, AAC 64/128/320, FLAC lossless
- Block streaming with `stream_block()` and `stream_block_from_metadata()`
- Prefetching support with `prefetch_next()`
- Builder pattern for client configuration
- Strong typing for EventId, DurationMs, and Bitrate
- Comprehensive error handling with thiserror
- Optional per-track extraction (feature: `per-track`)
  - FLAC decoding with claxon
  - WAV export with hound
  - PCM sample reading
  - Helper method `track_position_seconds()` for player-based seeking
- Optional logging support (feature: `logging`)
- Complete documentation with examples
- Unit tests for data models
- Integration tests with wiremock
- Three example programs:
  - `now_playing` - Display current block and songs
  - `stream_block` - Stream a block to stdout
  - `extract_track` - Extract individual tracks (requires per-track feature)
- CI/CD with GitHub Actions
- MIT/Apache-2.0 dual licensing

### Documentation

- Comprehensive README with usage examples
- Detailed module-level documentation
- Rustdoc for all public APIs
- Implementation notes and design decisions
- Clear warnings about per-track limitations
- Best practices for continuous playback

### Architecture

- Async/await with tokio runtime
- Feature gates for optional functionality
- Builder pattern for ergonomic configuration
- Type-safe API with minimal runtime overhead
- Stream-based block downloading
- Integration-ready for PMOMusic ecosystem

## [Unreleased]

### Planned Features

- Support for additional Radio Paradise channels (mellow, rock, world)
- Historical block access by date/time
- Optional block caching layer
- WebSocket support for live updates (if API adds it)
- Performance optimizations for per-track extraction

### Known Limitations

- Per-track extraction is resource-intensive (by design)
- No built-in block caching (users implement as needed)
- No authentication support (API is public)
- FLAC seeking requires full decode (claxon limitation)

[0.1.0]: https://github.com/yourusername/pmomusic/releases/tag/pmoparadise-v0.1.0
