//! Structures pour organiser les stations Radio France en groupes
//!
//! Ce module fournit les structures nécessaires pour organiser les stations
//! Radio France en groupes hiérarchiques et construire des containers DIDL.
//!
//! # Architecture
//!
//! Chaque niveau a deux méthodes :
//! - `to_didl()` : Retourne le container COMPLET avec tout son contenu
//! - `to_stub()` : Retourne juste les infos minimales pour apparaître dans la liste du parent
//!
//! Règle : `to_didl()` du niveau N appelle `to_stub()` du niveau N+1

use crate::error::Result;
use crate::metadata_cache::MetadataCache;
use crate::models::Station;
use pmodidl::Container;

// ============================================================================
// Niveau 0: StationGroups (racine "radiofrance")
// ============================================================================

/// Groupes de stations - Niveau 0
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StationGroups {
    pub groups: Vec<StationGroup>,
}

impl StationGroups {
    /// Organise une liste de stations en groupes
    pub fn from_stations(stations: Vec<Station>) -> Self {
        use std::collections::HashMap;

        let mut groups_map: HashMap<String, Vec<Station>> = HashMap::new();
        let mut ici_stations = Vec::new();

        for station in stations {
            // Détecter les radios locales ICI (ex-France Bleu)
            if station.slug.starts_with("francebleu_") {
                ici_stations.push(station);
                continue;
            }

            // Filtrer "francebleu" générique (pas une vraie station)
            if station.slug == "francebleu" {
                continue;
            }

            // Détecter le groupe de la station (par préfixe avant _)
            let group_key = if let Some(pos) = station.slug.find('_') {
                station.slug[..pos].to_string()
            } else {
                station.slug.clone()
            };

            groups_map.entry(group_key).or_default().push(station);
        }

        // Construire les groupes
        let mut groups: Vec<StationGroup> = groups_map
            .into_iter()
            .map(|(group_key, mut stations)| {
                // Trier : station principale en premier, puis webradios par ordre alphabétique
                stations.sort_by(|a, b| {
                    let a_is_main = a.slug == group_key;
                    let b_is_main = b.slug == group_key;

                    match (a_is_main, b_is_main) {
                        (true, false) => std::cmp::Ordering::Less, // Main avant webradios
                        (false, true) => std::cmp::Ordering::Greater, // Webradios après main
                        _ => a.name.cmp(&b.name),                  // Sinon tri alphabétique
                    }
                });

                StationGroup {
                    stations,
                    group_name: None,
                    group_slug: None,
                }
            })
            .collect();

        // Ajouter le groupe ICI si on a des radios locales
        if !ici_stations.is_empty() {
            ici_stations.sort_by(|a, b| a.name.cmp(&b.name));
            groups.push(StationGroup {
                stations: ici_stations,
                group_name: Some("Radios ICI".to_string()),
                group_slug: Some("ici".to_string()),
            });
        }

        // Trier les groupes par nom de la station principale
        groups.sort_by(|a, b| a.stations[0].name.cmp(&b.stations[0].name));

        Self { groups }
    }

    /// Niveau 0: to_didl() retourne le container "radiofrance" avec tous les groupes
    ///
    /// Appelle to_stub() sur chaque StationGroup
    pub async fn to_didl(
        &self,
        metadata_cache: &MetadataCache,
        server_base_url: &str,
    ) -> Result<Container> {
        let mut containers = Vec::new();

        for group in &self.groups {
            let container = group.to_stub(metadata_cache, server_base_url).await?;
            containers.push(container);
        }

        Ok(Container {
            id: "radiofrance".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(containers.len().to_string()),
            searchable: Some("0".to_string()),
            title: "Radio France".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers,
            items: vec![],
        })
    }
}

// ============================================================================
// Niveau 1: StationGroup (groupe de stations)
// ============================================================================

/// Groupe de stations - Niveau 1
///
/// Index 0 = première station du groupe (donne le nom/slug par défaut)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StationGroup {
    pub stations: Vec<Station>,
    /// Nom personnalisé du groupe (optionnel, sinon utilise le nom de stations[0])
    pub group_name: Option<String>,
    /// Slug personnalisé du groupe (optionnel, sinon utilise le slug de stations[0])
    pub group_slug: Option<String>,
}

impl StationGroup {
    /// Retourne le nom du groupe (personnalisé ou nom de la première station)
    fn name(&self) -> &str {
        self.group_name.as_deref().unwrap_or(&self.stations[0].name)
    }

    /// Retourne le slug du groupe (personnalisé ou slug de la première station)
    fn slug(&self) -> &str {
        self.group_slug.as_deref().unwrap_or(&self.stations[0].slug)
    }

