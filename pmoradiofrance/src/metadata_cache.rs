//! Cache de métadonnées pour Radio France avec TTL et système d'événements
//!
//! Ce module fournit un cache centralisé pour les métadonnées des stations Radio France:
//! - Cache in-memory avec TTL basé sur `end_time` de l'API
//! - Gestion automatique du cache de covers via pmocovers
//! - Système d'événements pour la synchronisation GENA
//! - Cache persistant des stations via pmoconfig
//!
//! # Architecture
//!
//! - `CachedMetadata` : Métadonnées simplifiées pour construire un DIDL
//! - `MetadataCache` : Gère le cache in-memory + cache persistant + événements

use crate::client::RadioFranceClient;
use crate::config_ext::{RadioFranceConfigExt, StationInfo};
use crate::error::Result;
use crate::models::{ImageSize, LiveResponse, Station};
use pmoconfig::Config;
#[cfg(feature = "playlist")]
use pmodidl::{Container, Item, Resource};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[cfg(feature = "cache")]
use pmocovers::Cache as CoverCache;

#[cfg(feature = "cache")]
use pmocache::cache_trait::FileCache;

// ============================================================================
// CachedMetadata
// ============================================================================

/// Métadonnées simplifiées pour construire un DIDL de playlist à un item
///
/// Contient UNIQUEMENT les données nécessaires pour remplir le DIDL.
/// Construit depuis `LiveResponse` avec les règles de mapping RF → UPnP.
#[derive(Debug, Clone)]
pub struct CachedMetadata {
    /// Slug de la station
    pub slug: String,

    // Champs pour le DIDL (playlist + item)
    pub title: String,
    pub creator: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub class: String,
    pub album_art: Option<String>,    // URL publique de la cover
    pub album_art_pk: Option<String>, // PK dans pmocovers

    // Resource (stream)
    pub stream_url: String,
    pub protocol_info: String,
    pub sample_frequency: Option<String>,
    pub nr_audio_channels: Option<String>,
    pub duration: Option<String>, // Calculé depuis end_time

    // TTL = end_time de l'API Radio France
    pub end_time: Option<u64>, // Unix timestamp

    /// Timestamp de la dernière récupération (pour TTL minimum)
    pub fetched_at: u64,
}

impl CachedMetadata {
    /// Parse depuis LiveResponse avec gestion automatique du cache de covers
    ///
    /// IMPORTANT: Préserve les règles de mapping RF → UPnP existantes:
    /// - Radio musicale avec song → métadonnées du morceau
    /// - Radio parlée → agrégation émission/producteur
    /// - Évite duplications du nom de station
    /// - Calcule duration depuis end_time
    #[cfg(feature = "cache")]
    pub async fn from_live_response(
        station: &Station,
        live: &LiveResponse,
        cover_cache: &Arc<CoverCache>,
        server_base_url: &str,
    ) -> Result<Self> {
        // 1. Extraire les champs metadata avec les règles RF → UPnP
        let (title, creator, artist, album, genre, class) =
            Self::extract_metadata_fields(station, live);

        // 2. Gestion de la cover avec cache
        let (album_art, album_art_pk) = Self::cache_cover(live, cover_cache, server_base_url).await;

        // 3. Construction de la ressource (stream)
        let (stream_url, protocol_info, sample_frequency, nr_audio_channels, duration) =
            Self::build_stream_resource(live, &station.slug, server_base_url);

        // 4. TTL = end_time
        let end_time = live.now.end_time;
        let fetched_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(Self {
            slug: station.slug.clone(),
            title,
            creator,
            artist,
            album,
            genre,
            class,
            album_art,
            album_art_pk,
            stream_url,
            protocol_info,
            sample_frequency,
            nr_audio_channels,
            duration,
            end_time,
            fetched_at,
        })
    }

    /// Extrait les champs de métadonnées selon le type de radio
    ///
    /// RÈGLES DE MAPPING (préservées du code existant):
    /// - Radio musicale avec song → titre/artiste/album du morceau
    /// - Radio parlée → agrégation émission/producteur
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
            let song_artist = if song.interpreters.is_empty() {
                None
            } else {
                Some(song.artists_display())
            };

            // Artist affiché = "Station - Artiste du morceau" pour identifier la radio
            // Éviter la duplication si l'artiste est égal au nom de la station
            let artist = if let Some(ref art) = song_artist {
                if art != &station.name {
                    Some(format!("{} - {}", station.name, art))
                } else {
                    Some(station.name.clone())
                }
            } else {
                Some(station.name.clone())
            };

