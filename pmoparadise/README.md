# pmoparadise

[![Crates.io](https://img.shields.io/crates/v/pmoparadise.svg)](https://crates.io/crates/pmoparadise)
[![Documentation](https://docs.rs/pmoparadise/badge.svg)](https://docs.rs/pmoparadise)
[![License](https://img.shields.io/crates/l/pmoparadise.svg)](https://github.com/yourusername/pmomusic)

An idiomatic Rust client library for [Radio Paradise](https://radioparadise.com) streaming service.

## Features

- 🎵 **Metadata Access** - Fetch current and historical block metadata with song information
- 📡 **Block Streaming** - Stream continuous FLAC/AAC blocks with automatic prefetching
- 🎚️ **Multiple Quality Levels** - Support for MP3, AAC (64/128/320 kbps), and FLAC lossless
- 🎼 **Per-Track Extraction** (optional) - Extract individual tracks from FLAC blocks
- ⚡ **Async/Await** - Built on tokio for efficient async I/O
- 🛡️ **Type-Safe** - Strongly typed API with comprehensive error handling
- 📚 **Well Documented** - Extensive API documentation and examples

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
pmoparadise = "0.1.0"
```

For per-track extraction support:

```toml
[dependencies]
pmoparadise = { version = "0.1.0", features = ["per-track"] }
```

## Quick Start

```rust
use pmoparadise::RadioParadiseClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = RadioParadiseClient::new().await?;

    // Get what's currently playing
    let now_playing = client.now_playing().await?;

    if let Some(song) = &now_playing.current_song {
        println!("Now Playing: {} - {}", song.artist, song.title);
        println!("Album: {}", song.album);
    }

    Ok(())
}
```

## Usage Examples

### Display Current Block Information

```rust
use pmoparadise::RadioParadiseClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioParadiseClient::new().await?;
    let block = client.get_block(None).await?;

    println!("Block {} contains {} songs", block.event, block.song_count());

    for (index, song) in block.songs_ordered() {
        println!("{}. {} - {} ({}s)",
                 index + 1,
                 song.artist,
                 song.title,
                 song.duration / 1000);
    }

    Ok(())
}
```

### Stream a Block

```rust
use pmoparadise::RadioParadiseClient;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioParadiseClient::new().await?;
    let block = client.get_block(None).await?;

    let mut stream = client.stream_block_from_metadata(&block).await?;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        // Feed to audio player, write to file, etc.
        println!("Received {} bytes", bytes.len());
    }

    Ok(())
}
```

### Configure Quality Level

```rust
use pmoparadise::{RadioParadiseClient, Bitrate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioParadiseClient::builder()
        .bitrate(Bitrate::Aac320)  // Use AAC 320 kbps
        .build()
        .await?;

    Ok(())
}
```

### Continuous Playback with Prefetching

```rust
use pmoparadise::RadioParadiseClient;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = RadioParadiseClient::new().await?;
    let mut current_block = client.get_block(None).await?;

    loop {
        println!("Playing block {}", current_block.event);

        // Prefetch next block
        client.prefetch_next(&current_block).await?;

        // Stream current block
        let mut stream = client.stream_block_from_metadata(&current_block).await?;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            // Send to audio player
        }

        // Move to next block
        current_block = client.get_block(Some(current_block.end_event)).await?;
    }
}
```

## Quality Levels

Radio Paradise offers 5 quality levels via the `Bitrate` enum:

| Bitrate | Format | Description |
|---------|--------|-------------|
| `Mp3_128` | MP3 | 128 kbps MP3 |
| `Aac64` | AAC | 64 kbps AAC |
| `Aac128` | AAC | 128 kbps AAC |
| `Aac320` | AAC | 320 kbps AAC |
| `Flac` | FLAC | Lossless (default) |

## Per-Track Extraction

**⚠️ Important**: This feature has significant tradeoffs. See details below.

### The Problem

Radio Paradise publishes *blocks* containing multiple songs, not individual per-track files. Each block is a single FLAC or AAC file with metadata indicating timing offsets for each song.

Block URL pattern:
```
https://apps.radioparadise.com/blocks/chan/0/4/<start_event>-<end_event>.flac
```

The `song[i].elapsed` field (in milliseconds) indicates when each track starts within the block.

### Recommended Approach: Player-Based Seeking

For most use cases, let your audio player handle seeking:

```bash
# Play a specific track using mpv
mpv --start=123.5 --length=234.0 <block_url>

# Extract a track using ffmpeg
ffmpeg -ss 123.5 -t 234.0 -i <block_url> -c copy track.flac
```

Get timing information from the API:

```rust
let client = RadioParadiseClient::new().await?;
let block = client.get_block(None).await?;

let (start_sec, duration_sec) = client.track_position_seconds(&block, 0)?;
println!("mpv --start={} --length={} {}", start_sec, duration_sec, block.url);
```

**Benefits of player-based seeking:**
- ✅ No need to download entire block
- ✅ Uses player's optimized seeking
- ✅ Starts playback immediately
- ✅ Preserves original quality
- ✅ Minimal CPU usage

### Alternative: FLAC Decoding (Feature: `per-track`)

If you need PCM samples or WAV files for processing:

```rust
use pmoparadise::RadioParadiseClient;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioParadiseClient::new().await?;
    let block = client.get_block(None).await?;

    // Extract first track to WAV
    let mut track = client.open_track_stream(&block, 0).await?;
    track.export_wav(Path::new("track.wav"))?;

    Ok(())
}
```

**Tradeoffs:**
- ❌ Downloads entire block (50-100 MB) to temporary file
- ❌ High latency before playback can start
- ❌ CPU-intensive FLAC decoding
- ❌ FLAC doesn't support random access (must decode from beginning)

**When to use:**
- You need individual WAV files for further processing
- You need raw PCM data for custom audio analysis
- You need separate files for non-streaming scenarios

## Radio Paradise Block Format

Understanding the block format is essential for working with Radio Paradise:

### Block Structure

- Each block is a single audio file (FLAC or AAC)
- Blocks contain multiple songs (typically 10-15 minutes total)
- Metadata includes timing offsets for each song (`song[i].elapsed` in ms)
- Blocks are seamlessly chained: `block_n.end_event == block_n+1.event`

### Block Metadata Example

```json
{
  "event": 1234,
  "end_event": 5678,
  "length": 900000,
  "url": "https://apps.radioparadise.com/blocks/chan/0/4/1234-5678.flac",
  "image_base": "https://img.radioparadise.com/covers/l/",
  "song": {
    "0": {
      "artist": "Miles Davis",
      "title": "So What",
      "album": "Kind of Blue",
      "year": 1959,
      "elapsed": 0,
      "duration": 540000,
      "cover": "B00000I0JF.jpg"
    },
    "1": {
      "artist": "John Coltrane",
      "title": "Giant Steps",
      "album": "Giant Steps",
      "year": 1960,
      "elapsed": 540000,
      "duration": 360000,
      "cover": "B000002I4U.jpg"
    }
  }
}
```

### Timing Information

- `event`: Start event ID for this block
- `end_event`: End event ID (= start of next block)
- `length`: Total duration in milliseconds
- `song[i].elapsed`: Start time of song `i` in milliseconds
- `song[i].duration`: Duration of song `i` in milliseconds

## Best Practices

### For Continuous Playback

1. Fetch current block with `get_block(None)`
2. Start streaming the block
3. Call `prefetch_next()` early (before block ends)
4. When block finishes, seamlessly transition to next block
5. Repeat

### For Gapless Playback

- Use the `end_event` to fetch the next block
- Prefetch metadata and prepare the stream before the current block ends
- Modern audio players (mpv, VLC) handle gapless FLAC natively

### For User Controls (Skip Track)

**Recommended**: Stream entire block to player, use player's seek commands:
```rust
let (start, duration) = client.track_position_seconds(&block, track_index)?;
// Send seek command to player
```

**Alternative**: Re-stream from a different block or position

### Network Best Practices

- Set appropriate User-Agent: `RadioParadiseClient::builder().user_agent("MyApp/1.0")`
- Implement retry logic with exponential backoff
- Respect Radio Paradise's infrastructure (no excessive parallel streams)
- Cache block metadata locally to reduce API calls

## Error Handling

All operations return `Result<T, Error>` with detailed error types:

```rust
use pmoparadise::{RadioParadiseClient, Error};

