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
//!
//! ## Exemple d'utilisation
//!
//! ```rust,ignore
//! use pmoserver::{ServerBuilder, logs::{LogState, SseLayer}};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Configuration des logs avec SSE
//!     let log_state = LogState::new(1000);
//!     tracing_subscriber::registry()
//!         .with(SseLayer::new(log_state.clone()))
//!         .init();
//!
//!     // Création et démarrage du serveur
//!     let mut server = ServerBuilder::new("MyServer", "http://localhost", 8080)
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
//! Le serveur peut être étendu avec UPnP via le trait `pmoupnp::UpnpServer`.
//! L'implémentation est fournie par `pmoupnp` (feature `pmoserver`), permettant
//! de connecter des devices UPnP sans que `pmoserver` dépende de `pmoupnp` :
//!
//! ```rust,ignore
//! use pmoupnp::{UpnpServer, mediarenderer::MEDIA_RENDERER};
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() {
//! let mut server = ServerBuilder::new("MediaRenderer", "http://localhost", 8080).build();
//! let device = MEDIA_RENDERER.create_instance();
//!
//! // Le trait UpnpServer est automatiquement disponible (implémenté dans pmoupnp)
//! device.register_urls(&mut server).await;
//! # }
//! ```

pub mod server;
pub mod logs;

pub use server::{Server, ServerBuilder, ServerInfo};
pub use logs::{LogState, SseLayer, log_sse, log_dump, init_logging, LoggingOptions, log_setup_get, log_setup_post};
