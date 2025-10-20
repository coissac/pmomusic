# Implementation Notes and Design Decisions

## Overview

`pmoparadise` is a Rust client library for Radio Paradise's streaming API, designed following idiomatic Rust patterns and inspired by the structure of `pmoqobuz`.

## Architecture Decisions

### 1. Module Structure

The crate is organized into focused modules:
- `client.rs` - HTTP client and API methods
- `models.rs` - Data structures with serde serialization
- `stream.rs` - Block streaming functionality
- `track.rs` - Per-track extraction (feature-gated)
- `error.rs` - Type-safe error handling

This separation ensures clear boundaries and makes the code maintainable.

### 2. Async/Await with Tokio

**Decision**: Use async/await throughout the API with tokio runtime.

**Rationale**:
- Radio Paradise API calls are I/O bound
- Streaming large FLAC blocks benefits from async I/O
- Tokio is the de facto standard for async Rust
- Enables efficient prefetching and concurrent operations

### 3. Type Safety

**Decision**: Use strong typing for all API concepts (EventId, DurationMs, Bitrate enum).

**Rationale**:
- Prevents mixing up event IDs with durations
- Enum for Bitrate makes invalid states unrepresentable
- Compile-time guarantees reduce runtime errors
- Self-documenting code

### 4. Error Handling

**Decision**: Use `thiserror` for structured errors with specific variants.

**Rationale**:
- Users can match on specific error types
- Better error messages than strings
- Idiomatic Rust error handling
- Easy to extend with new error types

### 5. Feature Gates

**Decision**: Gate the per-track functionality behind a feature flag.

**Rationale**:
- Most users don't need FLAC decoding
- Reduces dependencies for common use cases
- `claxon`, `hound`, `tempfile` add significant compile time
- Keeps the default build lightweight

## API Design Decisions

### 1. Builder Pattern for Client

**Decision**: Provide both `new()` and `builder()` methods.

**Rationale**:
- `new()` for simple cases (good defaults)
- `builder()` for customization (bitrate, proxy, timeout)
- Common Rust pattern (reqwest, etc.)
- Extensible without breaking changes

### 2. Block-Centric API

**Decision**: Focus on blocks as the primary abstraction, not individual songs.

**Rationale**:
- Matches Radio Paradise's actual architecture
- Blocks are the unit of streaming
- Enables efficient prefetching
- Transparent about implementation details

### 3. Prefetching Support

**Decision**: Provide explicit `prefetch_next()` method rather than automatic prefetching.

**Rationale**:
- Gives users control over when network calls happen
- Allows batching metadata requests
- Simpler to reason about
- Users can implement custom prefetch strategies

### 4. Stream Trait Implementation

**Decision**: Return a custom `BlockStream` that implements `Stream<Item = Result<Bytes>>`.

**Rationale**:
- Standard Rust async iterator pattern
- Compatible with futures combinators
- Easy to consume with `while let Some(chunk) = stream.next().await`
- Can be piped to any sink

## Per-Track Feature Decisions

### 1. Why It's Optional and Discouraged

**Decision**: Document limitations and recommend player-based seeking.

**Rationale**:
- FLAC doesn't support random access
- Must download entire block (50-100 MB)
- CPU-intensive decoding
- Players (mpv, ffmpeg) handle this better

**Trade-offs**:
- **Prefetch vs Per-Track**:
  - Prefetch: Low latency, efficient, recommended
  - Per-track: High latency, resource-intensive, only for special cases

### 2. Implementation Approach

**Decision**: Download to tempfile, decode with claxon, expose PCM/WAV.

**Rationale**:
- Claxon is pure Rust (no C dependencies)
- Tempfile ensures cleanup
- WAV export is a common use case
- Simple implementation

**Alternatives Considered**:
- **Streaming decode**: Too complex, claxon doesn't support seeking
- **HTTP range requests**: Radio Paradise blocks don't support it reliably
- **Caching decoded blocks**: Too much memory

### 3. Helper Method for Players

**Decision**: Provide `track_position_seconds()` to get timing for external players.

**Rationale**:
- Gives users the information they need
- Doesn't dictate how to use it
- Works with any player
- Zero overhead