match client.get_block(Some(12345)).await {
    Ok(block) => println!("Got block: {}", block.event),
    Err(Error::Http(e)) => eprintln!("Network error: {}", e),
    Err(Error::Json(e)) => eprintln!("Parse error: {}", e),
    Err(Error::InvalidEvent(e)) => eprintln!("Invalid event: {}", e),
    Err(e) => eprintln!("Other error: {}", e),
}
```

Available error types:
- `Http` - Network/HTTP errors
- `Json` - JSON parsing errors
- `InvalidUrl` - URL parsing errors
- `Io` - File I/O errors
- `InvalidIndex` - Invalid track index
- `InvalidBitrate` - Invalid quality level
- `InvalidEvent` - Invalid event ID
- `FlacDecode` - FLAC decoding errors (per-track feature)
- `WavEncode` - WAV encoding errors (per-track feature)
- `Timeout` - Request timeout
- `Other` - Generic errors

## Cargo Features

- **`default = ["metadata-only"]`** - Standard metadata and streaming (no FLAC decoding)
- **`per-track`** - Enable FLAC decoding and per-track extraction (adds dependencies: `claxon`, `hound`, `tempfile`)
- **`logging`** - Enable tracing logs for debugging

## Examples

Run examples with:

```bash
# Display current block and songs
cargo run --example now_playing

# Stream a block to stdout (pipe to player)
cargo run --example stream_block | mpv -

# Extract individual tracks (requires per-track feature)
cargo run --example extract_track --features per-track
```

## Architecture

```
pmoparadise/
├── src/
│   ├── lib.rs          # Library root and documentation
│   ├── client.rs       # HTTP client and API methods
│   ├── models.rs       # Data structures (Block, Song, etc.)
│   ├── stream.rs       # Block streaming functionality
│   ├── track.rs        # Per-track extraction (feature-gated)
│   └── error.rs        # Error types
├── examples/           # Usage examples
│   ├── now_playing.rs
│   ├── stream_block.rs
│   └── extract_track.rs
└── tests/              # Integration tests
    └── integration_tests.rs
```

## Testing

```bash
# Run all tests (metadata-only)
cargo test

# Run tests with per-track feature
cargo test --features per-track

# Run integration tests
cargo test --test integration_tests

# Run with logging
RUST_LOG=debug cargo test
```

## Requirements

- Rust 1.90+ (2021 edition)
- Tokio async runtime

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Disclaimer

This library is not affiliated with or endorsed by Radio Paradise. Please respect their [Terms of Service](https://radioparadise.com/terms) when using this library.

## Credits

Inspired by the Radio Paradise API and the Python implementation in [upmpdcli](https://www.lesbonscomptes.com/upmpdcli/).

## See Also

- [Radio Paradise](https://radioparadise.com) - Official website
- [Radio Paradise API Documentation](https://api.radioparadise.com)
- [PMOMusic](https://github.com/yourusername/pmomusic) - Parent project
