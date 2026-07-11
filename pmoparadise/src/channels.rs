//! Radio Paradise channel definitions
//!
//! This module maintains a dynamic registry of the available Radio Paradise
//! channels. The registry is initialized with a built-in default list and can
//! be refreshed at runtime from the `list_chan` API endpoint via
//! [`refresh_channels`], so newly added channels (Beyond, Serenity, KFAT, ...)
//! are picked up without a code change.
//!
//! Channel IDs are not contiguous (0, 1, 2, 3, 5, 42, 945...): never iterate
//! over an ID range, always go through [`channels`].

use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use serde::Deserialize;

/// Metadata descriptor for a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDescriptor {
    /// Channel ID as used by the RP API (`chan` parameter). Not contiguous.
    pub id: u16,
    /// Stable identifier used in playlist IDs, config paths, routes and UPnP
    /// object IDs. Legacy slugs are preserved for channels 0-3 so existing
    /// persisted playlists and configuration keep working.
    pub slug: String,
    /// Human-readable channel name.
    pub display_name: String,
    /// Short description of the channel.
    pub description: String,
    /// Cover image URL provided by the API, if any.
    pub image: Option<String>,
}

impl ChannelDescriptor {
    fn new_static(id: u16, slug: &str, display_name: &str, description: &str) -> Self {
        Self {
            id,
            slug: slug.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            // Stable URL pattern observed on img.radioparadise.com; the value
            // is overwritten by the API-provided one after refresh_channels().
            image: Some(format!(
                "https://img.radioparadise.com/channels/0/{}/cover_512x512/0.jpg",
                id
            )),
        }
    }
}

/// Legacy slugs for the historical channels (0-3).
///
/// Playlist IDs, config paths and UPnP object IDs are derived from the slug,
/// so the original slugs must be preserved even though the API now reports
/// different `stream_name`s ("main-mix", "global", ...).
fn legacy_slug(id: u16) -> Option<&'static str> {
    match id {
        0 => Some("main"),
        1 => Some("mellow"),
        2 => Some("rock"),
        3 => Some("eclectic"),
        _ => None,
    }
}

/// Built-in channel list, used as fallback when the API cannot be reached.
///
/// Snapshot of the `list_chan` endpoint (2026-07), with legacy slugs for 0-3.
pub fn default_channels() -> Vec<ChannelDescriptor> {
    vec![
        ChannelDescriptor::new_static(
            0,
            "main",
            "The Main Mix",
            "Eclectic mix of rock, world, electronica, and more",
        ),
        ChannelDescriptor::new_static(1, "mellow", "Mellow Mix", "Mellower, less aggressive music"),
        ChannelDescriptor::new_static(2, "rock", "RockIt!", "Heavier, more guitar-driven music"),
        ChannelDescriptor::new_static(3, "eclectic", "The Globe", "Curated worldwide selection"),
        ChannelDescriptor::new_static(5, "beyond", "Beyond...", "Adventurous, exploratory music"),
        ChannelDescriptor::new_static(
            42,
            "serenity",
            "Serenity",
            "Generative ambient soundscapes",
        ),
        ChannelDescriptor::new_static(945, "kfat", "KFAT", "Americana, blues and country"),
    ]
}

static CHANNEL_REGISTRY: Lazy<RwLock<Arc<Vec<ChannelDescriptor>>>> =
    Lazy::new(|| RwLock::new(Arc::new(default_channels())));

/// Snapshot of the currently known channels.
///
/// Returns the built-in defaults until [`refresh_channels`] has succeeded.
pub fn channels() -> Arc<Vec<ChannelDescriptor>> {
    CHANNEL_REGISTRY
        .read()
        .expect("channel registry poisoned")
        .clone()
}

/// Look up a channel by its API ID.
pub fn channel_by_id(id: u16) -> Option<ChannelDescriptor> {
    channels().iter().find(|ch| ch.id == id).cloned()
}

/// Look up a channel by its slug.
pub fn channel_by_slug(slug: &str) -> Option<ChannelDescriptor> {
    channels().iter().find(|ch| ch.slug == slug).cloned()
}

/// Resolve a channel from a user-supplied string: slug or numeric ID.
pub fn resolve_channel(s: &str) -> Option<ChannelDescriptor> {
    let s = s.trim();
    if let Ok(id) = s.parse::<u16>() {
        return channel_by_id(id);
    }
    channel_by_slug(&s.to_ascii_lowercase())
}

