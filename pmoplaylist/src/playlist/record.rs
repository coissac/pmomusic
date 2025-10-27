//! Record : entrée dans la playlist pointant vers le cache audio

use std::time::{Duration, SystemTime};

/// Un enregistrement dans la playlist
///
/// Contient uniquement une référence (pk) vers une entrée dans pmoaudiocache
/// et des informations de gestion (timestamp, TTL).
#[derive(Debug, Clone)]
pub struct Record {
    /// Clé primaire dans pmoaudiocache
    pub cache_pk: String,
    
    /// Timestamp d'ajout à la playlist (en nanosecondes depuis epoch)
    pub added_at: SystemTime,
    
    /// Durée de vie optionnelle (surcharge le TTL par défaut)
    pub ttl: Option<Duration>,
}

impl Record {
    /// Crée un nouveau record
    pub fn new(cache_pk: String) -> Self {
        Self {
            cache_pk,
            added_at: SystemTime::now(),
            ttl: None,
        }
    }
    
    /// Crée un record avec un TTL personnalisé
    pub fn with_ttl(cache_pk: String, ttl: Duration) -> Self {
        Self {
            cache_pk,
            added_at: SystemTime::now(),
            ttl: Some(ttl),
        }
    }
    
    /// Vérifie si le record est expiré
    pub fn is_expired(&self, default_ttl: Option<Duration>) -> bool {
        let now = SystemTime::now();
        let age = now.duration_since(self.added_at).unwrap_or_default();
        
        if let Some(ttl) = self.ttl {
            age >= ttl
        } else if let Some(default_ttl) = default_ttl {
            age >= default_ttl
        } else {
            false
        }
    }
    
    /// Retourne le timestamp en nanosecondes depuis epoch
    pub fn added_at_nanos(&self) -> i64 {
        self.added_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64
    }
}
