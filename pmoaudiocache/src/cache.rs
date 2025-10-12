//! Module de gestion du cache de pistes audio
//!
//! Ce module gère le cache audio avec :
//! - Stockage immédiat des métadonnées en DB
//! - Conversion FLAC asynchrone en arrière-plan
//! - Service DIDL-Lite immédiat avant fin de conversion

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use pmodidl::{Item, Resource};

use crate::{
    db::{AudioDB, AudioCacheEntry},
    metadata::AudioMetadata,
};

/// Cache de pistes audio avec conversion asynchrone
///
/// Permet de servir les métadonnées immédiatement pendant que
/// la conversion FLAC s'effectue en arrière-plan.
#[derive(Debug)]
pub struct AudioCache {
    dir: PathBuf,
    pub(crate) db: Arc<AudioDB>,
    conversion_queue: Arc<Mutex<Vec<String>>>, // PKs en attente de conversion
}

impl AudioCache {
    /// Crée un nouveau cache audio
    ///
    /// # Arguments
    ///
    /// * `dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (nombre de pistes)
    pub fn new(dir: &str, limit: usize) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let db_path = PathBuf::from(dir).join("audio_cache.db");
        let db = Arc::new(AudioDB::init(&db_path)?);

        Ok(Self {
            dir: PathBuf::from(dir),
            db,
            conversion_queue: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Ajoute une piste audio depuis une URL
    ///
    /// **Phase 1 (immédiate) :** Télécharge et stocke les métadonnées en DB
    /// **Phase 2 (async) :** Conversion FLAC en arrière-plan
    ///
    /// Les métadonnées sont disponibles immédiatement via `get_metadata()`
    ///
    /// # Arguments
    ///
    /// * `url` - URL de la piste audio
    /// * `external_metadata` - Métadonnées optionnelles depuis le service (Qobuz, etc.)
    ///
    /// # Returns
    ///
    /// * `(pk, metadata)` - Clé et métadonnées (disponibles immédiatement)
    pub async fn add_from_url(
        &self,
        url: &str,
        external_metadata: Option<AudioMetadata>,
    ) -> Result<(String, AudioMetadata)> {
        let response = reqwest::get(url).await?;
        let data = response.bytes().await?;

        self.add_from_bytes(url, &data, external_metadata).await
    }

    /// Ajoute une piste depuis des données brutes
    ///
    /// # Phase 1 (immédiate, <1s)
    /// 1. Extraire métadonnées du fichier
    /// 2. Fusionner avec métadonnées externes si fournies
    /// 3. Stocker métadonnées en DB
    /// 4. Stocker fichier original temporairement
    ///
    /// # Phase 2 (asynchrone)
    /// 5. Conversion FLAC en arrière-plan
    /// 6. Mise à jour du statut de conversion
    ///
    /// # Arguments
    ///
    /// * `url` - URL source
    /// * `data` - Données audio brutes
    /// * `external_metadata` - Métadonnées optionnelles depuis le service
    pub async fn add_from_bytes(
        &self,
        url: &str,
        data: &[u8],
        external_metadata: Option<AudioMetadata>,
    ) -> Result<(String, AudioMetadata)> {
        let pk = pmocache::pk_from_url(url);

        // Phase 1 : Extraction et stockage immédiat des métadonnées
        let mut metadata = AudioMetadata::from_bytes(data)?;

        // Fusionner avec métadonnées externes si fournies (priorité aux externes)
        if let Some(external) = external_metadata {
            metadata = merge_metadata(metadata, external);
        }

        let collection = metadata.collection_key();

        // Stocker les métadonnées immédiatement en DB
        self.db.add(&pk, url, collection.as_deref(), &metadata)?;

        // Stocker le fichier original temporairement
        let temp_path = self.temp_file_path(&pk);
        tokio::fs::write(&temp_path, data).await?;

        // Phase 2 : Lancer la conversion asynchrone
        self.start_conversion(pk.clone(), temp_path).await;

        Ok((pk, metadata))
    }

    /// Lance la conversion FLAC en arrière-plan
    async fn start_conversion(&self, pk: String, temp_path: PathBuf) {
        let db = Arc::clone(&self.db);
        let final_path = self.flac_file_path(&pk);

        tokio::spawn(async move {
            // Marquer comme en cours de conversion
            let _ = db.update_conversion_status(&pk, "converting");

            // Conversion FLAC
            match tokio::fs::read(&temp_path).await {
                Ok(data) => {
                    match crate::flac::convert_to_flac(&data, None) {
                        Ok(flac_data) => {
                            // Écrire le fichier FLAC
                            if let Ok(_) = tokio::fs::write(&final_path, flac_data).await {
                                // Supprimer le fichier temporaire
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                // Marquer comme complété
                                let _ = db.update_conversion_status(&pk, "completed");
                            } else {
                                let _ = db.update_conversion_status(&pk, "failed");
                            }
                        }
                        Err(_) => {
                            let _ = db.update_conversion_status(&pk, "failed");
                        }
                    }
                }
                Err(_) => {
                    let _ = db.update_conversion_status(&pk, "failed");
                }
            }
        });
    }

    /// Récupère les métadonnées d'une piste (disponible immédiatement)
    ///
    /// Cette méthode retourne les métadonnées même si la conversion FLAC
    /// n'est pas terminée. Permet de servir du DIDL-Lite immédiatement.
    pub async fn get_metadata(&self, pk: &str) -> Result<AudioMetadata> {
        self.db.update_hit(pk)?;
        let entry = self.db.get(pk)?;
        Ok(entry.metadata)
    }

    /// Récupère les métadonnées et le statut de conversion
    pub async fn get_entry(&self, pk: &str) -> Result<AudioCacheEntry> {
        self.db.update_hit(pk)?;
        Ok(self.db.get(pk)?)
    }

    /// Récupère le chemin du fichier audio (attend la fin de conversion si nécessaire)
    pub async fn get_file(&self, pk: &str) -> Result<PathBuf> {
        let entry = self.db.get(pk)?;

        match entry.conversion_status.as_str() {
            "completed" => {
                let flac_path = self.flac_file_path(pk);
                if flac_path.exists() {
                    self.db.update_hit(pk)?;
                    Ok(flac_path)
                } else {
                    Err(anyhow!("File not found"))
                }
            }
            "converting" | "pending" => {
                // Attendre un court instant (permet de servir rapidement après 1 seconde)
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // Re-vérifier le statut
                let entry = self.db.get(pk)?;
                if entry.conversion_status == "completed" {
                    let flac_path = self.flac_file_path(pk);
                    if flac_path.exists() {
                        self.db.update_hit(pk)?;
                        return Ok(flac_path);
                    }
                }

                Err(anyhow!("Conversion not completed yet"))
            }
            "failed" => Err(anyhow!("Conversion failed")),
            _ => Err(anyhow!("Unknown conversion status")),
        }
    }

    /// Génère un objet DIDL-Lite pour une piste
    ///
    /// Peut être appelé immédiatement après `add_from_bytes()` même si
    /// la conversion n'est pas terminée.
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé de la piste
    /// * `base_url` - URL de base du serveur (ex: "http://localhost:8080")
    pub async fn get_didl(&self, pk: &str, base_url: &str) -> Result<String> {
        let entry = self.get_entry(pk).await?;
        let metadata = entry.metadata;

        let stream_url = format!("{}/audio/tracks/{}/stream", base_url, pk);
        let duration = if let Some(duration_secs) = metadata.duration_secs {
            let hours = duration_secs / 3600;
            let minutes = (duration_secs % 3600) / 60;
            let seconds = duration_secs % 60;
            Some(format!("{}:{:02}:{:02}", hours, minutes, seconds))
        } else {
            None
        };

        let resource = Resource {
            protocol_info: "http-get:*:audio/flac:*".to_string(),
            bits_per_sample: None,
            sample_frequency: metadata.sample_rate.map(|sr| sr.to_string()),
            nr_audio_channels: metadata.channels.map(|c| c.to_string()),
            duration,
            url: stream_url,
        };

        let item = Item {
            id: pk.to_string(),
            parent_id: "0".to_string(),
            restricted: None,
            title: metadata.title.unwrap_or_default(),
            creator: None,
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: metadata.artist,
            album: metadata.album,
            genre: metadata.genre,
            album_art: None,
            album_art_pk: None,
            date: metadata.year.map(|y| format!("{:04}-01-01", y)),
            original_track_number: metadata.track_number.map(|n| n.to_string()),
            resources: vec![resource],
            descriptions: Vec::new(),
        };

        // Utiliser quick_xml pour serializer en XML
        let xml = quick_xml::se::to_string(&item)
            .map_err(|e| anyhow!("XML serialization error: {}", e))?;

        Ok(xml)
    }

    /// Récupère toutes les pistes d'une collection
    pub async fn get_collection(&self, collection: &str) -> Result<Vec<AudioCacheEntry>> {
        Ok(self.db.get_by_collection(collection)?)
    }

    /// Liste toutes les collections
    pub async fn list_collections(&self) -> Result<Vec<(String, usize)>> {
        let entries = self.db.get_all()?;
        let mut collections: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for entry in entries {
            if let Some(collection) = entry.collection {
                *collections.entry(collection).or_insert(0) += 1;
            }
        }

        let mut result: Vec<(String, usize)> = collections.into_iter().collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(result)
    }

    /// Purge le cache
    pub async fn purge(&self) -> Result<()> {
        // Supprimer tous les fichiers
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_file() {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }

        self.db.purge()?;
        Ok(())
    }

    /// Supprime une piste du cache
    ///
    /// Supprime les fichiers (temp et FLAC) et l'entrée de la base de données
    pub async fn delete(&self, pk: &str) -> Result<()> {
        // Supprimer les fichiers
        let temp_path = self.temp_file_path(pk);
        let flac_path = self.flac_file_path(pk);

        if temp_path.exists() {
            tokio::fs::remove_file(&temp_path).await?;
        }
        if flac_path.exists() {
            tokio::fs::remove_file(&flac_path).await?;
        }

        // Supprimer l'entrée DB
        self.db.delete(pk)?;
        Ok(())
    }

    /// Consolide le cache
    ///
    /// - Supprime les entrées DB sans fichiers correspondants
    /// - Supprime les fichiers sans entrées DB
    /// - Nettoie les conversions en échec
    pub async fn consolidate(&self) -> Result<()> {
        // Récupérer toutes les entrées
        let entries = self.db.get_all()?;

        // Supprimer les entrées sans fichiers ou en échec
        for entry in entries {
            let flac_path = self.flac_file_path(&entry.pk);
            let temp_path = self.temp_file_path(&entry.pk);

            // Si la conversion a échoué, supprimer l'entrée
            if entry.conversion_status == "failed" {
                self.delete(&entry.pk).await?;
                continue;
            }

            // Si le fichier FLAC devrait exister mais n'existe pas
            if entry.conversion_status == "completed" && !flac_path.exists() {
                self.db.delete(&entry.pk)?;
                if temp_path.exists() {
                    tokio::fs::remove_file(&temp_path).await?;
                }
            }
        }

        // Supprimer les fichiers orphelins (sans entrée DB)
        let mut dir_entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Ignorer le fichier de base de données
            if path == self.dir.join("audio_cache.db") {
                continue;
            }

            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                // Extraire le pk du nom de fichier
                let pk = if file_name.ends_with(".flac") {
                    file_name.trim_end_matches(".flac")
                } else if file_name.ends_with(".temp") {
                    file_name.trim_end_matches(".temp")
                } else {
                    continue;
                };

                // Si l'entrée n'existe pas en DB, supprimer le fichier
                if self.db.get(pk).is_err() {
                    tokio::fs::remove_file(path).await?;
                }
            }
        }

        Ok(())
    }

