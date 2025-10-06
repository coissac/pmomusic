//! # pmoserver - Serveur web haut niveau basé sur Axum
//!
//! Cette crate fournit une abstraction simple et ergonomique pour créer des serveurs HTTP
//! avec Axum, spécialement conçue pour les applications UPnP et les serveurs multimédia.
//!
//! ## Fonctionnalités
//!
//! - 🚀 **API de haut niveau** : Interface simple pour créer des serveurs HTTP avec Axum
//! - 🎯 **Support UPnP** : Implémentation du trait `UpnpServer` pour connecter des devices UPnP
//! - 📡 **Server-Sent Events (SSE)** : Support intégré pour les logs en temps réel via SSE
//! - ⚛️ **Applications SPA** : Support pour servir des applications Single Page (Vue.js, React, etc.)
//! - 📁 **Fichiers statiques** : Serve de fichiers statiques avec `RustEmbed`
//! - 🔀 **Redirections** : Support pour les redirections HTTP
//! - 📚 **Documentation OpenAPI** : Génération automatique de Swagger UI
//! - ⚡ **Arrêt gracieux** : Gestion propre de l'arrêt sur Ctrl+C
//!
//! ## Architecture
//!
//! La crate est organisée en plusieurs modules :
//!
//! - [`server`] : Implémentation du serveur principal et du builder
//! - [`logs`] : Système de logs SSE pour monitoring en temps réel
//! - `upnp_impl` : Implémentation du trait `pmoupnp::UpnpServer` (privé)
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmoserver::{ServerBuilder, logs::{LogState, SseLayer}};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Configuration des logs avec SSE
//!     let log_state = LogState::new();
//!     tracing_subscriber::registry()
//!         .with(SseLayer::new(log_state.clone()))
//!         .init();
//!
//!     // Création et démarrage du serveur
//!     let mut server = ServerBuilder::new("MyServer")
//!         .http_port(8080)
//!         .build();
//!
//!     // Ajout d'une route JSON
//!     server.add_route("/api/status", || async {
//!         serde_json::json!({"status": "ok"})
//!     }).await;
//!
//!     // Démarrage
//!     server.start().await;
//! }
//! ```
//!
//! ## Intégration UPnP
//!
//! Le serveur implémente automatiquement le trait `pmoupnp::UpnpServer`, permettant
//! de connecter des devices UPnP :
//!
//! ```rust,no_run
//! use pmoupnp::{UpnpServer, mediarenderer::device::MEDIA_RENDERER};
//! use pmoupnp::devices::DeviceInstance;
//! use pmoserver::ServerBuilder;
//! use std::sync::Arc;
//!
//! # async fn example() {
//! let mut server = ServerBuilder::new("MediaRenderer").build();
//! let device = Arc::new(DeviceInstance::new(&MEDIA_RENDERER));
//!
//! // Le device enregistre automatiquement ses routes
//! device.register_urls(&mut server).await;
//! # }
//! ```

pub mod server;
pub mod logs;
mod upnp_impl;

pub use server::{Server, ServerBuilder, ServerInfo, Webapp};
pub use logs::{LogState, SseLayer, log_sse, log_dump};
