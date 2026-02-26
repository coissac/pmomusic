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

pub mod config_ext;
pub mod logs;
pub mod server;

pub use config_ext::ConfigExt;
pub use logs::{
    LogState, LoggingOptions, LogsApiDoc, SseLayer, create_logs_router, init_logging, log_dump,
    log_setup_get, log_setup_post, log_sse,
};
pub use server::{ApiRegistry, ApiRegistryEntry, Server, ServerBuilder, ServerInfo};

// ============================================================================
// Singleton global du serveur
// ============================================================================

use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Instance globale unique du serveur PMOMusic
///
/// Ce singleton assure qu'une seule instance du serveur existe dans l'application.
/// Il est initialisé une seule fois via [`init_server()`] et accessible partout
/// via [`get_server()`].
///
/// # Exemple
///
/// ```ignore
/// use pmoserver::{init_server, get_server_base_url};
///
/// // Initialiser le serveur global (idempotent - peut être appelé plusieurs fois)
/// let server = init_server().await;
///
/// // Récupérer l'URL de base du serveur
/// if let Some(url) = get_server_base_url() {
///     println!("Server running at: {}", url);
/// }
/// ```
static GLOBAL_SERVER: OnceCell<Arc<RwLock<Server>>> = OnceCell::new();

/// Initialise le serveur global unique depuis la configuration
///
/// Cette fonction est **idempotente** : elle peut être appelée plusieurs fois
/// sans danger. Si le serveur est déjà initialisé, elle retourne simplement
/// la référence existante.
///
/// # Configuration
///
/// Le serveur est créé via [`ServerBuilder::new_configured()`] qui lit
/// la configuration depuis `pmoconfig`.
///
/// # Returns
///
/// Une référence Arc vers le serveur global, encapsulé dans un RwLock
/// pour permettre les accès concurrents mutables.
///
/// # Exemple
///
/// ```ignore
/// use pmoserver::init_server;
///
/// #[tokio::main]
/// async fn main() {
///     // Première initialisation
///     let server = init_server();
///
///     // Les appels suivants retournent la même instance
///     let same_server = init_server();
/// }
/// ```
pub fn init_server() -> Arc<RwLock<Server>> {
    GLOBAL_SERVER
        .get_or_init(|| {
            let server = ServerBuilder::new_configured().build();
            Arc::new(RwLock::new(server))
        })
        .clone()
}

/// Récupère le serveur global s'il a été initialisé
///
/// Retourne `None` si [`init_server()`] n'a pas encore été appelé.
///
/// # Returns
///
/// - `Some(Arc<RwLock<Server>>)` si le serveur est initialisé
/// - `None` si le serveur n'est pas encore initialisé
///
/// # Exemple
///
/// ```ignore
/// use pmoserver::get_server;
///
/// if let Some(server) = get_server() {
///     let srv = server.read().await;
///     println!("Server is running at: {}", srv.base_url());
/// } else {
///     println!("Server not initialized yet");
/// }
/// ```
pub fn get_server() -> Option<Arc<RwLock<Server>>> {
    GLOBAL_SERVER.get().cloned()
}

/// Récupère l'URL de base du serveur global
///
/// Fonction helper qui extrait directement l'URL de base sans avoir
/// à manipuler le RwLock manuellement.
///
/// # Returns
///
/// - `Some(String)` contenant l'URL complète (ex: "http://192.168.1.10:8080")
/// - `None` si le serveur n'est pas encore initialisé
///
/// # Exemple
///
/// ```ignore
/// use pmoserver::get_server_base_url;
///
/// if let Some(url) = get_server_base_url() {
///     let stream_url = format!("{}/api/stream", url);
///     println!("Stream available at: {}", stream_url);
/// }
/// ```
/// Récupère l'URL de base effective pour une requête donnée.
///
/// Utilise les headers `X-Forwarded-*` si présents (accès via reverse proxy),
/// sinon fallback sur l'URL de base configurée (accès local direct).
pub fn get_request_base_url(headers: &axum::http::HeaderMap) -> Option<String> {
    GLOBAL_SERVER.get().map(|server| {
        if let Ok(srv) = server.try_read() {
            srv.request_base_url(headers)
        } else {
            futures::executor::block_on(async { server.read().await.request_base_url(headers) })
        }
    })
}

pub fn get_server_base_url() -> Option<String> {
    GLOBAL_SERVER.get().map(|server| {
        // Utiliser try_read() pour éviter de bloquer
        // Si le lock est occupé, on retourne quand même l'URL
        // car elle ne change pas après l'initialisation
        if let Ok(srv) = server.try_read() {
            srv.base_url()
        } else {
            // Fallback: bloquer jusqu'à obtenir le lock
            // (ne devrait jamais arriver en pratique)
            futures::executor::block_on(async { server.read().await.base_url() })
        }
    })
}
