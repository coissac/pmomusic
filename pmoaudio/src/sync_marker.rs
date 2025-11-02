use std::sync::Arc;
use tokio::sync::RwLock;

use pmometadata::TrackMetadata;

pub enum SyncMarker {
    TrackBoundary { metadata: Arc<RwLock<dyn TrackMetadata>> },
    StreamMetadata { key: String, value: String },
    TopZeroSync,
    Heartbeat,
    EndOfStream,
    Error(String),
    // autres cas à venir…
}
