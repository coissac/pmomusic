//! Extension pour intégrer l'API de configuration de pmoconfig dans pmoserver
//!
//! Ce module fournit le trait `ConfigExt` qui permet d'ajouter facilement
//! l'API REST de configuration au serveur.

use crate::Server;
use anyhow::Result;
use pmoconfig::{api, get_config, ApiDoc};
use utoipa::OpenApi;

/// Trait d'extension pour ajouter l'API de configuration à pmoserver
pub trait ConfigExt {
    /// Initialise l'API de configuration et enregistre les routes HTTP
    ///
    /// # Routes enregistrées
    ///
    /// - `GET /api/config` - Récupérer toute la configuration
    /// - `GET /api/config/{path}` - Récupérer une valeur spécifique (ex: host.http_port)
    /// - `POST /api/config` - Mettre à jour une valeur
    /// - `GET /swagger-ui/config` - Documentation interactive Swagger
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoserver::{ServerBuilder, ConfigExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let mut server = ServerBuilder::new_configured().build();
    ///     server.init_config_api().await?;
    ///     server.start().await;
    ///     Ok(())
    /// }
    /// ```
    async fn init_config_api(&mut self) -> Result<()>;
}

impl ConfigExt for Server {
    async fn init_config_api(&mut self) -> Result<()> {
        let config = get_config();

        // API REST pour la configuration
        let api_router = api::create_router(config);
        let openapi = ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "config").await;

        Ok(())
    }
}
