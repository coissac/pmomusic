//! Extension trait pour accéder aux métadonnées audio de manière typée
//!
//! Ce module utilise la macro `define_metadata_properties!` de pmocache
//! pour générer automatiquement des méthodes d'accès typées aux métadonnées audio.

use crate::AudioConfig;
use pmocache::define_metadata_properties;

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
