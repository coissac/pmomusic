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
//! ```rust,ignore
//! use pmoapp::{Webapp, WebAppExt};
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() {
//! let mut server = ServerBuilder::new("MyApp", "http://localhost", 8080).build();
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

impl WebAppExt for Server {
    async fn add_webapp<W>(&mut self, path: &str)
    where
        W: RustEmbed + Clone + Send + Sync + 'static,
    {
        let mount_path = normalize_mount_path(path);
        mount_spa_with_trailing_slash_redirect::<W>(self, &mount_path).await;
    }

    async fn add_webapp_with_redirect<W>(&mut self, path: &str)
    where
        W: RustEmbed + Clone + Send + Sync + 'static,
    {
        let mount_path = normalize_mount_path(path);

        mount_spa_with_trailing_slash_redirect::<W>(self, &mount_path).await;
        self.add_redirect("/", &mount_path).await;
    }
}

/// S'assure que les chemins SPA sont cohérents : `"/app"` devient `"/app"`,
/// tandis que `"/"` reste tel quel. Les espaces ou slashs multiples sont
/// nettoyés pour éviter des routes dupliquées.
fn normalize_mount_path(path: &str) -> String {
    let trimmed = path.trim();

    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

/// Monte la SPA et ajoute automatiquement une redirection `"/app/" -> "/app"`
/// afin que les URLs avec slash final servent également l'application.
async fn mount_spa_with_trailing_slash_redirect<W>(server: &mut Server, path: &str)
where
    W: RustEmbed + Clone + Send + Sync + 'static,
{
    server.add_spa::<W>(path).await;

    if path != "/" {
        let trailing = format!("{}/", path.trim_end_matches('/'));
        server.add_redirect(&trailing, path).await;
    }
}
