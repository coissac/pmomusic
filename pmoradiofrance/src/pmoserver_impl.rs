//! Implémentation du trait RadioFranceExt pour pmoserver::Server
//!
//! Ce module enrichit `pmoserver::Server` avec les fonctionnalités du client Radio France
//! en implémentant le trait [`RadioFranceExt`](crate::RadioFranceExt).
//!
//! ## Architecture
//!
//! `pmoradiofrance` étend `pmoserver::Server` sans que `pmoserver` connaisse `pmoradiofrance`.
//! C'est le pattern d'extension : `pmoradiofrance` ajoute des fonctionnalités à un type
//! externe via un trait, similaire au pattern utilisé par `pmoqobuz` pour `QobuzServerExt`.
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmoradiofrance::RadioFranceExt;
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut server = ServerBuilder::new_configured().build();
//!
//! // Le trait RadioFranceExt est automatiquement disponible
//! let state = server.init_radiofrance().await?;
//!
//! server.start().await;
//! # Ok(())
//! # }
//! ```

use crate::api_rest::create_router;
use crate::pmoserver_ext::{RadioFranceExt, RadioFranceState};
use crate::stateful_client::RadioFranceStatefulClient;
use anyhow::Result;
use pmoserver::Server;
use std::sync::Arc;
use tracing::info;

impl RadioFranceExt for Server {
    async fn init_radiofrance(&mut self) -> Result<Arc<RadioFranceState>> {
        info!("Initializing Radio France API...");

        // Créer une source dédiée pour l'API (sans enregistrement UPnP)
        let config = pmoconfig::get_config();
        let source = crate::source::RadioFranceSource::new(config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Radio France source: {}", e))?;

        // Créer l'état partagé
        let state = RadioFranceState::new(Arc::new(source));

        // Créer et enregistrer le router
        let router = create_router(state.clone());
        self.add_router("/api/radiofrance", router).await;

        info!("Radio France API initialized");
        info!("API endpoints available at /api/radiofrance/*");

        Ok(Arc::new(state))
    }

    async fn init_radiofrance_with_source(
        &mut self,
        source: Arc<crate::source::RadioFranceSource>,
    ) -> Result<Arc<RadioFranceState>> {
        info!("Initializing Radio France API with existing source...");

        // Créer l'état partagé (simplement une référence à la source)
        let state = RadioFranceState::new(source);

        // Créer et enregistrer le router
        let router = create_router(state.clone());
        self.add_router("/api/radiofrance", router).await;

        info!("Radio France API initialized with source");
        info!("API endpoints available at /api/radiofrance/*");

        Ok(Arc::new(state))
    }
}
