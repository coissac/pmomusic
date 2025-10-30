use std::sync::Arc;

use pmometadata::TrackMetadata;

pub enum SyncMarker {
    TrackBoundary { metadata: Arc<dyn TrackMetadata> },
    StreamMetadata { key: String, value: String },
    TopZeroSync,
    Heartbeat,
    EndOfStream,
    Error(String),
    // autres cas à venir…
}
