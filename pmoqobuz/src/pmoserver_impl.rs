//! Implémentation du trait QobuzServerExt pour pmoserver::Server
//!
//! Ce module enrichit `pmoserver::Server` avec les fonctionnalités du client Qobuz en
//! implémentant le trait [`QobuzServerExt`](crate::QobuzServerExt). Cette implémentation
//! permet d'initialiser facilement le client Qobuz et d'enregistrer les routes HTTP.
//!
//! ## Architecture
//!
//! `pmoqobuz` étend `pmoserver::Server` sans que `pmoserver` connaisse `pmoqobuz`.
//! C'est le pattern d'extension : `pmoqobuz` ajoute des fonctionnalités à un type
//! externe via un trait, similaire au pattern utilisé par `pmocovers` pour `CoverCacheExt`.
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmoqobuz::QobuzServerExt;
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut server = ServerBuilder::new_configured().build();
//!
//! // Le trait QobuzServerExt est automatiquement disponible
//! let client = server.init_qobuz_client_configured().await?;
//!
//! server.start().await;
//! # Ok(())
//! # }
//! ```

use crate::api_rest::{create_router, QobuzState};
use crate::client::QobuzClient;
use crate::pmoserver_ext::QobuzServerExt;
use anyhow::Result;
use pmoconfig::Config;
use pmoserver::Server;
use std::sync::Arc;
use tracing::info;

impl QobuzServerExt for Server {
    async fn init_qobuz_client(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<Arc<QobuzClient>> {
        info!("Initializing Qobuz client for user: {}", username);

        // Créer le client Qobuz
        let client = QobuzClient::new(username, password).await?;
        let client = Arc::new(client);

        // Créer l'état de l'API sans cache d'images
        let state = QobuzState {
            client: client.clone(),
            cover_cache: None,
        };

        // Créer le router et l'enregistrer
        let router = create_router(state);
        self.add_router("/qobuz", router).await;

        info!("Qobuz client initialized successfully");
        info!("API endpoints available at /qobuz/*");

        Ok(client)
    }

    async fn init_qobuz_client_configured(&mut self) -> Result<Arc<QobuzClient>> {
        info!("Initializing Qobuz client from configuration");

        // Récupérer les credentials depuis la config
        let config = pmoconfig::get_config();
        let (username, password) = config.get_qobuz_credentials()?;

        self.init_qobuz_client(&username, &password).await
    }

    async fn init_qobuz_client_with_covers(
        &mut self,
        username: &str,
        password: &str,
        cover_cache: Arc<pmocovers::Cache>,
    ) -> Result<Arc<QobuzClient>> {
        info!("Initializing Qobuz client with pmocovers integration");

        // Créer le client Qobuz
        let client = QobuzClient::new(username, password).await?;
        let client = Arc::new(client);

        info!("pmocovers integration enabled - album images will be cached automatically");

        // Créer l'état de l'API avec le cache
        let state = QobuzState {
            client: client.clone(),
            cover_cache: Some(cover_cache),
        };

        // Créer le router et l'enregistrer
        let router = create_router(state);
        self.add_router("/qobuz", router).await;

        info!("Qobuz client initialized successfully with covers");
        info!("API endpoints available at /qobuz/*");

        Ok(client)
    }

    async fn init_qobuz_client_configured_with_covers(
        &mut self,
        cover_cache: Arc<pmocovers::Cache>,
    ) -> Result<Arc<QobuzClient>> {
        info!("Initializing Qobuz client from configuration with pmocovers");

        // Récupérer les credentials depuis la config
        let config = pmoconfig::get_config();
        let (username, password) = config.get_qobuz_credentials()?;

        self.init_qobuz_client_with_covers(&username, &password, cover_cache)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_implemented() {
        // Ce test vérifie simplement que le trait est bien implémenté
        // Les tests fonctionnels nécessiteraient un serveur et des credentials réels
    }
}