    /// Niveau 1: to_stub() retourne comment ce groupe apparaît dans la liste de StationGroups
    ///
    /// - Si 1 station: retourne une playlist avec métadonnées (pour avoir titre/artiste à jour)
    /// - Si plusieurs: retourne juste un container de groupe sans métadonnées
    pub async fn to_stub(
        &self,
        metadata_cache: &MetadataCache,
        server_base_url: &str,
    ) -> Result<Container> {
        if self.stations.len() == 1 {
            // Groupe à 1 station : retourner la playlist avec métadonnées
            self.stations[0]
                .to_stub(metadata_cache, server_base_url)
                .await
        } else {
            // Groupe multi-stations : juste le nom, pas de métadonnées
            let album_art = Some(format!(
                "{}/api/radiofrance/default-logo",
                server_base_url.trim_end_matches('/')
            ));

            Ok(Container {
                id: format!("radiofrance:group:{}", self.slug()),
                parent_id: "radiofrance".to_string(),
                restricted: Some("1".to_string()),
                child_count: Some(self.stations.len().to_string()),
                searchable: Some("0".to_string()),
                title: self.name().to_string(),
                class: "object.container".to_string(),
                artist: None,
                album_art,
                containers: vec![],
                items: vec![],
            })
        }
    }

    /// Niveau 1: to_didl() retourne le container du groupe avec TOUT son contenu
    ///
    /// - Si 1 station: retourne la playlist complète avec l'item stream
    /// - Si plusieurs: retourne le container avec toutes les playlists en stub
    pub async fn to_didl(
        &self,
        metadata_cache: &MetadataCache,
        server_base_url: &str,
    ) -> Result<Container> {
        if self.stations.len() == 1 {
            // Groupe à 1 station : retourner la playlist complète
            self.stations[0]
                .to_didl(metadata_cache, server_base_url)
                .await
        } else {
            // Groupe multi-stations : retourner un container avec toutes les playlists en stub
            let mut containers = Vec::new();

            for station in &self.stations {
                // Appeler to_stub() sur chaque station
                let playlist_stub = station.to_stub(metadata_cache, server_base_url).await?;
                containers.push(playlist_stub);
            }

            let album_art = Some(format!(
                "{}/api/radiofrance/default-logo",
                server_base_url.trim_end_matches('/')
            ));

            Ok(Container {
                id: format!("radiofrance:group:{}", self.slug()),
                parent_id: "radiofrance".to_string(),
                restricted: Some("1".to_string()),
                child_count: Some(containers.len().to_string()),
                searchable: Some("0".to_string()),
                title: self.name().to_string(),
                class: "object.container".to_string(),
                artist: None,
                album_art,
                containers,
                items: vec![],
            })
        }
    }
}

// ============================================================================
// Niveau 2: Station (playlist singleton)
// ============================================================================

impl Station {
    /// Niveau 2: to_stub() retourne comment cette station apparaît dans la liste d'un groupe
    ///
    /// Retourne un container de playlist vide avec métadonnées live
    pub async fn to_stub(
        &self,
        metadata_cache: &MetadataCache,
        _server_base_url: &str,
    ) -> Result<Container> {
        let playlist_id = format!("radiofrance:{}", self.slug);
        let parent_id = self.compute_parent_id();

        // Récupérer les métadonnées du cache
        let cached_metadata = metadata_cache.get(&self.slug).await?;

        // Construire juste le container de playlist (sans l'item stream)
        Ok(Container {
            id: playlist_id,
            parent_id,
            restricted: Some("1".to_string()),
            child_count: Some("1".to_string()),
            searchable: Some("0".to_string()),
            title: cached_metadata.title.clone(),
            class: "object.container.playlistContainer".to_string(),
            artist: cached_metadata.artist.clone(),
            album_art: cached_metadata.album_art.clone(),
            containers: vec![],
            items: vec![],
        })
    }

    /// Niveau 2: to_didl() retourne la playlist complète avec l'item stream
    ///
    /// Retourne un container de playlist avec 1 item stream dedans
    pub async fn to_didl(
        &self,
        metadata_cache: &MetadataCache,
        _server_base_url: &str,
    ) -> Result<Container> {
        // Récupérer les métadonnées du cache
        let cached_metadata = metadata_cache.get(&self.slug).await?;

        // Construire le container de playlist avec l'item via CachedMetadata::to_didl()
        let playlist_id = format!("radiofrance:{}", self.slug);
        let parent_id = self.compute_parent_id();

        Ok(cached_metadata.to_didl(&playlist_id, &parent_id))
    }

    /// Calcule le parent_id selon la position de la station
    fn compute_parent_id(&self) -> String {
        if self.slug.starts_with("francebleu_") {
            // Radio locale ICI (ex-France Bleu) : parent = groupe ICI
            "radiofrance:group:ici".to_string()
        } else if let Some(pos) = self.slug.find('_') {
            // Webradio : parent = groupe
            format!("radiofrance:group:{}", &self.slug[..pos])
        } else {
            // Station principale : parent = racine
            "radiofrance".to_string()
        }
    }
}