    /// Retourne le répertoire du cache
    pub fn cache_dir(&self) -> String {
        self.dir.to_string_lossy().to_string()
    }

    // Helpers privés
    fn temp_file_path(&self, pk: &str) -> PathBuf {
        self.dir.join(format!("{}.temp", pk))
    }

    fn flac_file_path(&self, pk: &str) -> PathBuf {
        self.dir.join(format!("{}.flac", pk))
    }
}

/// Fusionne les métadonnées du fichier avec les métadonnées externes
///
/// Priorité aux métadonnées externes (source de confiance : Qobuz, etc.)
fn merge_metadata(file_meta: AudioMetadata, external_meta: AudioMetadata) -> AudioMetadata {
    AudioMetadata {
        title: external_meta.title.or(file_meta.title),
        artist: external_meta.artist.or(file_meta.artist),
        album: external_meta.album.or(file_meta.album),
        year: external_meta.year.or(file_meta.year),
        track_number: external_meta.track_number.or(file_meta.track_number),
        track_total: external_meta.track_total.or(file_meta.track_total),
        disc_number: external_meta.disc_number.or(file_meta.disc_number),
        disc_total: external_meta.disc_total.or(file_meta.disc_total),
        genre: external_meta.genre.or(file_meta.genre),
        // Pour les infos techniques, on garde celles du fichier
        duration_secs: file_meta.duration_secs.or(external_meta.duration_secs),
        sample_rate: file_meta.sample_rate.or(external_meta.sample_rate),
        channels: file_meta.channels.or(external_meta.channels),
        bitrate: file_meta.bitrate.or(external_meta.bitrate),
    }
}
