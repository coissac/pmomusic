//! Extension trait pour accéder aux métadonnées audio de manière typée
//!
//! Ce module utilise la macro `define_metadata_properties!` de pmocache
//! pour générer automatiquement des méthodes d'accès typées aux métadonnées audio.

use crate::{AudioCacheTrackMetadata, AudioConfig};
use pmocache::define_metadata_properties;
use pmometadata::TrackMetadata;
use std::sync::Arc;
use tokio::sync::RwLock;

// Génération automatique du trait AudioMetadataExt avec toutes les propriétés audio
define_metadata_properties! {
    AudioMetadataExt for pmocache::Cache<AudioConfig> {
        // Métadonnées textuelles
        title: String as string,
        artist: String as string,
        album: String as string,
        album_artist: String as string,
        genre: String as string,
        composer: String as string,
        comment: String as string,

        // Métadonnées numériques (année, numéros de piste)
        year: i64 as i64,
        track_number: i64 as i64,
        disc_number: i64 as i64,
        total_tracks: i64 as i64,
        total_discs: i64 as i64,

        // Métadonnées techniques audio
        duration_secs: i64 as i64,
        sample_rate: i64 as i64,
        bitrate: i64 as i64,
        channels: i64 as i64,
        bit_depth: i64 as i64,
    }
}

/// Fournit un accès direct à une implémentation `TrackMetadata` basée sur le cache.
pub trait AudioTrackMetadataExt {
    fn track_metadata(&self, pk: impl Into<String>) -> Arc<RwLock<dyn TrackMetadata>>;
}

impl AudioTrackMetadataExt for Arc<pmocache::Cache<AudioConfig>> {
    fn track_metadata(&self, pk: impl Into<String>) -> Arc<RwLock<dyn TrackMetadata>> {
        let metadata = AudioCacheTrackMetadata::new(self.clone(), pk);
        Arc::new(RwLock::new(metadata))
    }
}
