use crate::{DeviceId, model::TrackMetadata};

/// Canonical representation of a track in a renderer queue.
///
/// This type is the bridge between:
///   - the UPnP MediaServer (DIDL-Lite items),
///   - the ControlPoint runtime,
///   - and the different queue backends (internal / OpenHome).
///
/// It is intentionally DIDL-centric: every item in a queue comes from
/// a UPnP ContentDirectory and carries its MediaServer identity.
#[derive(Clone, Debug)]
pub struct PlaybackItem {
    /// Identifier of the UPnP MediaServer that owns this content.
    ///
    /// Typically this is the UDN of the MediaServer device, or an
    /// equivalent logical identifier.
    pub media_server_id: DeviceId,

    /// Internal ID of the item in the queue.
    /// This ID only has meaning when returning a snapshot.
    /// In an internal queue, it can have any value; the position in the vector is what matters.
    /// In principle, it can be set to usize::MAX.
    pub backend_id: usize,

    /// DIDL-Lite `id` attribute of the `item` in the ContentDirectory.
    ///
    /// This, combined with `media_server_id`, is the logical identity
    /// of the track across refreshes of the MediaServer state.
    pub didl_id: String,

    /// Main resource URI to be used for playback.
    ///
    /// This is usually the first `<res>` element (or a selected one)
    /// from the DIDL-Lite item.
    pub uri: String,

    /// UPnP protocolInfo string for the resource (e.g., "http-get:*:audio/flac:*").
    ///
    /// This string describes the protocol, network, MIME type, and additional
    /// info about the media resource. It's required for proper UPnP/OpenHome
    /// renderer compatibility.
    pub protocol_info: String,

    /// Optional rich metadata for the track (title, artist, album, cover,
    /// duration, …).
    ///
    /// The exact structure is defined in `TrackMetadata` and may
    /// aggregate information from DIDL, tags, or additional sources.
    pub metadata: Option<TrackMetadata>,
}

impl PlaybackItem {
    /// Returns a stable, backend-agnostic logical identifier for this item.
    ///
    /// By default this is the concatenation of the MediaServer identifier
    /// and the DIDL `id`. Backends and higher-level logic should use this
    /// when they need to match items across queue rebuilds.
    pub fn unique_id(&self) -> String {
        // ADAPTE si MediaServerId n'implémente pas Display : utilise
        // un champ string interne ou une méthode as_str().
        format!("{}::{}", self.media_server_id.0, self.didl_id)
    }
}

/// Logical snapshot of a renderer queue.
///
/// This is the canonical view used by the ControlPoint and the REST/API
/// layer. It is independent of how the queue is actually stored (local
/// in-memory queue, OpenHome playlist, …).
#[derive(Clone, Debug)]
pub struct QueueSnapshot {
    /// All items currently in the queue, in play order.
    pub items: Vec<PlaybackItem>,
    /// Index (0-based) of the current item in `items`, or `None` if
    /// no item is currently selected.
    pub current_index: Option<usize>,
    /// Optional playlist ID if the queue is bound to a specific playlist.
    /// This allows reconstructing a queue from a snapshot by referencing
    /// the source playlist, enabling transfer between renderers.
    pub playlist_id: Option<String>,
}

impl QueueSnapshot {
    /// Returns the number of items in the snapshot.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the snapshot contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
