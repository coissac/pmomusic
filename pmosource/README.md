# pmosource

Common traits and types for PMOMusic sources.

## Overview

`pmosource` provides the foundational abstractions for different music sources in the PMOMusic ecosystem, such as Radio Paradise, Qobuz, and potentially others in the future.

## Features

- **`MusicSource` trait**: Common interface for all music sources
- **Default images**: Standardized 300x300px WebP images embedded in binaries
- **Source identification**: Consistent naming and ID scheme

## Usage

### Implementing the trait

```rust
use pmosource::MusicSource;

const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

#[derive(Debug)]
pub struct MyMusicSource;

impl MusicSource for MyMusicSource {
    fn name(&self) -> &str {
        "My Music Service"
    }

    fn id(&self) -> &str {
        "my-music-service"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }
}
```

### Using a music source

```rust
use pmosource::MusicSource;
use pmoparadise::RadioParadiseSource;
use pmoqobuz::QobuzSource;

let rp = RadioParadiseSource;
let qobuz = QobuzSource;

println!("Source: {} ({})", rp.name(), rp.id());
println!("Image size: {} bytes", rp.default_image().len());
```

## Image Format

All default images should be:
- **Format**: WebP
- **Dimensions**: 300x300 pixels (square)
- **Quality**: 85 (good balance between size and quality)
- **Location**: `<crate>/assets/default.webp`

### Converting images

Use the provided Python script or similar tool:

```python
from PIL import Image

def convert_to_webp(input_path, output_path, size=300):
    img = Image.open(input_path)

    # Convert to RGB if necessary
    if img.mode not in ('RGB', 'RGBA'):
        img = img.convert('RGB')

    # Make it square (center crop)
    width, height = img.size
    if width != height:
        min_dim = min(width, height)
        left = (width - min_dim) // 2
        top = (height - min_dim) // 2
        right = left + min_dim
        bottom = top + min_dim
        img = img.crop((left, top, right, bottom))

    # Resize to target size
    img = img.resize((size, size), Image.Resampling.LANCZOS)

    # Save as WebP
    img.save(output_path, 'WEBP', quality=85, method=6)
```

## Current Implementations

- **pmoparadise**: Radio Paradise
- **pmoqobuz**: Qobuz

## Future Enhancements

The `MusicSource` trait can be extended with additional methods such as:

- Authentication status
- Available quality levels
- Streaming capabilities
- Search functionality
- Playlist management
- And more...

## License

MIT OR Apache-2.0