            let album = song.release.title.clone();
            let creator = song_artist; // Creator reste l'artiste du morceau
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
                station.name.clone()
            };

            // Artist/Creator = "{Station} - {Subtitle}"
            // Éviter la duplication si subtitle == nom de la station
            let artist = if !second.is_empty() && second != station.name {
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

    /// Cache la cover et retourne (url_publique, pk)
    #[cfg(feature = "cache")]
    async fn cache_cover(
        metadata: &LiveResponse,
        cache: &Arc<CoverCache>,
        server_base_url: &str,
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
                let logo_url = format!(
                    "{}/api/radiofrance/default-logo",
                    server_base_url.trim_end_matches('/')
                );
                return (Some(logo_url), None);
            }
        };

        // URL haute résolution
        let cover_url = ImageSize::Large.build_url(&uuid);

        // Tenter de cacher la cover
        match cache.add_from_url(&cover_url, Some("radiofrance")).await {
            Ok(pk) => {
                // Construire l'URL publique
                // Note: add_from_url() lance le téléchargement complet en arrière-plan.
                // L'URL est valide immédiatement — si le fichier n'est pas encore prêt,
                // le client web doit réessayer (retry avec backoff).
                let public_url = pmocache::covers_absolute_url_for(&pk, None);

                #[cfg(feature = "logging")]
                tracing::debug!(
                    "Cached cover - UUID: {}, PK: {}, public_url: {}",
                    uuid,
                    pk,
                    public_url
                );

                (Some(public_url), Some(pk))
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                tracing::warn!("Failed to cache Radio France cover UUID {}: {}", uuid, e);
                // Fallback sur le logo par défaut en cas d'erreur
                let logo_url = format!(
                    "{}/api/radiofrance/default-logo",
                    server_base_url.trim_end_matches('/')
                );
                (Some(logo_url), None)
            }
        }
    }

    /// Construit la ressource stream avec URL du proxy
    fn build_stream_resource(
        metadata: &LiveResponse,
        station_slug: &str,
        server_base_url: &str,
    ) -> (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        // Calculer la durée restante (maintenant -> end_time)
        let duration = if let Some(end) = metadata.now.end_time {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if end > now {
                let duration_secs = end - now;
                let hours = duration_secs / 3600;
                let minutes = (duration_secs % 3600) / 60;
                let seconds = duration_secs % 60;
                Some(format!("{}:{:02}:{:02}", hours, minutes, seconds))
            } else {
                None
            }
        } else {
            None
        };

        // URL du proxy PMOMusic
        let url = format!(
            "{}/api/radiofrance/{}/stream",
            server_base_url.trim_end_matches('/'),
            station_slug
        );

        // Déterminer le protocol_info et caractéristiques audio
        let best_stream = metadata.now.media.best_hifi_stream();

        let (protocol_info, sample_frequency, nr_audio_channels) = match best_stream {
            Some(stream) => {
                let protocol_info = match stream.format {
                    crate::models::StreamFormat::Aac => "http-get:*:audio/aac:*".to_string(),
                    crate::models::StreamFormat::Hls => {
                        "http-get:*:application/vnd.apple.mpegurl:*".to_string()
                    }
                    crate::models::StreamFormat::Mp3 => "http-get:*:audio/mpeg:*".to_string(),
                };

                let sample_freq = match stream.format {
                    crate::models::StreamFormat::Aac => Some("48000".to_string()),
                    _ => None,
                };

                let channels = match stream.format {
                    crate::models::StreamFormat::Aac | crate::models::StreamFormat::Mp3 => {
                        Some("2".to_string())
                    }
                    _ => None,
                };

                (protocol_info, sample_freq, channels)
            }
            None => {
                // Fallback
                ("http-get:*:audio/aac:*".to_string(), None, None)
            }
        };

        (
            url,
            protocol_info,
            sample_frequency,
            nr_audio_channels,
            duration,
        )
    }

    /// Construit un Container DIDL de playlist à un item
    ///
    /// La playlist et l'item ont EXACTEMENT les mêmes métadonnées
    #[cfg(feature = "playlist")]
    pub fn to_didl(&self, playlist_id: &str, parent_id: &str) -> Container {
        // Calculer la duration dynamiquement (temps restant jusqu'à end_time)
        let duration = if let Some(end) = self.end_time {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if end > now {
                let duration_secs = end - now;
                let hours = duration_secs / 3600;
                let minutes = (duration_secs % 3600) / 60;
                let seconds = duration_secs % 60;
                let dur = format!("{}:{:02}:{:02}", hours, minutes, seconds);

                #[cfg(feature = "logging")]
                tracing::debug!(
                    "Duration calculated for {}: {} (end_time: {}, now: {}, remaining: {}s)",
                    self.slug,
                    dur,
                    end,
                    now,
                    duration_secs
                );

                Some(dur)
            } else {
                #[cfg(feature = "logging")]
                tracing::warn!(
                    "Duration expired for {}: end_time {} < now {}",
                    self.slug,
                    end,
                    now
                );
                None
            }
        } else {
            #[cfg(feature = "logging")]
            tracing::warn!("No end_time for {}, duration will be None", self.slug);
            None
        };

        let item = Item {
            id: format!("{}:stream", playlist_id),
            parent_id: playlist_id.to_string(),
            restricted: Some("1".to_string()),
            title: self.title.clone(),
            creator: self.creator.clone(),
            class: self.class.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            genre: self.genre.clone(),
            album_art: self.album_art.clone(),
            album_art_pk: self.album_art_pk.clone(),
            date: None,
            original_track_number: None,
            resources: vec![Resource {
                protocol_info: self.protocol_info.clone(),
                bits_per_sample: None,
                sample_frequency: self.sample_frequency.clone(),
                nr_audio_channels: self.nr_audio_channels.clone(),
                duration,
                url: self.stream_url.clone(),
            }],
            descriptions: vec![],
        };

        // Container de playlist avec LES MÊMES métadonnées
        Container {
            id: playlist_id.to_string(),
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            child_count: Some("1".to_string()),
            searchable: Some("0".to_string()),
            title: self.title.clone(),
            class: "object.container.playlistContainer".to_string(),
            artist: self.artist.clone(),
            album_art: self.album_art.clone(),
            containers: vec![],
            items: vec![item],
        }
    }

    /// TTL minimum quand l'API ne fournit pas de end_time (3 minutes)
    pub const FALLBACK_TTL_SECS: u64 = 3 * 60;

    /// Vérifie si le TTL est dépassé
    ///
    /// - Si `end_time` est fourni par l'API : expire exactement quand l'émission se termine
    /// - Sinon : expire après `FALLBACK_TTL_SECS` depuis le dernier fetch
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Some(end_time) = self.end_time {
            now >= end_time
        } else {
            now >= self.fetched_at + Self::FALLBACK_TTL_SECS
        }
    }
}

