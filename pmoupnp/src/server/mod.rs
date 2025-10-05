//! # Module Server - API de haut niveau pour Axum
//!
//! Ce module fournit une abstraction simple et ergonomique pour créer des serveurs HTTP
//! avec Axum, en cachant la complexité de la configuration et du routage.
//!
//! ## Fonctionnalités
//!
//! - 🚀 **Routes JSON simples** : Ajoutez des endpoints API avec `add_route()`
//! - 📁 **Fichiers statiques** : Servez des assets avec `add_dir()`
//! - ⚛️ **Applications SPA** : Support pour Vue.js/React avec `add_spa()`
//! - 🔀 **Redirections** : Redirigez des routes avec `add_redirect()`
//! - 🎯 **Handlers personnalisés** : Support SSE, WebSocket, etc. avec `add_handler_with_state()`
//! - 📚 **Documentation API** : OpenAPI/Swagger automatique avec `add_openapi()`
//! - ⚡ **Gestion gracieuse** : Arrêt propre sur Ctrl+C

pub mod logs;

use axum::handler::Handler;
use axum::response::Redirect;
use axum::routing::get;
use axum::{Json, Router};
use axum_embed::ServeEmbed;
use pmoconfig::get_config;
use rust_embed::RustEmbed;
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::{signal, sync::RwLock, task::JoinHandle};
use tracing::{info, warn, debug, error};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Info serveur sérialisable
#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ServerInfo {
    /// Nom du serveur
    pub name: String,
    /// URL de base
    pub base_url: String,
    /// Port HTTP
    pub http_port: u16,
}

/// Serveur principal
pub struct Server {
    name: String,
    base_url: String,
    http_port: u16,
    router: Arc<RwLock<Router>>,
    api_router: Arc<RwLock<Option<Router>>>,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(RustEmbed, Clone)]
#[folder = "webapp/dist"]
pub struct Webapp;

