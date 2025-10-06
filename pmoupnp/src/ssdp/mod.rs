//! # Module SSDP - Simple Service Discovery Protocol
//!
//! Ce module implémente le protocole SSDP (Simple Service Discovery Protocol) pour UPnP,
//! permettant la découverte automatique des devices sur le réseau.
//!
//! ## Fonctionnalités
//!
//! - ✅ Envoi de NOTIFY alive/byebye en multicast
//! - ✅ Réponse aux M-SEARCH en unicast
//! - ✅ Gestion multi-devices avec types de notification
//! - ✅ Annonces périodiques automatiques
//! - ✅ Arrêt propre avec byebye
//!
//! ## Architecture
//!
//! - [`SsdpServer`] : Serveur SSDP principal gérant les devices
//! - [`SsdpDevice`] : Représentation d'un device pour SSDP
//!
//! ## Constants SSDP
//!
//! - **Multicast Address**: 239.255.255.250:1900
//! - **Max-Age**: 1800 secondes (30 minutes)
//! - **Announcement Period**: 900 secondes (15 minutes, Max-Age/2)

mod device;
mod server;

pub use device::SsdpDevice;
pub use server::SsdpServer;

/// Adresse multicast SSDP
pub const SSDP_MULTICAST_ADDR: &str = "239.255.255.250";

/// Port SSDP
pub const SSDP_PORT: u16 = 1900;

/// Durée de validité des annonces (en secondes)
pub const MAX_AGE: u32 = 1800;
