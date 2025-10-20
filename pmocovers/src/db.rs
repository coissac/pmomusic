//! Module de compatibilité pour l'ancien module db
//!
//! Ce module réexporte les types de `pmocache::db` pour maintenir
//! la compatibilité avec l'API existante.

// Réexporter les types de pmocache
pub use pmocache::db::{CacheEntry, DB};
