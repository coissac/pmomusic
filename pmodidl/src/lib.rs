//! # pmodidl - DIDL-Lite Parser
//!
//! Parser et utilitaires pour le format DIDL-Lite utilisé dans UPnP/DLNA.

use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

// ============= Couche d'abstraction générique =============

/// Trait pour tout parser de métadonnées média
pub trait MediaMetadataParser: Sized {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Parse une chaîne de métadonnées
    fn parse(input: &str) -> Result<Self, Self::Error>;

    /// Retourne le format du parser
    fn format_name() -> &'static str;
}

/// Enveloppe générique pour tout type de métadonnées parsées
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct ParsedMetadata<T> {
    /// Format du document (ex: "DIDL-Lite", "RSS", etc.)
    pub format: String,

    /// Données parsées
    pub data: T,

    /// Timestamp du parsing (exclu de la réflexion car SystemTime n'implémente pas Reflect)
    #[reflect(ignore)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_at: Option<std::time::SystemTime>,
}

impl<T> ParsedMetadata<T> {
    pub fn new(format: impl Into<String>, data: T) -> Self {
        Self {
            format: format.into(),
            data,
            parsed_at: Some(std::time::SystemTime::now()),
        }
    }

    /// Transforme les données avec une fonction
    pub fn map<U, F>(self, f: F) -> ParsedMetadata<U>
    where
        F: FnOnce(T) -> U,
    {
        ParsedMetadata {
            format: self.format,
            data: f(self.data),
            parsed_at: self.parsed_at,
        }
    }
}

/// Fonction helper pour parser et envelopper automatiquement
pub fn parse_metadata<P: MediaMetadataParser>(input: &str) -> Result<ParsedMetadata<P>, P::Error> {
    let data = P::parse(input)?;
    Ok(ParsedMetadata::new(P::format_name(), data))
}

// ============= Implémentation pour DIDLLite =============

impl MediaMetadataParser for DIDLLite {
    type Error = quick_xml::de::DeError;

    fn parse(input: &str) -> Result<Self, Self::Error> {
        quick_xml::de::from_str(input)
    }

    fn format_name() -> &'static str {
        "DIDL-Lite"
    }
}

/// Type alias pour faciliter l'utilisation
pub type DidlMetadata = ParsedMetadata<DIDLLite>;

// ============= Structures DIDL-Lite =============

/// Racine d'un document DIDL-Lite
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Reflect)]
#[serde(rename = "DIDL-Lite")]
pub struct DIDLLite {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,

    #[serde(rename = "@xmlns:upnp", skip_serializing_if = "Option::is_none")]
    pub xmlns_upnp: Option<String>,

    #[serde(rename = "@xmlns:dc", skip_serializing_if = "Option::is_none")]
    pub xmlns_dc: Option<String>,

    #[serde(rename = "@xmlns:dlna", skip_serializing_if = "Option::is_none")]
    pub xmlns_dlna: Option<String>,

    #[serde(rename = "@xmlns:sec", skip_serializing_if = "Option::is_none")]
    pub xmlns_sec: Option<String>,

    #[serde(rename = "@xmlns:pv", skip_serializing_if = "Option::is_none")]
    pub xmlns_pv: Option<String>,

    #[serde(rename = "container", default)]
    pub containers: Vec<Container>,

    #[serde(rename = "item", default)]
    pub items: Vec<Item>,
}

/// Container pouvant contenir d'autres containers ou items
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Reflect)]
pub struct Container {
    #[serde(rename = "@id")]
    pub id: String,

    #[serde(rename = "@parentID")]
    pub parent_id: String,

    #[serde(rename = "@restricted", skip_serializing_if = "Option::is_none")]
    pub restricted: Option<String>,

    #[serde(rename = "@childCount", skip_serializing_if = "Option::is_none")]
    pub child_count: Option<String>,

    #[serde(rename = "dc:title", alias = "title")]
    pub title: String,

    #[serde(rename = "upnp:class", alias = "class")]
    pub class: String,

    #[serde(rename = "container", default)]
    pub containers: Vec<Container>,

    #[serde(rename = "item", default)]
    pub items: Vec<Item>,
}

/// Item représentant un objet audio
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Reflect)]
pub struct Item {
    #[serde(rename = "@id")]
    pub id: String,

    #[serde(rename = "@parentID")]
    pub parent_id: String,

    #[serde(rename = "@restricted", skip_serializing_if = "Option::is_none")]
    pub restricted: Option<String>,

    #[serde(rename = "dc:title", alias = "title")]
    pub title: String,

