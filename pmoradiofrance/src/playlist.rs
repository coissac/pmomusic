//! Structures et helpers pour la construction de playlists UPnP Radio France
//!
//! Ce module fournit les structures nécessaires pour organiser les stations
//! Radio France en groupes hiérarchiques et construire des playlists UPnP
//! avec métadonnées volatiles.
//!
//! # Architecture
//!
//! - `StationGroups` : Organisation hiérarchique de toutes les stations
//! - `StationGroup` : Groupe station principale + webradios associées
//! - `StationPlaylist` : Playlist UPnP volatile pour une station
//!
//! # Exemple
//!
//! ```rust,ignore
//! use pmoradiofrance::playlist::{StationGroups, StationPlaylist};
//!
//! // Organiser les stations en groupes
//! let groups = StationGroups::from_stations(stations);
//!
//! // Construire une playlist pour une station
//! let playlist = StationPlaylist::from_live_metadata(
//!     station,
//!     metadata,
//!     &cover_cache,
//!     server_base_url,
//! ).await?;
//! ```

use crate::error::Result;
use crate::models::{ImageSize, LiveResponse, Station, StationType, StreamFormat};
use pmodidl::{Item, Resource};
use serde::{Deserialize, Serialize};

#[cfg(feature = "cache")]
use pmocovers::Cache as CoverCache;
#[cfg(feature = "cache")]
use std::sync::Arc;

// ============================================================================
// Groupes de stations
// ============================================================================

/// Groupes de stations organisés hiérarchiquement
///
/// Cette structure organise les stations Radio France en trois catégories :
/// - `standalone` : Stations sans webradios (France Culture, France Inter, France Info, Mouv')
/// - `with_webradios` : Groupes avec station principale + webradios (FIP, France Musique)
/// - `local_radios` : Toutes les radios ICI (ex-France Bleu)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationGroups {
    /// Stations sans webradios associées
    pub standalone: Vec<Station>,
    /// Groupes station principale + webradios
    pub with_webradios: Vec<StationGroup>,
    /// Radios locales ICI (ex-France Bleu)
    pub local_radios: Vec<Station>,
}

/// Groupe station principale + webradios associées
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationGroup {
    /// Station principale (ex: FIP)
    pub main: Station,
    /// Webradios associées (ex: FIP Rock, FIP Jazz, ...)
    pub webradios: Vec<Station>,
}

impl StationGroups {
    /// Organise une liste de stations en groupes hiérarchiques
    ///
    /// # Logique de regroupement
    ///
    /// 1. Les stations locales (France Bleu/ICI) sont regroupées dans `local_radios`
    /// 2. Les webradios sont associées à leur station parente
    /// 3. Les stations principales sans webradios vont dans `standalone`
    /// 4. Les stations avec au moins une webradio vont dans `with_webradios`
    pub fn from_stations(stations: Vec<Station>) -> Self {
        use std::collections::HashMap;

        let mut standalone = Vec::new();
        let mut local_radios = Vec::new();
        let mut main_stations: HashMap<String, Station> = HashMap::new();
        let mut webradios_by_parent: HashMap<String, Vec<Station>> = HashMap::new();

        // Premier passage : trier par type
        for station in stations {
            match &station.station_type {
                StationType::Main => {
                    // Filtrer France Bleu : ce n'est pas une vraie radio mais le nom générique
                    // pour toutes les radios locales ICI (ex-France Bleu)
                    if station.slug != "francebleu" {
                        main_stations.insert(station.slug.clone(), station);
                    }
                }
                StationType::Webradio { parent_station } => {
                    webradios_by_parent
                        .entry(parent_station.clone())
                        .or_default()
                        .push(station);
                }
                StationType::LocalRadio { .. } => {
                    local_radios.push(station);
                }
            }
        }

        // Deuxième passage : construire les groupes
        let mut with_webradios = Vec::new();

        for (slug, main) in main_stations {
            if let Some(webradios) = webradios_by_parent.remove(&slug) {
                // Cette station a des webradios
                with_webradios.push(StationGroup { main, webradios });
            } else {
                // Station standalone
                standalone.push(main);
            }
        }

        // Trier pour un affichage cohérent
        standalone.sort_by(|a, b| a.name.cmp(&b.name));
        local_radios.sort_by(|a, b| a.name.cmp(&b.name));
        with_webradios.sort_by(|a, b| a.main.name.cmp(&b.main.name));

        for group in &mut with_webradios {
            group.webradios.sort_by(|a, b| a.name.cmp(&b.name));
        }

        Self {
            standalone,
            with_webradios,
            local_radios,
        }
    }

