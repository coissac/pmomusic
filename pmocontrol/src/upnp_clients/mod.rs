mod avtransport_client;
mod connection_manager_client;
mod openhome_client;
mod rendering_control_client;

pub use crate::upnp_clients::avtransport_client::{AvTransportClient, PositionInfo};
pub use crate::upnp_clients::connection_manager_client::{
    ConnectionInfo, ConnectionManagerClient, ProtocolInfo,
};
pub use crate::upnp_clients::openhome_client::{
    OPENHOME_PLAYLIST_HEAD_ID, OhInfoClient, OhPlaylistClient, OhProductClient, OhRadioClient,
    OhTimeClient, OhTrack, OhTrackEntry, OhVolumeClient,
};
pub use crate::upnp_clients::rendering_control_client::RenderingControlClient;

/// Resolve a possibly relative controlURL against the description URL.
///
/// - If `control_url` is already absolute (starts with http:// or https://), it is returned as-is.
/// - Otherwise, it is resolved against the scheme://host:port of `description_url`.
pub fn resolve_control_url(description_url: &str, control_url: &str) -> String {
    if control_url.starts_with("http://") || control_url.starts_with("https://") {
        return control_url.to_string();
    }

    // Extract "scheme://host[:port]" from description_url
    if let Some((scheme, rest)) = description_url.split_once("://") {
        if let Some(pos) = rest.find('/') {
            let authority = &rest[..pos];
            let base = format!("{}://{}", scheme, authority);

            if control_url.starts_with('/') {
                return format!("{}{}", base, control_url);
            } else {
                return format!("{}/{}", base, control_url);
            }
        }
    }

    // Fallback: just return the raw control_url if we cannot parse
    control_url.to_string()
}