    #[serde(
        rename = "dc:creator",
        alias = "creator",
        skip_serializing_if = "Option::is_none"
    )]
    pub creator: Option<String>,

    #[serde(rename = "upnp:class", alias = "class")]
    pub class: String,

    #[serde(
        rename = "upnp:artist",
        alias = "artist",
        skip_serializing_if = "Option::is_none"
    )]
    pub artist: Option<String>,

    #[serde(
        rename = "upnp:album",
        alias = "album",
        skip_serializing_if = "Option::is_none"
    )]
    pub album: Option<String>,

    #[serde(
        rename = "upnp:genre",
        alias = "genre",
        skip_serializing_if = "Option::is_none"
    )]
    pub genre: Option<String>,

    #[serde(
        rename = "upnp:albumArtURI",
        alias = "albumArtURI",
        skip_serializing_if = "Option::is_none"
    )]
    pub album_art: Option<String>,

    #[serde(skip)]
    pub album_art_pk: Option<String>,

    #[serde(
        rename = "dc:date",
        alias = "date",
        skip_serializing_if = "Option::is_none"
    )]
    pub date: Option<String>,

    #[serde(
        rename = "upnp:originalTrackNumber",
        alias = "originalTrackNumber",
        skip_serializing_if = "Option::is_none"
    )]
    pub original_track_number: Option<String>,

    #[serde(rename = "res", default)]
    pub resources: Vec<Resource>,

    #[serde(rename = "desc", default)]
    pub descriptions: Vec<Description>,
}

/// Ressource média (fichier audio)
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Reflect)]
pub struct Resource {
    #[serde(rename = "@protocolInfo")]
    pub protocol_info: String,

    #[serde(rename = "@bitsPerSample", skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<String>,

    #[serde(rename = "@sampleFrequency", skip_serializing_if = "Option::is_none")]
    pub sample_frequency: Option<String>,

    #[serde(rename = "@nrAudioChannels", skip_serializing_if = "Option::is_none")]
    pub nr_audio_channels: Option<String>,

    #[serde(rename = "@duration", skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,

    #[serde(rename = "$text")]
    pub url: String,
}

/// Description avec métadonnées additionnelles (replaygain, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Reflect)]
pub struct Description {
    #[serde(rename = "@id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "@nameSpace", skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    #[serde(rename = "track_gain", skip_serializing_if = "Option::is_none")]
    pub track_gain: Option<String>,

    #[serde(rename = "track_peak", skip_serializing_if = "Option::is_none")]
    pub track_peak: Option<String>,
}

// ============= Implémentation des méthodes =============

impl DIDLLite {
    /// Itère sur tous les containers de manière récursive
    pub fn all_containers(&self) -> impl Iterator<Item = &Container> {
        AllContainersIter::new(&self.containers)
    }

    /// Itère sur tous les items de manière récursive
    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        AllItemsIter::new(&self.containers, &self.items)
    }

    /// Trouve un container par ID
    pub fn get_container_by_id(&self, id: &str) -> Option<&Container> {
        self.all_containers().find(|c| c.id == id)
    }

    /// Trouve un item par ID
    pub fn get_item_by_id(&self, id: &str) -> Option<&Item> {
        self.all_items().find(|i| i.id == id)
    }

    /// Filtre les containers
    pub fn filter_containers<F>(&self, predicate: F) -> impl Iterator<Item = &Container>
    where
        F: Fn(&Container) -> bool,
    {
        self.all_containers().filter(move |c| predicate(c))
    }

    /// Filtre les items
    pub fn filter_items<F>(&self, predicate: F) -> impl Iterator<Item = &Item>
    where
        F: Fn(&Item) -> bool,
    {
        self.all_items().filter(move |i| predicate(i))
    }

    /// Génère une représentation Markdown
    pub fn to_markdown(&self) -> String {
        let mut buf = String::new();
        buf.push_str("### DIDL-Lite Document\n\n");

        if !self.containers.is_empty() {
            buf.push_str("#### Containers\n\n");
            for container in &self.containers {
                container.write_markdown(&mut buf, 0);
            }
        }

        if !self.items.is_empty() {
            buf.push_str("#### Items\n\n");
            for item in &self.items {
                item.write_markdown(&mut buf, 0);
            }
        }

        buf
    }
}

impl Container {
    /// Itère sur tous les containers enfants récursivement
    pub fn all_containers(&self) -> impl Iterator<Item = &Container> {
        AllContainersIter::new(&self.containers)
    }

