//! # pmoapp - Application web UPnP pour PMOMusic
//!
//! Cette crate fournit l'application web frontend pour le contrôle et la visualisation
//! des devices UPnP MediaRenderer, intégrée via RustEmbed pour être servie par pmoserver.
//!
//! ## Vue d'ensemble
//!
//! `pmoapp` est une application Vue.js 3 moderne avec TypeScript qui offre une interface
//! utilisateur pour :
//! - Visualiser les logs système en temps réel (Server-Sent Events)
//! - Contrôler les devices UPnP MediaRenderer
//! - Afficher et formater automatiquement le XML dans les logs
//!
//! ## Fonctionnalités
//!
//! ### 📦 Frontend intégré
//! - Application web compilée et embarquée dans le binaire Rust
//! - Aucun fichier statique externe à gérer en production
//! - Intégration via `RustEmbed` pour une distribution simplifiée
//!
//! ### 🎨 Interface utilisateur
//! - **LogView** : Visualisation des logs en temps réel avec filtres par niveau
//! - **Auto-scroll** : Défilement automatique des nouveaux logs (désactivable)
//! - **Formatage XML** : Détection et coloration syntaxique automatique du XML
//! - **Design responsive** : Compatible desktop et mobile
//! - **Thème sombre** : Style inspiré de VS Code pour une meilleure lisibilité
//!
//! ### 🚀 Zero configuration
//! - Pas besoin de serveur web séparé pour les assets
//! - Les fichiers sont servis directement depuis la mémoire du binaire
//! - Configuration automatique du routing Vue Router
//!
//! ## Architecture
//!
//! ### Stack technique
//!
//! - **Frontend** : Vue.js 3 avec Composition API
//! - **Langage** : TypeScript
//! - **Build** : Vite (rapide, moderne, HMR)
//! - **Routing** : Vue Router
//! - **Markdown** : Marked.js pour le rendu
//! - **Sécurité** : DOMPurify pour la sanitization HTML
//!
//! ### Structure des fichiers
//!
//! ```text
//! pmoapp/
//! ├── Cargo.toml              # Dépendances Rust (rust-embed)
//! ├── src/
//! │   └── lib.rs              # Point d'entrée Rust (ce fichier)
//! └── webapp/
//!     ├── src/
//!     │   ├── main.ts         # Point d'entrée Vue.js
//!     │   ├── App.vue         # Composant racine
//!     │   ├── router/         # Configuration Vue Router
//!     │   └── components/
//!     │       ├── LogView.vue # Visualiseur de logs SSE
//!     │       └── ...
//!     ├── dist/               # Build output (généré, non versionné)
//!     ├── package.json        # Dépendances npm
//!     └── vite.config.ts      # Configuration Vite
//! ```
//!
//! ## Workflow de build
//!
//! ### 1. Build de la webapp (Vue.js)
//!
//! ```bash
//! # Installation des dépendances
//! cd pmoapp/webapp
//! npm install
//!
//! # Build de production
//! npm run build
//! # Génère : webapp/dist/index.html, assets/*.js, assets/*.css
//! ```
//!
//! ### 2. Compilation Rust
//!
//! ```bash
//! cargo build
//! # RustEmbed inclut automatiquement les fichiers de webapp/dist/
//! ```
//!
//! ### 3. Utilisation avec Makefile
//!
//! ```bash
//! # Build complet (webapp + Rust)
//! make build
//!
//! # Ou juste la webapp
//! make webapp
//!
//! # Clean
//! make clean
//! ```
//!
//! ## Utilisation
//!
//! ### Exemple basique
//!
//! ```rust,no_run
//! use pmoapp::Webapp;
//! use pmoserver::ServerBuilder;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut server = ServerBuilder::new("MyApp")
//!         .http_port(8080)
//!         .build();
//!
//!     // Ajouter la webapp comme Single Page Application
//!     server.add_spa::<Webapp>("/app").await;
//!
//!     // Ajouter une redirection de la racine vers /app
//!     server.add_redirect("/", "/app").await;
//!
//!     server.start().await;
//!     server.wait().await;
//! }
//! ```
//!
//! ### Exemple avec logs SSE
//!
//! ```rust,no_run
//! use pmoapp::Webapp;
//! use pmoserver::{ServerBuilder, logs::{LogState, SseLayer}};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Configuration des logs avec SSE
//!     let log_state = LogState::new(1000); // Buffer de 1000 logs
//!     tracing_subscriber::registry()
//!         .with(tracing_subscriber::fmt::layer())
//!         .with(SseLayer::new(log_state.clone()))
//!         .init();
//!
//!     let mut server = ServerBuilder::new("MyApp").build();
//!
//!     // Endpoints SSE pour les logs
//!     server.add_handler_with_state("/log-sse", pmoserver::logs::log_sse, log_state.clone()).await;
//!     server.add_handler_with_state("/log-dump", pmoserver::logs::log_dump, log_state).await;
//!
//!     // Webapp (consommera les logs via /log-sse)
//!     server.add_spa::<Webapp>("/app").await;
//!     server.add_redirect("/", "/app").await;
//!
//!     server.start().await;
//!     server.wait().await;
//! }
//! ```
//!
//! ## Développement
//!
//! ### Mode développement Vue.js
//!
//! Pour développer la webapp avec Hot Module Replacement :
//!
//! ```bash
//! cd pmoapp/webapp
//! npm run dev
//! # Serveur de dev sur http://localhost:5173
//! ```
//!
//! ### Rebuild après modifications
//!
//! Après avoir modifié le code Vue.js :
//!
//! ```bash
//! # Rebuild webapp + recompile Rust
//! make build
//!
//! # Ou séparément
//! make webapp        # Build Vue.js seulement
//! cargo build        # Recompile Rust (intègre le nouveau dist/)
//! ```
//!
//! ## Composants Vue.js
//!
//! ### LogView
//!
//! Composant principal pour la visualisation des logs :
//!
//! - **Connexion SSE** : Stream temps réel via EventSource
//! - **Filtrage** : Par niveau (TRACE, DEBUG, INFO, WARN, ERROR)
//! - **Auto-scroll** : Activable/désactivable
//! - **Formatage** : Markdown + détection XML automatique
//! - **Buffer** : Limite à 1000 logs en mémoire
//! - **Déduplication** : Évite les logs en double
//!
//! ### Formatage XML
//!
//! Le composant LogView détecte automatiquement le XML dans les messages :
//!
//! ```
//! Input:  "INFO: <?xml version=\"1.0\"?><scpd>...</scpd>"
//! Output: Bloc de code avec coloration syntaxique XML
//! ```
//!
//! - Détection via regex : `<?xml` ou balises courantes (`<scpd>`, `<service>`, etc.)
//! - Conversion en bloc markdown : ` ```xml ... ``` `
//! - Rendu avec coloration et scrollbar pour le XML long
//!
//! ## Intégration avec pmoupnp
//!
//! La webapp communique avec les devices UPnP via les endpoints HTTP fournis par
//! `pmoserver` et `pmoupnp` :
//!
//! - `/log-sse` : Stream de logs (Server-Sent Events)
//! - `/log-dump` : Historique des logs
//! - `/device/*/description.xml` : Descripteurs UPnP
//! - `/service/*/control` : Endpoints de contrôle SOAP
//! - `/service/*/event` : Souscription aux événements UPnP
//!
//! ## Notes de déploiement
//!
//! ### Taille du binaire
//!
//! La webapp ajoutera ~150KB au binaire (compressé avec gzip par RustEmbed).
//!
//! ### Cache du navigateur
//!
//! Les assets sont servis avec des hashes dans les noms de fichiers
//! (`index-BBZcSinC.js`) pour un cache busting automatique.
//!
//! ### Compatibilité navigateurs
//!
//! - Chrome/Edge : ✅ Moderne
//! - Firefox : ✅ Moderne
//! - Safari : ✅ iOS 13+
//! - IE11 : ❌ Non supporté (utilise ES modules)
//!
//! ## Voir aussi
//!
//! - [`pmoserver`] : Serveur HTTP Axum pour servir la webapp
//! - [`pmoupnp`] : Bibliothèque UPnP MediaRenderer
//! - [Vue.js Documentation](https://vuejs.org/)
//! - [Vite Documentation](https://vitejs.dev/)