    /// Retourne toutes les stations dans un ordre de navigation logique
    ///
    /// Ordre : standalone, puis groupes (main + webradios), puis locales
    pub fn all_stations(&self) -> impl Iterator<Item = &Station> {
        self.standalone
            .iter()
            .chain(
                self.with_webradios
                    .iter()
                    .flat_map(|g| std::iter::once(&g.main).chain(g.webradios.iter())),
            )
            .chain(self.local_radios.iter())
    }

    /// Nombre total de stations
    pub fn total_count(&self) -> usize {
        self.standalone.len()
            + self
                .with_webradios
                .iter()
                .map(|g| 1 + g.webradios.len())
                .sum::<usize>()
            + self.local_radios.len()
    }
}

impl StationGroup {
    /// Retourne toutes les stations du groupe (main + webradios)
    pub fn all_stations(&self) -> impl Iterator<Item = &Station> {
        std::iter::once(&self.main).chain(self.webradios.iter())
    }

    /// Nombre de stations dans le groupe
    pub fn count(&self) -> usize {
        1 + self.webradios.len()
    }
}

// ============================================================================
// Playlist UPnP pour une station
// ============================================================================

/// Playlist UPnP volatile pour une station Radio France
///
/// Contient UN SEUL item représentant le stream live.
/// Les métadonnées de l'item changent au fil du temps (émissions, morceaux)
/// mais l'URL du stream reste identique.
///
/// # Volatilité
///
/// - L'URL du stream ne change JAMAIS
/// - Le titre, artiste, album changent toutes les 2-5 minutes
/// - La cover change avec chaque nouvelle émission/morceau
#[derive(Debug, Clone)]
pub struct StationPlaylist {
    /// ID de la playlist (ex: "radiofrance:franceculture")
    pub id: String,

    /// Station source
    pub station: Station,

    /// Item UPnP unique représentant le stream
    pub stream_item: Item,
}

impl StationPlaylist {
    /// Construit une playlist depuis les métadonnées live
    ///
    /// # Arguments
    ///
    /// * `station` - Station Radio France
    /// * `metadata` - Métadonnées live de l'API
    /// * `cover_cache` - Cache des covers (optionnel)
    /// * `server_base_url` - URL de base du serveur pour les covers cachées
    ///
    /// # Mapping des métadonnées
    ///
    /// Pour **radios parlées** (France Culture, France Inter, France Info) :
    /// - `title` = émission + titre du jour
    /// - `artist` = producteur
    /// - `album` = nom de l'émission
    ///
    /// Pour **radios musicales** (FIP, France Musique) :
    /// - Si morceau en cours : titre, artiste, album du morceau
    /// - Sinon : fallback sur le mapping radio parlée
    #[cfg(feature = "cache")]
    pub async fn from_live_metadata(
        station: Station,
        metadata: &LiveResponse,
        cover_cache: Option<&Arc<CoverCache>>,
        server_base_url: Option<&str>,
    ) -> Result<Self> {
        let id = format!("radiofrance:{}", station.slug);
        let stream_item =
            Self::build_item_from_metadata(&station, metadata, cover_cache, server_base_url)
                .await?;

        Ok(Self {
            id,
            station,
            stream_item,
        })
    }

    /// Construit une playlist sans cache de covers
    pub fn from_live_metadata_no_cache(
        station: Station,
        metadata: &LiveResponse,
        server_base_url: Option<&str>,
    ) -> Result<Self> {
        let id = format!("radiofrance:{}", station.slug);
        let stream_item = Self::build_item_from_metadata_sync(&station, metadata, server_base_url)?;

        Ok(Self {
            id,
            station,
            stream_item,
        })
    }