## Data Model Decisions

### 1. HashMap for Songs

**Decision**: Use `HashMap<String, Song>` matching the API response.

**Rationale**:
- Matches JSON structure exactly
- Easy serde deserialization
- Provides `songs_ordered()` helper for iteration
- Preserves all data from API

### 2. Optional Fields

**Decision**: Make many fields `Option<T>` (year, rating, cover, etc.).

**Rationale**:
- API doesn't always provide all fields
- Future-proof against API changes
- Explicit about what's guaranteed

### 3. Extra Fields

**Decision**: Use `#[serde(flatten)]` for unknown fields.

**Rationale**:
- Forwards compatibility
- Don't break on new API fields
- Can inspect raw data if needed

## Testing Strategy

### 1. Unit Tests

- Inline tests for data model parsing
- Tests for timing calculations
- Builder pattern validation

### 2. Integration Tests with Mocks

**Decision**: Use `wiremock` for HTTP mocking.

**Rationale**:
- Don't hit real API in CI
- Reproducible tests
- Fast execution
- Can test error conditions

### 3. Example Programs

**Decision**: Provide runnable examples for all major features.

**Rationale**:
- Examples serve as documentation
- Users can copy-paste working code
- Tested in CI (via `cargo test --doc`)

## Documentation Strategy

### 1. Extensive Rustdoc

**Decision**: Document every public function, struct, and enum.

**Rationale**:
- Discoverability via docs.rs
- IDE autocomplete shows docs
- Examples in docs are tested
- Professional appearance

### 2. README with Use Cases

**Decision**: Detailed README covering common scenarios.

**Rationale**:
- First thing users see
- Explains design decisions
- Guides users to best practices
- Warns about per-track limitations

### 3. Module-Level Documentation

**Decision**: Each module has overview documentation.

**Rationale**:
- Explains purpose of module
- Links to related modules
- Top-down understanding

## Performance Considerations

### 1. Streaming vs Downloading

- **Streaming** (`stream_block`): Low latency, constant memory
- **Downloading** (`download_block`): Required for per-track, high memory

### 2. Prefetching

- Metadata prefetch is cheap (~1KB JSON)
- Block prefetch is expensive (~50-100MB)
- Leave block caching to users

### 3. Connection Pooling

**Decision**: Allow sharing `reqwest::Client`.

**Rationale**:
- Reuse connections
- User controls connection pool size
- Works with existing infrastructure

## Future Extensions

### Possible Additions (Not Implemented)

1. **Channel Support**: Main mix, mellow, rock, world (API supports this)
2. **Historical Blocks**: Fetch blocks by date/time
3. **Playlist API**: If Radio Paradise adds it
4. **WebSocket Live Updates**: Real-time now-playing updates
5. **Caching Layer**: Optional disk cache for blocks

### Why Not Included Now

- Keep initial release focused
- No user demand yet
- Can add without breaking changes
- Some features may require API changes

## Lessons Learned

### What Worked Well

1. **Builder pattern**: Easy to extend
2. **Feature gates**: Keeps default build fast
3. **Strong typing**: Caught many bugs at compile time
4. **Integration tests**: Gave confidence in refactoring

### What Could Be Improved

1. **FLAC seeking**: Claxon limitations make per-track expensive
2. **Error messages**: Could be more actionable
3. **Examples**: Could add more advanced patterns

## Comparison with pmoqobuz

### Similarities

- Builder pattern for client
- Serde models
- Async/await
- Integration with PMOMusic ecosystem

### Differences

- **No caching layer**: Radio Paradise API is simpler, less need
- **Streaming focus**: Qobuz is track-based, Paradise is block-based
- **No authentication**: Paradise API is public (for metadata)
- **Feature gates**: Paradise has optional FLAC decoding

## Conclusion

This implementation prioritizes:
1. **Ergonomics**: Easy for common cases, flexible for advanced
2. **Performance**: Async, streaming, minimal allocations
3. **Safety**: Type-safe, comprehensive error handling
4. **Documentation**: Extensive docs and examples
5. **Honesty**: Clear about limitations (per-track)

The result is a production-ready library that's pleasant to use and maintains high code quality standards.