use rust_embed::RustEmbed;
use std::future::Future;
use std::pin::Pin;

/// Structure représentant l'application web embarquée.
///
/// Cette structure utilise `RustEmbed` pour inclure tous les fichiers
/// du répertoire `webapp/dist` dans le binaire au moment de la compilation.
///
/// ## Exemple
///
/// ```rust,no_run
/// use pmoapp::{Webapp, WebAppExt};
/// use pmoserver::ServerBuilder;
///
/// # async fn example() {
/// let mut server = ServerBuilder::new("MyApp").build();
///
/// // Ajouter la webapp via le trait WebAppExt
/// server.add_webapp::<Webapp>("/app").await;
/// # }
/// ```
#[derive(RustEmbed, Clone)]
#[folder = "webapp/dist"]
pub struct Webapp;

/// Trait pour étendre un serveur HTTP avec des fonctionnalités webapp.
///
/// Ce trait permet à `pmoapp` d'ajouter des méthodes d'extension sur des types
/// de serveurs externes (comme `pmoserver::Server`) sans que ces crates dépendent de `pmoapp`.
///
/// # Architecture
///
/// Similaire au pattern utilisé par `pmoupnp` pour `UpnpServer`, ce trait permet
/// une extension propre et découplée :
///
/// - `pmoserver` définit un serveur HTTP générique
/// - `pmoapp` étend ce serveur avec des méthodes webapp via ce trait
/// - Le serveur n'a pas besoin de connaître `pmoapp`
///
/// # Exemple d'implémentation
///
/// ```ignore
/// impl WebAppExt for pmoserver::Server {
///     fn add_webapp<W: RustEmbed>(&mut self, path: &str) -> ... {
///         // Délègue à la méthode interne add_spa
///         self.add_spa::<W>(path)
///     }
/// }
/// ```
pub trait WebAppExt {
    /// Ajoute une Single Page Application au serveur.
    ///
    /// # Arguments
    ///
    /// * `path` - Le chemin où monter la webapp (ex: "/app")
    ///
    /// # Type Parameter
    ///
    /// * `W` - Type RustEmbed contenant les fichiers de la webapp
    fn add_webapp<W>(&mut self, path: &str) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        W: RustEmbed + Clone + Send + Sync + 'static;

    /// Ajoute une webapp avec une redirection automatique depuis la racine.
    ///
    /// # Arguments
    ///
    /// * `path` - Le chemin où monter la webapp (ex: "/app")
    ///
    /// # Type Parameter
    ///
    /// * `W` - Type RustEmbed contenant les fichiers de la webapp
    fn add_webapp_with_redirect<W>(&mut self, path: &str) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        W: RustEmbed + Clone + Send + Sync + 'static;
}

// Implémentation du trait pour pmoserver::Server (feature-gated)
#[cfg(feature = "pmoserver")]
mod pmoserver_impl;
