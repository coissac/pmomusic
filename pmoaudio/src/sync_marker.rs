use std::sync::Arc;
use tokio::sync::RwLock;

use pmometadata::TrackMetadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    Continuous,
    Finite,
}

pub enum SyncMarker {
    TrackBoundary {
        metadata: Arc<RwLock<dyn TrackMetadata>>,
        stream_type: StreamType,
    },
    StreamMetadata {
        key: String,
        value: String,
    },
    TopZeroSync,
    Heartbeat,
    EndOfStream,
    Error(String),
}