    /// Met à jour les métadonnées volatiles de l'item
    ///
    /// Met à jour uniquement les champs volatiles :
    /// - title, artist, album (depuis nouvelles métadonnées)
    /// - album_art / album_art_pk (si nouvelle cover)
    ///
    /// L'URL du stream (resource.url) ne change JAMAIS.
    #[cfg(feature = "cache")]
    pub async fn update_metadata(
        &mut self,
        metadata: &LiveResponse,
        cover_cache: Option<&Arc<CoverCache>>,
        server_base_url: Option<&str>,
    ) -> Result<()> {
        // Reconstruire l'item avec les nouvelles métadonnées
        // mais conserver l'URL du stream
        let old_url = self
            .stream_item
            .resources
            .first()
            .map(|r| r.url.clone())
            .unwrap_or_default();

        let mut new_item =
            Self::build_item_from_metadata(&self.station, metadata, cover_cache, server_base_url)
                .await?;

        // S'assurer que l'URL du stream n'a pas changé
        if let Some(res) = new_item.resources.first_mut() {
            if !old_url.is_empty() {
                res.url = old_url;
            }
        }

        self.stream_item = new_item;
        Ok(())
    }

    /// Met à jour les métadonnées sans cache
    pub fn update_metadata_no_cache(
        &mut self,
        metadata: &LiveResponse,
        server_base_url: Option<&str>,
    ) -> Result<()> {
        let old_url = self
            .stream_item
            .resources
            .first()
            .map(|r| r.url.clone())
            .unwrap_or_default();

        let mut new_item =
            Self::build_item_from_metadata_sync(&self.station, metadata, server_base_url)?;

        if let Some(res) = new_item.resources.first_mut() {
            if !old_url.is_empty() {
                res.url = old_url;
            }
        }

        self.stream_item = new_item;
        Ok(())
    }

    /// Construit un Item UPnP depuis les métadonnées live (avec cache)
    #[cfg(feature = "cache")]
    async fn build_item_from_metadata(
        station: &Station,
        metadata: &LiveResponse,
        cover_cache: Option<&Arc<CoverCache>>,
        server_base_url: Option<&str>,
    ) -> Result<Item> {
        let (title, creator, artist, album, genre, class) =
            Self::extract_metadata_fields(station, metadata);

        // Gestion de la cover
        let (album_art, album_art_pk) = if let Some(cache) = cover_cache {
            Self::cache_cover(metadata, cache, server_base_url).await
        } else {
            Self::extract_cover_url(metadata, server_base_url)
        };

        // Construction de la ressource (stream)
        let resource = Self::build_stream_resource(metadata);

        Ok(Item {
            id: format!("radiofrance:{}:stream", station.slug),
            parent_id: format!("radiofrance:{}", station.slug),
            restricted: Some("1".to_string()),
            title,
            creator,
            class,
            artist,
            album,
            genre,
            album_art,
            album_art_pk,
            date: None,
            original_track_number: None,
            resources: vec![resource],
            descriptions: vec![],
        })
    }

    /// Construit un Item UPnP depuis les métadonnées live (sans cache async)
    fn build_item_from_metadata_sync(
        station: &Station,
        metadata: &LiveResponse,
        server_base_url: Option<&str>,
    ) -> Result<Item> {
        let (title, creator, artist, album, genre, class) =
            Self::extract_metadata_fields(station, metadata);

        let (album_art, album_art_pk) = Self::extract_cover_url(metadata, server_base_url);
        let resource = Self::build_stream_resource(metadata);

        Ok(Item {
            id: format!("radiofrance:{}:stream", station.slug),
            parent_id: format!("radiofrance:{}", station.slug),
            restricted: Some("1".to_string()),
            title,
            creator,
            class,
            artist,
            album,
            genre,
            album_art,
            album_art_pk,
            date: None,
            original_track_number: None,
            resources: vec![resource],
            descriptions: vec![],
        })
    }