// ============================================================================
// MetadataCache
// ============================================================================

/// Type de callback pour les notifications de mise à jour
pub type MetadataUpdateCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Cache de métadonnées avec TTL et système d'événements
///
/// Gère:
/// - Cache in-memory des métadonnées (HashMap avec TTL)
/// - Cache persistant des stations (via pmoconfig)
/// - Système d'événements (subscribe/notify)
pub struct MetadataCache {
    /// Cache in-memory slug -> CachedMetadata
    cache: Arc<RwLock<HashMap<String, CachedMetadata>>>,
    /// Client HTTP Radio France
    client: RadioFranceClient,
    /// Cache de covers (pmocovers)
    #[cfg(feature = "cache")]
    cover_cache: Arc<CoverCache>,
    /// URL de base du serveur
    server_base_url: String,
    /// Configuration (pour cache persistant des stations)
    config: Arc<Config>,
    /// Abonnés aux événements
    subscribers: Arc<RwLock<Vec<MetadataUpdateCallback>>>,
}

impl MetadataCache {
    /// Constructeur avec tous les paramètres obligatoires
    ///
    /// Charge automatiquement le mapping station depuis pmoconfig (si valide).
    /// Si le cache est absent ou expiré, utilise les valeurs hardcodées du client.
    #[cfg(feature = "cache")]
    pub fn new(
        client: RadioFranceClient,
        cover_cache: Arc<CoverCache>,
        server_base_url: String,
        config: Arc<Config>,
    ) -> Self {
        // Charger le mapping depuis pmoconfig et l'injecter dans le client
        if let Ok(Some(mapping)) = config.get_radiofrance_station_mapping() {
            #[cfg(feature = "logging")]
            tracing::info!(
                "Loaded station mapping from config ({} entries)",
                mapping.len()
            );
            client.set_station_mapping(mapping);
        } else {
            // Pas de cache persistant : persister le mapping par défaut
            let mapping = client.get_station_mapping();
            let _ = config.set_radiofrance_station_mapping(&mapping);
            #[cfg(feature = "logging")]
            tracing::info!(
                "Initialized station mapping from defaults ({} entries)",
                mapping.len()
            );
        }

        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            client,
            cover_cache,
            server_base_url,
            config,
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Récupère les métadonnées (rafraîchit si TTL expiré)
    ///
    /// # Logique
    ///
    /// 1. Vérifie le cache in-memory
    /// 2. Si valide, retourne directement
    /// 3. Sinon, appelle API Radio France
    /// 4. Met à jour le cache
    /// 5. Notifie les abonnés
    /// 6. Retourne les métadonnées
    ///
    /// # Graceful degradation
    ///
    /// Si l'API Radio France est down, retourne les données expirées du cache
    #[cfg(feature = "cache")]
    pub async fn get(&self, slug: &str) -> Result<CachedMetadata> {
        // 1. Vérifie le cache
        {
            let cache = self.cache.read().await;
            if let Some(metadata) = cache.get(slug) {
                if !metadata.is_expired() {
                    return Ok(metadata.clone());
                }
            }
        }

        // 2. TTL expiré ou absent: appelle API Radio France
        let live_response = match self.client.live_metadata(slug).await {
            Ok(resp) => resp,
            Err(e) => {
                // Si station inconnue → tenter une re-découverte du mapping
                let is_unknown = e.to_string().contains("Unknown station slug");
                if is_unknown {
                    #[cfg(feature = "logging")]
                    tracing::warn!("Unknown station '{}', attempting rediscovery", slug);

                    match self.client.rediscover_station(slug).await {
                        Ok(_) => {
                            // Persister le mapping mis à jour
                            self.persist_station_mapping();
                            // Réessayer la requête
                            match self.client.live_metadata(slug).await {
                                Ok(resp) => resp,
                                Err(e2) => return Err(e2),
                            }
                        }
                        Err(discover_err) => {
                            #[cfg(feature = "logging")]
                            tracing::error!(
                                "Rediscovery failed for '{}': {}",
                                slug,
                                discover_err
                            );
                            return Err(e);
                        }
                    }
                } else {
                    // Graceful degradation: retourner les données expirées si API down
                    let cache = self.cache.read().await;
                    if let Some(metadata) = cache.get(slug) {
                        #[cfg(feature = "logging")]
                        tracing::warn!(
                            "API Radio France down for {}, using expired cache: {}",
                            slug,
                            e
                        );
                        return Ok(metadata.clone());
                    }
                    return Err(e);
                }
            }
        };

        // 3. Récupérer le nom de la station depuis la liste des stations
        let station_name = {
            let stations = self.get_stations().await.unwrap_or_default();
            stations
                .iter()
                .find(|s| s.slug == slug)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| slug.to_string())
        };

        // 4. Parse LiveResponse -> CachedMetadata
        let metadata = CachedMetadata::from_live_response(
            &Station {
                slug: slug.to_string(),
                name: station_name,
            },
            &live_response,
            &self.cover_cache,
            &self.server_base_url,
        )
        .await?;

        // 4. Met à jour le cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(slug.to_string(), metadata.clone());
        }