/// Raw channel entry as returned by the `list_chan` API endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct ApiChannel {
    pub chan: String,
    pub title: String,
    pub stream_name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    #[serde(default)]
    pub image: Option<String>,
}

impl ApiChannel {
    /// Convert to a descriptor. Returns `None` for entries our block-based
    /// pipeline cannot play (non-"block" channels) or with an unparsable ID.
    pub(crate) fn into_descriptor(self) -> Option<ChannelDescriptor> {
        if self.channel_type != "block" {
            return None;
        }
        let id: u16 = self.chan.parse().ok()?;
        let slug = legacy_slug(id)
            .map(str::to_string)
            .unwrap_or(self.stream_name);
        Some(ChannelDescriptor {
            id,
            slug,
            // The API provides no description; reuse the title.
            description: self.title.clone(),
            display_name: self.title,
            image: self.image,
        })
    }
}

/// Refresh the channel registry from the Radio Paradise API.
///
/// On success the registry is replaced with the fetched list and the new
/// snapshot is returned. On failure the registry is left untouched (built-in
/// defaults or previous successful fetch).
pub async fn refresh_channels(
    client: &crate::client::RadioParadiseClient,
) -> crate::error::Result<Arc<Vec<ChannelDescriptor>>> {
    let fetched = client.list_channels().await?;
    if fetched.is_empty() {
        return Err(crate::error::Error::other(
            "list_chan returned no playable channel",
        ));
    }
    let snapshot = Arc::new(fetched);
    *CHANNEL_REGISTRY
        .write()
        .expect("channel registry poisoned") = snapshot.clone();
    tracing::info!(
        "Radio Paradise channel registry refreshed: {} channels ({})",
        snapshot.len(),
        snapshot
            .iter()
            .map(|ch| ch.slug.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(snapshot)
}

/// Default maximum number of tracks to keep in history
///
/// This is used as the default if not configured via pmoconfig.
/// Value: 100 tracks - represents ~5-8 hours of playback history
pub const HISTORY_DEFAULT_MAX_TRACKS: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_channels_have_legacy_slugs() {
        let channels = default_channels();
        assert_eq!(channels[0].slug, "main");
        assert_eq!(channels[1].slug, "mellow");
        assert_eq!(channels[2].slug, "rock");
        assert_eq!(channels[3].slug, "eclectic");
    }

    #[test]
    fn test_default_channels_include_new_channels() {
        let channels = default_channels();
        assert!(channels.iter().any(|ch| ch.id == 5 && ch.slug == "beyond"));
        assert!(channels.iter().any(|ch| ch.id == 42 && ch.slug == "serenity"));
        assert!(channels.iter().any(|ch| ch.id == 945 && ch.slug == "kfat"));
    }

    #[test]
    fn test_resolve_channel() {
        assert_eq!(resolve_channel("main").map(|ch| ch.id), Some(0));
        assert_eq!(resolve_channel("0").map(|ch| ch.id), Some(0));
        assert_eq!(resolve_channel("MELLOW").map(|ch| ch.id), Some(1));
        assert_eq!(resolve_channel("945").map(|ch| ch.slug), Some("kfat".to_string()));
        assert!(resolve_channel("invalid").is_none());
        // IDs are sparse: 4 is not a channel
        assert!(resolve_channel("4").is_none());
    }

    #[test]
    fn test_api_channel_conversion() {
        let api = ApiChannel {
            chan: "3".to_string(),
            title: "The Globe".to_string(),
            stream_name: "global".to_string(),
            channel_type: "block".to_string(),
            image: None,
        };
        let desc = api.into_descriptor().unwrap();
        // Legacy slug preserved for channel 3
        assert_eq!(desc.slug, "eclectic");
        assert_eq!(desc.display_name, "The Globe");

        let api = ApiChannel {
            chan: "945".to_string(),
            title: "KFAT".to_string(),
            stream_name: "kfat".to_string(),
            channel_type: "block".to_string(),
            image: None,
        };
        assert_eq!(api.into_descriptor().unwrap().slug, "kfat");

        let api = ApiChannel {
            chan: "7".to_string(),
            title: "Live Stream".to_string(),
            stream_name: "live".to_string(),
            channel_type: "live".to_string(),
            image: None,
        };
        assert!(api.into_descriptor().is_none());
    }
}
