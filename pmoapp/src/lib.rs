//! # pmoapp - Application web UPnP pour PMOMusic
//!
//! Cette crate fournit l'application web frontend pour le contrôle UPnP,
//! intégrée via RustEmbed pour être servie par pmoserver.
//!
//! ## Fonctionnalités
//!
//! - 📦 **Frontend intégré** : Application web compilée et embarquée dans le binaire
//! - 🎨 **Interface de contrôle** : UI pour gérer les devices UPnP MediaRenderer
//! - 🚀 **Zero configuration** : Pas besoin de servir des fichiers statiques séparés
//!
//! ## Utilisation
//!
//! ```rust,no_run
//! use pmoapp::Webapp;
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() {
//! let mut server = ServerBuilder::new("MyApp").build();
//! server.add_spa::<Webapp>("/app").await;
//! # }
//! ```
//!
//! ## Structure
//!
//! La webapp est construite avec Vite et Vue.js, et les fichiers statiques
//! sont embarqués dans le binaire au moment de la compilation via `RustEmbed`.

use rust_embed::RustEmbed;

/// Structure représentant l'application web embarquée.
///
/// Cette structure utilise `RustEmbed` pour inclure tous les fichiers
/// du répertoire `webapp/dist` dans le binaire au moment de la compilation.
///
/// ## Exemple
///
/// ```rust,no_run
/// use pmoapp::Webapp;
/// use pmoserver::ServerBuilder;
///
/// # async fn example() {
/// let mut server = ServerBuilder::new("MyApp").build();
///
/// // Ajouter la webapp comme SPA sur le chemin /app
/// server.add_spa::<Webapp>("/app").await;
/// # }
/// ```
#[derive(RustEmbed, Clone)]
#[folder = "webapp/dist"]
pub struct Webapp;