    /// Itère sur tous les items de ce container et ses enfants
    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        AllItemsIter::new(&self.containers, &self.items)
    }

    fn write_markdown(&self, buf: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);

        writeln!(buf, "{}- **Container**: {}", indent, self.title).unwrap();
        writeln!(buf, "{}  - ID: `{}`", indent, self.id).unwrap();
        writeln!(buf, "{}  - ParentID: `{}`", indent, self.parent_id).unwrap();
        writeln!(buf, "{}  - Class: `{}`", indent, self.class).unwrap();

        if let Some(ref restricted) = self.restricted {
            writeln!(buf, "{}  - Restricted: `{}`", indent, restricted).unwrap();
        }
        if let Some(ref count) = self.child_count {
            writeln!(buf, "{}  - ChildCount: `{}`", indent, count).unwrap();
        }

        if !self.containers.is_empty() {
            writeln!(buf, "{}  - Subcontainers:", indent).unwrap();
            for sub in &self.containers {
                sub.write_markdown(buf, depth + 2);
            }
        }

        if !self.items.is_empty() {
            writeln!(buf, "{}  - Items:", indent).unwrap();
            for item in &self.items {
                item.write_markdown(buf, depth + 2);
            }
        }

        buf.push('\n');
    }
}

impl Item {
    /// Itère sur les ressources audio uniquement
    pub fn audio_resources(&self) -> impl Iterator<Item = &Resource> {
        self.resources
            .iter()
            .filter(|r| r.protocol_info.contains("audio/"))
    }

    /// Retourne la ressource principale (première disponible)
    pub fn primary_resource(&self) -> Option<&Resource> {
        self.resources.first()
    }

    /// Itère sur les métadonnées sous forme de paires clé-valeur
    pub fn metadata(&self) -> impl Iterator<Item = (&str, &str)> {
        let mut pairs = Vec::new();

        pairs.push(("title", self.title.as_str()));

        if let Some(ref artist) = self.artist {
            pairs.push(("artist", artist.as_str()));
        }
        if let Some(ref album) = self.album {
            pairs.push(("album", album.as_str()));
        }
        if let Some(ref genre) = self.genre {
            pairs.push(("genre", genre.as_str()));
        }
        if let Some(ref date) = self.date {
            pairs.push(("date", date.as_str()));
        }
        if let Some(ref track) = self.original_track_number {
            pairs.push(("trackNumber", track.as_str()));
        }

        for desc in &self.descriptions {
            if let Some(ref gain) = desc.track_gain {
                pairs.push(("replayGain", gain.as_str()));
            }
            if let Some(ref peak) = desc.track_peak {
                pairs.push(("replayPeak", peak.as_str()));
            }
        }

        pairs.into_iter()
    }

    fn write_markdown(&self, buf: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);

        writeln!(buf, "{}- **Item**: {}", indent, self.title).unwrap();
        writeln!(buf, "{}  - ID: `{}`", indent, self.id).unwrap();
        writeln!(buf, "{}  - ParentID: `{}`", indent, self.parent_id).unwrap();
        writeln!(buf, "{}  - Class: `{}`", indent, self.class).unwrap();

        if let Some(ref creator) = self.creator {
            writeln!(buf, "{}  - Creator: {}", indent, creator).unwrap();
        }
        if let Some(ref artist) = self.artist {
            writeln!(buf, "{}  - Artist: {}", indent, artist).unwrap();
        }
        if let Some(ref album) = self.album {
            writeln!(buf, "{}  - Album: {}", indent, album).unwrap();
        }
        if let Some(ref genre) = self.genre {
            writeln!(buf, "{}  - Genre: {}", indent, genre).unwrap();
        }
        if let Some(ref art) = self.album_art {
            writeln!(buf, "{}  - Album Art: ![Cover]({})", indent, art).unwrap();
        }
        if let Some(ref date) = self.date {
            writeln!(buf, "{}  - Date: {}", indent, date).unwrap();
        }
        if let Some(ref track) = self.original_track_number {
            writeln!(buf, "{}  - Track: {}", indent, track).unwrap();
        }

        if !self.resources.is_empty() {
            writeln!(buf, "{}  - Resources:", indent).unwrap();
            for res in &self.resources {
                writeln!(buf, "{}    - URL: {}", indent, res.url).unwrap();
                writeln!(buf, "{}      - Protocol: `{}`", indent, res.protocol_info).unwrap();
                if let Some(ref dur) = res.duration {
                    writeln!(buf, "{}      - Duration: `{}`", indent, dur).unwrap();
                }
                if let Some(ref bits) = res.bits_per_sample {
                    writeln!(buf, "{}      - BitsPerSample: `{}`", indent, bits).unwrap();
                }
                if let Some(ref freq) = res.sample_frequency {
                    writeln!(buf, "{}      - SampleFrequency: `{}`", indent, freq).unwrap();
                }
                if let Some(ref channels) = res.nr_audio_channels {
                    writeln!(buf, "{}      - Channels: `{}`", indent, channels).unwrap();
                }
            }
        }

