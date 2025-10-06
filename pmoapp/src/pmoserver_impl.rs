//! Implémentation du trait WebAppExt pour le serveur pmoserver
//!
//! Ce module enrichit `pmoserver::Server` avec les fonctionnalités webapp en
//! implémentant le trait [`WebAppExt`](crate::WebAppExt). Cette implémentation
//! permet d'enregistrer facilement des webapps embarquées sur le serveur.
//!
//! ## Architecture
//!
//! `pmoapp` étend `pmoserver::Server` sans que `pmoserver` connaisse `pmoapp`.
//! C'est le pattern d'extension : `pmoapp` ajoute des fonctionnalités à un type
//! externe via un trait, similaire au pattern utilisé par `pmoupnp` pour `UpnpServer`.
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmoapp::{Webapp, WebAppExt};
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() {
//! let mut server = ServerBuilder::new("MyApp").build();
//!
//! // Le trait WebAppExt est automatiquement disponible
//! server.add_webapp::<Webapp>("/app").await;
//!
//! // Ou avec redirection
//! server.add_webapp_with_redirect::<Webapp>("/app").await;
//! # }
//! ```

use crate::WebAppExt;
use pmoserver::Server;
use rust_embed::RustEmbed;
use std::future::Future;
use std::pin::Pin;

impl WebAppExt for Server {
    fn add_webapp<W>(&mut self, path: &str) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        W: RustEmbed + Clone + Send + Sync + 'static,
    {
        let path = path.to_string();
        Box::pin(async move {
            self.add_spa::<W>(&path).await;
        })
    }

    fn add_webapp_with_redirect<W>(&mut self, path: &str) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        W: RustEmbed + Clone + Send + Sync + 'static,
    {
        let path = path.to_string();
        Box::pin(async move {
            self.add_spa::<W>(&path).await;
            self.add_redirect("/", &path).await;
        })
    }
}