        // 5. Notifie les abonnés (async)
        self.notify_async(slug).await;

        // 6. Retourne les métadonnées
        Ok(metadata)
    }

    /// Récupère la liste des stations (cache persistant via pmoconfig, TTL 7 jours)
    ///
    /// # Logique
    ///
    /// 1. Essaie de lire depuis pmoconfig (cache TTL 7 jours)
    /// 2. Si cache valide, retourne sans appel réseau
    /// 3. Sinon, découvre via scraping et met à jour pmoconfig
    pub async fn get_stations(&self) -> Result<Vec<Station>> {
        // 1. Essayer le cache persistant
        if let Ok(Some(stations)) = self.config.get_radiofrance_stations_cached() {
            #[cfg(feature = "logging")]
            tracing::debug!("Using cached station list ({} stations)", stations.len());
            return Ok(stations);
        }

        // 2. Cache miss : découverte via scraping
        #[cfg(feature = "logging")]
        tracing::info!("Station cache miss, discovering via web scraping...");

        let stations = self.client.discover_all_stations().await?;

        // 3. Persister dans pmoconfig
        if let Err(e) = self.config.set_radiofrance_cached_stations(&stations) {
            #[cfg(feature = "logging")]
            tracing::warn!("Failed to cache station list: {}", e);
        }

        Ok(stations)
    }

    /// Récupère les métadonnées live brutes de l'API (sans cache)
    ///
    /// Cette méthode est utilisée par l'API REST pour retourner
    /// la réponse complète de l'API Radio France
    pub async fn get_live_metadata(&self, slug: &str) -> Result<LiveResponse> {
        self.client.live_metadata(slug).await
    }

    /// Récupère l'URL du stream HiFi pour une station
    pub async fn get_stream_url(&self, slug: &str) -> Result<String> {
        self.client.get_hifi_stream_url(slug).await
    }

    /// Force une re-découverte du mapping pour un slug donné
    ///
    /// Utilisé quand un stream retourne une erreur HTTP (404, connexion refusée, etc.)
    /// Persiste le mapping mis à jour dans pmoconfig.
    pub async fn handle_stream_failure(&self, slug: &str) {
        #[cfg(feature = "logging")]
        tracing::warn!("Stream failure for '{}', triggering rediscovery", slug);

        match self.client.rediscover_station(slug).await {
            Ok((id, url)) => {
                #[cfg(feature = "logging")]
                tracing::info!(
                    "Re-discovered station '{}': id={}, url={}",
                    slug,
                    id,
                    url
                );
                self.persist_station_mapping();
                // Invalider le cache mémoire pour forcer un refresh des métadonnées
                let mut cache = self.cache.write().await;
                cache.remove(slug);
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                tracing::error!("Rediscovery failed for '{}': {}", slug, e);
            }
        }
    }

    /// Persiste le mapping courant dans pmoconfig
    fn persist_station_mapping(&self) {
        let mapping = self.client.get_station_mapping();
        if let Err(e) = self.config.set_radiofrance_station_mapping(&mapping) {
            #[cfg(feature = "logging")]
            tracing::warn!("Failed to persist station mapping: {}", e);
        }
    }

    /// Met à jour manuellement le mapping d'une station et le persiste
    pub fn update_station_mapping(&self, slug: &str, station_id: u32, stream_url: String) {
        self.client
            .update_station_entry(slug, station_id, stream_url.clone());

        // Persister dans pmoconfig
        let info = StationInfo {
            station_id,
            stream_url,
        };
        if let Err(e) = self.config.upsert_radiofrance_station_info(slug, info) {
            #[cfg(feature = "logging")]
            tracing::warn!("Failed to upsert station info for '{}': {}", slug, e);
        }
    }

    /// S'abonner aux changements de métadonnées
    ///
    /// Le callback sera appelé avec le slug chaque fois que
    /// les métadonnées de ce slug sont rafraîchies
    pub fn subscribe(&self, callback: MetadataUpdateCallback) {
        // Spawn une tâche pour éviter le blocking dans le runtime
        let subscribers = self.subscribers.clone();
        tokio::spawn(async move {
            let mut subs = subscribers.write().await;
            subs.push(callback);
        });
    }

    /// Notifier tous les abonnés qu'un slug a été mis à jour (version async)
    async fn notify_async(&self, slug: &str) {
        // Clone pour éviter de bloquer longtemps
        let callbacks: Vec<_> = {
            let subscribers = self.subscribers.read().await;
            subscribers.clone()
        };

        for callback in callbacks.iter() {
            callback(slug);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_metadata_is_expired() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let base = CachedMetadata {
            slug: "test".to_string(),
            title: "Test".to_string(),
            creator: None,
            artist: None,
            album: None,
            genre: None,
            class: "test".to_string(),
            album_art: None,
            album_art_pk: None,
            stream_url: "".to_string(),
            protocol_info: "".to_string(),
            sample_frequency: None,
            nr_audio_channels: None,
            duration: None,
            end_time: Some(0), // Dans le passé
            fetched_at: now,
        };

        // end_time dans le passé → expiré
        assert!(base.is_expired());

        // end_time dans le futur → non expiré
        let not_expired = CachedMetadata { end_time: Some(now + 3600), ..base.clone() };
        assert!(!not_expired.is_expired());

        // Pas de end_time, fetched_at récent → non expiré (fallback TTL)
        let no_end_fresh = CachedMetadata { end_time: None, fetched_at: now, ..base.clone() };
        assert!(!no_end_fresh.is_expired());

        // Pas de end_time, fetched_at ancien → expiré
        let no_end_old = CachedMetadata {
            end_time: None,
            fetched_at: now - CachedMetadata::FALLBACK_TTL_SECS - 1,
            ..base
        };
        assert!(no_end_old.is_expired());
    }
}
