//! Extension pmoserver pour Radio France
//!
//! Ce module fournit un trait d'extension pour ajouter l'API Radio France
//! à un serveur pmoserver.

use anyhow::Result;
use std::sync::Arc;

/// État partagé pour les handlers Radio France
#[derive(Clone)]
pub struct RadioFranceState {
    pub source: Arc<crate::source::RadioFranceSource>,
}

impl RadioFranceState {
    pub fn new(source: Arc<crate::source::RadioFranceSource>) -> Self {
        Self { source }
    }
}

/// Trait pour étendre pmoserver avec les fonctionnalités Radio France
///
/// Ce trait permet à `pmoradiofrance` d'ajouter des méthodes d'extension sur
/// `pmoserver::Server` sans que pmoserver dépende de pmoradiofrance.
///
/// # Architecture
///
/// Similaire au pattern utilisé par `pmoqobuz` avec `QobuzServerExt`, ce trait permet
/// une extension propre et découplée :
///
/// - `pmoserver` définit un serveur HTTP générique
/// - `pmoradiofrance` étend ce serveur avec les fonctionnalités Radio France via ce trait
/// - Le serveur n'a pas besoin de connaître `pmoradiofrance`
///
/// # Exemple
///
/// ```rust,no_run
/// use pmoradiofrance::RadioFranceExt;
/// use pmoserver::ServerBuilder;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let mut server = ServerBuilder::new_configured().build();
///
///     // Initialise le client Radio France
///     server.init_radiofrance().await?;
///
///     server.start().await;
///     server.wait().await;
///     Ok(())
/// }
/// ```
pub trait RadioFranceExt {
    /// Initialise l'extension Radio France et enregistre les routes HTTP
    ///
    /// Cette méthode :
    /// - Crée un client stateful Radio France
    /// - Configure les routes API pour les stations et métadonnées
    /// - Configure le proxy streaming pour les flux AAC
    ///
    /// # Returns
    /// État partagé de Radio France
    ///
    /// # Routes enregistrées
    ///
    /// - `GET /api/radiofrance/stations` - Liste groupée des stations
    /// - `GET /api/radiofrance/:slug/metadata` - Métadonnées live d'une station
    /// - `GET /api/radiofrance/:slug/stream` - Proxy du flux AAC
    ///
    /// # Exemple
    /// ```ignore
    /// use pmoserver::ServerBuilder;
    /// use pmoradiofrance::RadioFranceExt;
    ///
    /// let mut server = ServerBuilder::new_configured().build();
    /// server.init_radiofrance().await?;
    /// ```
    async fn init_radiofrance(&mut self) -> Result<Arc<RadioFranceState>>;

    /// Initialise l'extension Radio France avec une source existante
    ///
    /// Cette méthode est similaire à `init_radiofrance()` mais utilise une source
    /// déjà créée et enregistrée, permettant de partager la même instance entre
    /// le MediaServer UPnP et les routes API REST.
    async fn init_radiofrance_with_source(
        &mut self,
        source: Arc<crate::source::RadioFranceSource>,
    ) -> Result<Arc<RadioFranceState>>;
}

// L'implémentation du trait sera dans un module séparé (pmoserver_impl.rs)
// pour éviter les dépendances circulaires