    /// Extrait les champs de métadonnées selon le type de radio
    fn extract_metadata_fields(
        station: &Station,
        metadata: &LiveResponse,
    ) -> (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    ) {
        let now = &metadata.now;

        // Détecter si c'est une radio musicale avec un morceau en cours
        if let Some(ref song) = now.song {
            // Radio musicale avec morceau
            let title = now.first_line.title_or_default().to_string();
            let artist = if song.interpreters.is_empty() {
                None
            } else {
                Some(song.artists_display())
            };
            let album = song.release.title.clone();
            let creator = artist.clone();
            let genre = Some("Music".to_string());
            let class = "object.item.audioItem.musicTrack".to_string();

            (title, creator, artist, album, genre, class)
        } else {
            // Radio parlée ou segment talk sur radio musicale
            let first = now.first_line.title_or_default();
            let second = now.second_line.title_or_default();

            // Construire le titre en évitant les duplications
            let title = if !first.is_empty() && !second.is_empty() {
                // Si first contient déjà second, utiliser seulement first
                if first.contains(second) {
                    first.to_string()
                } else {
                    format!("{} • {}", first, second)
                }
            } else if !first.is_empty() {
                first.to_string()
            } else {
                station.display_name().to_string()
            };

            // Artist/Creator = "{Station} - {Subtitle}"
            let artist = if !second.is_empty() {
                Some(format!("{} - {}", station.name, second))
            } else {
                Some(station.name.clone())
            };
            let creator = artist.clone();
            // Album = nom de l'émission principale
            let album = if !first.is_empty() {
                Some(first.to_string())
            } else {
                Some(station.name.clone())
            };
            let genre = Some("Talk Radio".to_string());
            let class = "object.item.audioItem.audioBroadcast".to_string();

            (title, creator, artist, album, genre, class)
        }
    }

    /// Extrait l'URL de cover depuis les métadonnées (sans cache)
    fn extract_cover_url(
        metadata: &LiveResponse,
        server_base_url: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        // Priorité : visual_background > visuals.card > visuals.player > logo par défaut

        // 1. visual_background
        if let Some(ref visual) = metadata.now.visual_background {
            if let Some(uuid) = visual.extract_uuid() {
                let url = ImageSize::Large.build_url(&uuid);
                return (Some(url), None);
            }
        }

        // 2. visuals.card
        if let Some(ref visuals) = metadata.now.visuals {
            if let Some(ref card) = visuals.card {
                if let Some(uuid) = card.extract_uuid() {
                    let url = ImageSize::Large.build_url(&uuid);
                    return (Some(url), None);
                }
            }

            // 3. visuals.player
            if let Some(ref player) = visuals.player {
                if let Some(uuid) = player.extract_uuid() {
                    let url = ImageSize::Large.build_url(&uuid);
                    return (Some(url), None);
                }
            }
        }

        // Fallback sur le logo par défaut via l'API REST
        if let Some(base) = server_base_url {
            let logo_url = format!(
                "{}/api/radiofrance/default-logo",
                base.trim_end_matches('/')
            );
            return (Some(logo_url), None);
        }

        // Pas de cover trouvée et pas de serveur configuré
        (None, None)
    }

    /// Cache la cover et retourne (url_publique, pk)
    #[cfg(feature = "cache")]
    async fn cache_cover(
        metadata: &LiveResponse,
        cache: &Arc<CoverCache>,
        server_base_url: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        // Extraire l'UUID de la cover (priorité : visual_background > visuals.card > visuals.player)
        let uuid = metadata
            .now
            .visual_background
            .as_ref()
            .and_then(|v| v.extract_uuid())
            .or_else(|| {
                metadata.now.visuals.as_ref().and_then(|visuals| {
                    visuals
                        .card
                        .as_ref()
                        .and_then(|c| c.extract_uuid())
                        .or_else(|| visuals.player.as_ref().and_then(|p| p.extract_uuid()))
                })
            });

        let uuid = match uuid {
            Some(u) => u,
            None => {
                // Fallback sur le logo par défaut via l'API REST
                if let Some(base) = server_base_url {
                    let logo_url = format!(
                        "{}/api/radiofrance/default-logo",
                        base.trim_end_matches('/')
                    );
                    return (Some(logo_url), None);
                }
                return (None, None);
            }
        };

        // URL haute résolution
        let cover_url = ImageSize::Large.build_url(&uuid);

        // Tenter de cacher la cover
        match cache.add_from_url(&cover_url, Some("radiofrance")).await {
            Ok(pk) => {
                // Construire l'URL publique si server_base_url est fourni
                let public_url = server_base_url
                    .map(|base| format!("{}/covers/{}", base.trim_end_matches('/'), pk));

                (public_url.or(Some(cover_url)), Some(pk))
            }
            Err(e) => {
                tracing::warn!("Failed to cache Radio France cover: {}", e);
                // Fallback sur le logo par défaut via l'API REST en cas d'erreur
                if let Some(base) = server_base_url {
                    let logo_url = format!(
                        "{}/api/radiofrance/default-logo",
                        base.trim_end_matches('/')
                    );
                    (Some(logo_url), None)
                } else {
                    (Some(cover_url), None)
                }
            }
        }
    }