        if !self.descriptions.is_empty() {
            writeln!(buf, "{}  - Descriptions:", indent).unwrap();
            for desc in &self.descriptions {
                if let Some(ref ns) = desc.namespace {
                    writeln!(buf, "{}    - Namespace: `{}`", indent, ns).unwrap();
                }
                if let Some(ref gain) = desc.track_gain {
                    writeln!(buf, "{}      - Track Gain: `{}`", indent, gain).unwrap();
                }
                if let Some(ref peak) = desc.track_peak {
                    writeln!(buf, "{}      - Track Peak: `{}`", indent, peak).unwrap();
                }
            }
        }

        buf.push('\n');
    }
}

// ============= Itérateurs personnalisés =============

struct AllContainersIter<'a> {
    stack: Vec<&'a Container>,
}

impl<'a> AllContainersIter<'a> {
    fn new(containers: &'a [Container]) -> Self {
        Self {
            stack: containers.iter().collect(),
        }
    }
}

impl<'a> Iterator for AllContainersIter<'a> {
    type Item = &'a Container;

    fn next(&mut self) -> Option<Self::Item> {
        self.stack.pop().map(|container| {
            // Ajouter les enfants à la pile
            self.stack.extend(container.containers.iter());
            container
        })
    }
}

struct AllItemsIter<'a> {
    containers: Vec<&'a Container>,
    current_items: std::slice::Iter<'a, Item>,
}

impl<'a> AllItemsIter<'a> {
    fn new(containers: &'a [Container], items: &'a [Item]) -> Self {
        Self {
            containers: containers.iter().collect(),
            current_items: items.iter(),
        }
    }
}

impl<'a> Iterator for AllItemsIter<'a> {
    type Item = &'a Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.current_items.next() {
                return Some(item);
            }

            let container = self.containers.pop()?;
            self.containers.extend(container.containers.iter());
            self.current_items = container.items.iter();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_didl() {
        let xml = r#"
        <DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"
                   xmlns:dc="http://purl.org/dc/elements/1.1/"
                   xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
            <item id="1" parentID="0">
                <dc:title>Test Song</dc:title>
                <upnp:class>object.item.audioItem.musicTrack</upnp:class>
                <res protocolInfo="http-get:*:audio/mpeg:*">http://example.com/song.mp3</res>
            </item>
        </DIDL-Lite>
        "#;

        let didl = DIDLLite::parse(xml).unwrap();
        assert_eq!(didl.items.len(), 1);
        assert_eq!(didl.items[0].title, "Test Song");
    }

    #[test]
    fn test_parse_without_namespaces() {
        // Teste un XML sans namespaces explicites (devices UPnP laxistes)
        let xml = r#"
        <DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/">
            <item id="1" parentID="0">
                <title>Test Song</title>
                <class>object.item.audioItem.musicTrack</class>
                <res protocolInfo="http-get:*:audio/mpeg:*">http://example.com/song.mp3</res>
            </item>
        </DIDL-Lite>
        "#;

        let didl = DIDLLite::parse(xml).unwrap();
        assert_eq!(didl.items.len(), 1);
        assert_eq!(didl.items[0].title, "Test Song");
    }

    #[test]
    fn test_generic_parser() {
        let xml = r#"
        <DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"
                   xmlns:dc="http://purl.org/dc/elements/1.1/"
                   xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
        </DIDL-Lite>
        "#;

        // Utiliser le parser générique
        let metadata: DidlMetadata = parse_metadata(xml).unwrap();

        assert_eq!(metadata.format, "DIDL-Lite");
        assert!(metadata.parsed_at.is_some());
    }

    #[test]
    fn test_metadata_map() {
        let xml = r#"
        <DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"
                   xmlns:dc="http://purl.org/dc/elements/1.1/"
                   xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
        </DIDL-Lite>
        "#;

        let metadata: DidlMetadata = parse_metadata(xml).unwrap();

        // Transformer les données
        let item_count = metadata.map(|didl| didl.items.len());

        assert_eq!(item_count.format, "DIDL-Lite");
        assert_eq!(item_count.data, 0);
    }
}