impl Server {
    /// Crée une nouvelle instance de serveur
    ///
    /// # Arguments
    ///
    /// * `name` - Nom du serveur (pour les logs)
    /// * `base_url` - URL de base (ex: "http://localhost:3000")
    /// * `http_port` - Port HTTP à écouter
    ///
    /// # Exemple
    ///
    /// ```rust
    /// # use pmoupnp::server::Server;
    /// let server = Server::new("MyAPI", "http://localhost:3000", 3000);
    /// ```
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, http_port: u16) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            http_port,
            router: Arc::new(RwLock::new(Router::new())),
            api_router: Arc::new(RwLock::new(None)),
            join_handle: None,
        }
    }

    pub fn new_configured() -> Self {
        let config = get_config();
        let url = config.get_base_url();
        let port = config.get_http_port();

        return Self::new("PMO-Music-Server", url, port);
    }

    /// Ajoute une route JSON dynamique
    ///
    /// Crée un endpoint qui retourne du JSON. La closure fournie sera appelée
    /// à chaque requête GET sur le chemin spécifié.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin de la route (ex: "/api/hello")
    /// * `f` - Closure async retournant une valeur sérialisable
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// # use pmoupnp::server::Server;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// server.add_route("/api/status", || async {
    ///     serde_json::json!({
    ///         "status": "online",
    ///         "version": "1.0.0"
    ///     })
    /// }).await;
    /// # }
    /// ```
    pub async fn add_route<F, Fut, T>(&mut self, path: &str, f: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        T: Serialize + Send + 'static,
    {
        let f = Arc::new(f);

        let handler = {
            let f = f.clone();
            move || {
                let f = f.clone();
                async move { Json(f().await) }
            }
        };

        let route = Router::new().route("/", get(handler));

        let mut r = self.router.write().await;
        *r = std::mem::take(&mut *r).nest(path, route);
    }

    /// Ajoute un répertoire de fichiers statiques
    ///
    /// Sert des fichiers embarqués via `RustEmbed`. Les fichiers sont compilés
    /// dans le binaire à la compilation.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin où monter les fichiers statiques
    ///
    /// # Type Parameter
    ///
    /// * `E` - Type RustEmbed définissant le répertoire à servir
    ///
    /// # Exemple
    ///
    /// ```ignore
    /// use pmoupnp::server::Server;
    /// use rust_embed::RustEmbed;
    /// 
    /// #[derive(RustEmbed, Clone)]
    /// #[folder = "static/"]
    /// struct Assets;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// server.add_dir::<Assets>("/assets").await;
    /// // Les fichiers de static/ sont accessibles via /assets/*
    /// # }
    /// ```
    pub async fn add_dir<E>(&mut self, path: &str)
    where
        E: RustEmbed + Clone + Send + Sync + 'static,
    {
        let serve = ServeEmbed::<E>::new();
        
        let mut r = self.router.write().await;
        
        if path == "/" {
            *r = std::mem::take(&mut *r).fallback_service(serve);
        } else {
            let route = Router::new().fallback_service(serve);
            *r = std::mem::take(&mut *r).nest(path, route);
        }
    }

    /// Ajoute une Single Page Application (SPA)
    ///
    /// Sert une application JavaScript moderne (Vue.js, React, etc.) avec support
    /// du routage côté client. Tous les chemins non trouvés renvoient `index.html`
    /// pour permettre au routeur JavaScript de gérer la navigation.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin où monter l'application (souvent "/" ou "/app")
    ///
    /// # Type Parameter
    ///
    /// * `E` - Type RustEmbed contenant les fichiers de la SPA
    ///
    /// # Exemple avec Vue.js
    ///
    /// ```rust,no_run
    /// # use pmoupnp::server::Server;
    /// # use rust_embed::RustEmbed;
    /// #[derive(RustEmbed, Clone)]
    /// #[folder = "webapp/dist"]  // Build output de Vue.js
    /// struct WebApp;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// server.add_spa::<WebApp>("/").await;
    /// // L'app Vue.js gère toutes les routes comme /about, /users, etc.
    /// # }
    /// ```
    ///
    /// # Note
    ///
    /// Pour Vue.js/Vite, configure le `base` dans `vite.config.js` si tu montes
    /// sur un sous-chemin :
    /// ```javascript
    /// export default {
    ///   base: '/app/'
    /// }
    /// ```
    pub async fn add_spa<E>(&mut self, path: &str)
    where
        E: RustEmbed + Clone + Send + Sync + 'static,
    {
        let serve = ServeEmbed::<E>::with_parameters(
            Some("index.html".to_string()),
            axum_embed::FallbackBehavior::Ok,
            Some("index.html".to_string()),
        );
        
        let mut r = self.router.write().await;
        
        if path == "/" {
            *r = std::mem::take(&mut *r).fallback_service(serve);
        } else {
            let route = Router::new().fallback_service(serve);
            *r = std::mem::take(&mut *r).nest(path, route);
        }
    }

    /// Ajoute un handler Axum personnalisé
    ///
    /// Pour des cas d'usage avancés nécessitant un contrôle complet sur le handler.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin de la route
    /// * `handler` - Handler Axum
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// # use pmoupnp::server::Server;
    /// # use axum::response::Html;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// async fn custom_handler() -> Html<&'static str> {
    ///     Html("<h1>Custom Response</h1>")
    /// }
    ///
    /// server.add_handler("/custom", custom_handler).await;
    /// # }
    /// ```
    pub async fn add_handler<H, T>(&mut self, path: &str, handler: H)
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        let route = Router::new().route("/", get(handler));

        let mut r = self.router.write().await;
        *r = std::mem::take(&mut *r).nest(path, route);
    }

    /// Ajoute un handler avec state (pour SSE, extracteurs, etc.)
    ///
    /// Permet d'utiliser des extracteurs Axum comme `State`, `Query`, etc.
    /// Idéal pour Server-Sent Events (SSE), WebSockets ou tout handler nécessitant un état partagé.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin de la route
    /// * `handler` - Handler Axum avec extracteurs
    /// * `state` - État partagé (doit être Clone + Send + Sync)
    ///
    /// # Exemple avec SSE
    ///
    /// ```ignore
    /// use pmoupnp::server::Server;
    /// use axum::extract::State;
    /// use axum::response::sse::{Event, Sse, KeepAlive};
    /// use tokio::sync::broadcast;
    /// 
    /// #[derive(Clone)]
    /// struct LogState { 
    ///     tx: broadcast::Sender<String> 
    /// }
    /// 
    /// impl LogState { 
    ///     fn subscribe(&self) -> broadcast::Receiver<String> { 
    ///         self.tx.subscribe() 
    ///     } 
    /// }
    /// 
    /// async fn log_sse(State(state): State<LogState>) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    ///     let mut rx = state.subscribe();
    ///     let stream = async_stream::stream! {
    ///         while let Ok(msg) = rx.recv().await {
    ///             yield Ok(Event::default().data(msg));
    ///         }
    ///     };
    ///     Sse::new(stream).keep_alive(KeepAlive::default())
    /// }
    ///
    /// let log_state = LogState { tx: broadcast::channel(100).0 };
    /// server.add_handler_with_state("/logs", log_sse, log_state).await;
    /// ```
    pub async fn add_handler_with_state<H, T, S>(&mut self, path: &str, handler: H, state: S)
    where
        H: Handler<T, S>,
        T: 'static,
        S: Clone + Send + Sync + 'static,
    {
        let route = Router::new()
            .route("/", get(handler))
            .with_state(state);

        let mut r = self.router.write().await;
        *r = std::mem::take(&mut *r).nest(path, route);
    }

    /// Ajoute un handler POST avec state
    ///
    /// Similaire à `add_handler_with_state` mais pour les requêtes POST.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin de la route
    /// * `handler` - Handler Axum pour POST
    /// * `state` - État partagé
    pub async fn add_post_handler_with_state<H, T, S>(&mut self, path: &str, handler: H, state: S)
    where
        H: Handler<T, S>,
        T: 'static,
        S: Clone + Send + Sync + 'static,
    {
        let route = Router::new()
            .route("/", axum::routing::post(handler))
            .with_state(state);

        let mut r = self.router.write().await;
        *r = std::mem::take(&mut *r).nest(path, route);
    }

    /// Ajoute une redirection HTTP
    ///
    /// Redirige automatiquement les requêtes d'un chemin vers un autre avec un code 308 (permanent).
    ///
    /// # Arguments
    ///
    /// * `from` - Chemin source (peut être "/" pour la racine)
    /// * `to` - Chemin de destination
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// # use pmoupnp::server::Server;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// // Rediriger la racine vers /app
    /// server.add_redirect("/", "/app").await;
    /// # }
    /// ```
    pub async fn add_redirect(&mut self, from: &str, to: &str) {
        let to = to.to_string();
        let handler = move || {
            let to = to.clone();
            async move { Redirect::permanent(&to) }
        };

        let mut r = self.router.write().await;

        if from == "/" {
            // Pour la racine, utiliser merge au lieu de nest
            let route = Router::new().route("/", get(handler));
            *r = std::mem::take(&mut *r).merge(route);
        } else {
            let route = Router::new().route("/", get(handler));
            *r = std::mem::take(&mut *r).nest(from, route);
        }
    }

    /// Ajoute une API documentée avec OpenAPI
    ///
    /// Monte un routeur d'API sous `/api` et active Swagger UI sur `/swagger-ui`
    ///
    /// # Arguments
    ///
    /// * `api_router` - Router Axum contenant les routes API
    /// * `openapi` - Spécification OpenAPI générée par utoipa
    ///
    /// # Exemple
    ///
    /// ```ignore
    /// use utoipa::OpenApi;
    /// use axum::{Router, Json, routing::get};
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Serialize, Deserialize, utoipa::ToSchema)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// #[derive(utoipa::OpenApi)]
    /// #[openapi(
    ///     paths(get_users),
    ///     components(schemas(User))
    /// )]
    /// struct ApiDoc;
    ///
    /// #[utoipa::path(
    ///     get,
    ///     path = "/users",
    ///     responses((status = 200, description = "List users"))
    /// )]
    /// async fn get_users() -> Json<Vec<User>> {
    ///     Json(vec![])
    /// }
    ///
    /// let api_router = Router::new()
    ///     .route("/users", get(get_users));
    ///
    /// server.add_openapi(api_router, ApiDoc::openapi()).await;
    /// ```
    pub async fn add_openapi(&mut self, api_router: Router, openapi: utoipa::openapi::OpenApi) {
        // Stocker le routeur API
        let mut api_r = self.api_router.write().await;
        *api_r = Some(api_router);

        // Ajouter Swagger UI
        let swagger = SwaggerUi::new("/swagger-ui")
            .url("/api-docs/openapi.json", openapi);

        let mut r = self.router.write().await;
        *r = std::mem::take(&mut *r).merge(swagger);
    }

    /// Démarre le serveur HTTP
    ///
    /// Lance le serveur sur le port configuré et met en place la gestion
    /// de Ctrl+C pour un arrêt gracieux.
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// # use pmoupnp::server::Server;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// server.start().await;
    /// server.wait().await;  // Attend Ctrl+C
    /// # }
    /// ```
    pub async fn start(&mut self) {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.http_port));
        info!("Server {} running at [http://{}:{}](http://{}:{})", self.name, self.base_url, self.http_port, self.base_url, self.http_port);

        // Merger le routeur API si présent
        let api_router = self.api_router.read().await;
        if let Some(api_r) = api_router.as_ref() {
            let mut r = self.router.write().await;
            *r = std::mem::take(&mut *r).nest("/api", api_r.clone());
        }
        drop(api_router);

        let router = self.router.clone();

        let server_task = tokio::spawn(async move {
            let r = router.read().await.clone();
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            axum::serve(listener, r.into_make_service()).await.unwrap();
        });

        let shutdown_task = tokio::spawn(async move {
            signal::ctrl_c().await.expect("failed to listen for ctrl_c");
            info!("Ctrl+C reçu, arrêt gracieux");
        });

        self.join_handle = Some(tokio::spawn(async move {
            tokio::select! {
                _ = server_task => {},
                _ = shutdown_task => {},
            }
        }));
    }

    /// Attend la fin du serveur
    pub async fn wait(&mut self) {
        if let Some(h) = self.join_handle.take() {
            let _ = h.await;
        }
    }

    /// Récupère les infos du serveur
    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            http_port: self.http_port,
        }
    }
}

/// Builder pattern
pub struct ServerBuilder {
    name: String,
    base_url: String,
    http_port: u16,
}

impl ServerBuilder {
    /// Crée un nouveau builder
    ///
    /// # Arguments
    ///
    /// * `name` - Nom du serveur
    /// * `base_url` - URL de base (ex: "http://localhost:3000")
    /// * `http_port` - Port HTTP
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, http_port: u16) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            http_port,
        }
    }

    pub fn new_configured() -> Self {
        let config = get_config();
        Self {
            name: "PMO-Music-Server".to_string(),
            base_url: config.get_base_url(),
            http_port: config.get_http_port()
        }
    }

    /// Construit le serveur
    ///
    /// Consomme le builder et retourne une instance de `Server` prête à l'emploi.
    ///
    /// # Exemple
    ///
    /// ```rust
    /// # use pmoupnp::server::ServerBuilder;
    /// let mut server = ServerBuilder::new("MyAPI", "http://localhost:3000", 3000)
    ///     .build();
    /// ```
    pub fn build(self) -> Server {
        Server::new(self.name, self.base_url, self.http_port)
    }
}