    /// Construit la ressource stream
    fn build_stream_resource(metadata: &LiveResponse) -> Resource {
        // Trouver le meilleur stream HiFi
        let best_stream = metadata.now.media.best_hifi_stream();

        let (url, protocol_info, sample_frequency, nr_audio_channels) = match best_stream {
            Some(stream) => {
                let protocol_info = match stream.format {
                    StreamFormat::Aac => "http-get:*:audio/aac:*".to_string(),
                    StreamFormat::Hls => "http-get:*:application/vnd.apple.mpegurl:*".to_string(),
                    StreamFormat::Mp3 => "http-get:*:audio/mpeg:*".to_string(),
                };

                let sample_freq = match stream.format {
                    StreamFormat::Aac => Some("48000".to_string()),
                    _ => None,
                };

                let channels = match stream.format {
                    StreamFormat::Aac | StreamFormat::Mp3 => Some("2".to_string()),
                    _ => None,
                };

                (stream.url.clone(), protocol_info, sample_freq, channels)
            }
            None => {
                // Fallback : pas de stream trouvé
                (
                    String::new(),
                    "http-get:*:audio/aac:*".to_string(),
                    None,
                    None,
                )
            }
        };

        Resource {
            protocol_info,
            bits_per_sample: None,
            sample_frequency,
            nr_audio_channels,
            duration: None, // Stream live = pas de durée
            url,
        }
    }

    /// Retourne l'URL du stream
    pub fn stream_url(&self) -> Option<&str> {
        self.stream_item.resources.first().map(|r| r.url.as_str())
    }

    /// Retourne le titre actuel
    pub fn current_title(&self) -> &str {
        &self.stream_item.title
    }

    /// Retourne l'artiste actuel
    pub fn current_artist(&self) -> Option<&str> {
        self.stream_item.artist.as_deref()
    }
}

// ============================================================================
// Helpers pour le renommage France Bleu → ICI
// ============================================================================

impl Station {
    /// Retourne le nom d'affichage avec renommage France Bleu → ICI
    ///
    /// Les slugs sont conservés (francebleu_alsace) mais l'affichage
    /// utilise "ICI" (ICI Alsace).
    pub fn display_name(&self) -> &str {
        // Le renommage est déjà fait lors de la découverte via l'API
        // qui retourne directement "ICI Alsace" etc.
        &self.name
    }

    /// Vérifie si c'est une radio ICI (ex-France Bleu locale)
    pub fn is_ici_radio(&self) -> bool {
        self.name.starts_with("ICI ") || self.slug.starts_with("francebleu_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_groups_organization() {
        let stations = vec![
            Station::main("franceculture", "France Culture"),
            Station::main("fip", "FIP"),
            Station::webradio("fip_rock", "FIP Rock", "fip"),
            Station::webradio("fip_jazz", "FIP Jazz", "fip"),
            Station::local_radio("francebleu_alsace", "ICI Alsace", "Alsace", 1),
        ];

        let groups = StationGroups::from_stations(stations);

        assert_eq!(groups.standalone.len(), 1);
        assert_eq!(groups.standalone[0].slug, "franceculture");

        assert_eq!(groups.with_webradios.len(), 1);
        assert_eq!(groups.with_webradios[0].main.slug, "fip");
        assert_eq!(groups.with_webradios[0].webradios.len(), 2);

        assert_eq!(groups.local_radios.len(), 1);
        assert_eq!(groups.local_radios[0].slug, "francebleu_alsace");

        assert_eq!(groups.total_count(), 5);
    }

    #[test]
    fn test_station_display_name() {
        let station = Station::local_radio("francebleu_alsace", "ICI Alsace", "Alsace", 1);
        assert_eq!(station.display_name(), "ICI Alsace");
        assert!(station.is_ici_radio());

        let main = Station::main("franceculture", "France Culture");
        assert_eq!(main.display_name(), "France Culture");
        assert!(!main.is_ici_radio());
    }
}
