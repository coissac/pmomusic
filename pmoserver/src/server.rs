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

use crate::logs::{LogState, init_logging, log_dump, log_sse};
use axum::extract::State;
use axum::handler::Handler;
use axum::response::Redirect;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use axum_embed::ServeEmbed;
use pmoconfig::get_config;
use rust_embed::RustEmbed;
use serde::Serialize;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{signal, sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Info serveur sérialisable
#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ServerInfo {
    pub name: String,
    pub base_url: String,
    pub http_port: u16,
}

/// Entrée du registre d'API
#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ApiRegistryEntry {
    /// Nom de l'API
    pub name: String,
    /// Chemin de base de l'API
    pub path: String,
    /// Chemin vers Swagger UI
    pub swagger_ui_path: String,
    /// Chemin vers le JSON OpenAPI
    pub openapi_json_path: String,
    /// Nombre d'endpoints
    pub endpoint_count: usize,
    /// Version de l'API
    pub version: String,
    /// Description de l'API
    pub description: Option<String>,
    /// Titre de l'API
    pub title: String,
}

/// Liste des APIs enregistrées
#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ApiRegistry {
    /// Liste des APIs disponibles
    pub apis: Vec<ApiRegistryEntry>,
    /// Nombre total d'endpoints
    pub total_endpoints: usize,
}

type ApiRegistryState = Arc<RwLock<Vec<ApiRegistryEntry>>>;

/// Handler pour l'endpoint /api/registry
async fn get_api_registry(State(registry): State<ApiRegistryState>) -> Json<ApiRegistry> {
    let apis = registry.read().await.clone();
    let total_endpoints = apis.iter().map(|api| api.endpoint_count).sum();

    Json(ApiRegistry {
        apis,
        total_endpoints,
    })
}

/// Serveur principal
pub struct Server {
    name: String,
    base_url: String,
    http_port: u16,
    router: Arc<RwLock<Router>>,
    api_router: Arc<RwLock<Option<Router>>>,
    join_handle: Option<JoinHandle<()>>,
    log_state: Option<LogState>,
    api_registry: ApiRegistryState,
    shutdown_token: CancellationToken,
}

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
    /// # use pmoserver::Server;
    /// let server = Server::new("MyAPI", "http://localhost:3000", 3000);
    /// ```
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, http_port: u16) -> Self {
        let api_registry = Arc::new(RwLock::new(Vec::new()));

        // Créer le router initial avec l'endpoint de registre
        let registry_route = Router::new()
            .route("/api/registry", get(get_api_registry))
            .with_state(api_registry.clone());

        Self {
            name: name.into(),
            base_url: base_url.into(),
            http_port,
            router: Arc::new(RwLock::new(registry_route)),
            api_router: Arc::new(RwLock::new(None)),
            join_handle: None,
            log_state: None,
            api_registry,
            shutdown_token: CancellationToken::new(),
        }
    }

    pub fn new_configured() -> Self {
        let config = get_config();
        let url = config.get_base_url();
        let port = config.get_http_port();
        Self::new("PMO-Music-Server", url, port)
    }

    /// Retourne une copie du token d'arrêt gracieux
    ///
    /// Ce token peut être donné aux composants qui ont besoin de savoir
    /// quand le serveur s'arrête (threads, tâches longues, etc.)
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
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
    /// ```rust,ignore
    /// # use pmoserver::Server;
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
        Fut: Future<Output = T> + Send + 'static,
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
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
    }

    /// Ajoute un handler Axum standard
    pub async fn add_handler<H, T>(&mut self, path: &str, handler: H)
    where
        H: Handler<T, ()> + Clone + 'static,
        T: 'static,
    {
        let route = Router::new().route("/", get(handler.clone()));

        let mut r = self.router.write().await;
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
    }

    /// Ajoute un handler POST avec état
    pub async fn add_post_handler_with_state<H, T, S>(&mut self, path: &str, handler: H, state: S)
    where
        H: Handler<T, S> + Clone + 'static,
        T: 'static,
        S: Clone + Send + Sync + 'static,
    {
        let route = Router::new()
            .route("/", post(handler.clone()))
            .with_state(state.clone());

        let mut r = self.router.write().await;
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
    }

    /// Ajoute un handler avec état
    pub async fn add_handler_with_state<H, T, S>(&mut self, path: &str, handler: H, state: S)
    where
        H: Handler<T, S> + Clone + 'static,
        T: 'static,
        S: Clone + Send + Sync + 'static,
    {
        let route = Router::new()
            .route("/", get(handler.clone()))
            .with_state(state.clone());

        let mut r = self.router.write().await;
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
    }

    /// Ajoute un handler qui accepte tous les verbes HTTP (ANY) avec état
    pub async fn add_any_handler_with_state<H, T, S>(&mut self, path: &str, handler: H, state: S)
    where
        H: Handler<T, S> + Clone + 'static,
        T: 'static,
        S: Clone + Send + Sync + 'static,
    {
        let route = Router::new()
            .route("/", any(handler.clone()))
            .with_state(state.clone());

        let mut r = self.router.write().await;
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
    }

    /// Ajoute un répertoire statique
    pub async fn add_dir<E>(&mut self, path: &str)
    where
        E: RustEmbed + Clone + Send + Sync + 'static,
    {
        let serve = ServeEmbed::<E>::new();
        let mut r = self.router.write().await;

        let route = Router::new().fallback_service(serve);
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
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
    /// ```rust,ignore
    /// # use pmoserver::Server;
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

        let route = Router::new().fallback_service(serve);
        *r = if path == "/" {
            std::mem::take(&mut *r).merge(route)
        } else {
            std::mem::take(&mut *r).nest(path, route)
        };
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
    /// ```rust,ignore
    /// # use pmoserver::Server;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// // Rediriger la racine vers /app
    /// server.add_redirect("/", "/app").await;
    /// # }
    /// ```

    pub async fn add_redirect(&mut self, from: &str, to: &str) {
        let to = to.to_string();
        let make_handler = || {
            let target = to.clone();
            get(move || async move { Redirect::permanent(&target) })
        };

        let mut r = self.router.write().await;
        *r = if from == "/" {
            std::mem::take(&mut *r).merge(Router::new().route("/", make_handler()))
        } else {
            std::mem::take(&mut *r).nest(from, Router::new().route("/", make_handler()))
        };
    }

    /// Ajoute une API documentée avec OpenAPI et Swagger UI
    ///
    /// Cette méthode fusionne le `api_router` fourni avec le router principal du serveur.
    /// Chaque appel peut ajouter une nouvelle API distincte, avec sa propre documentation Swagger.
    ///
    /// # Arguments
    ///
    /// * `api_router` - Router Axum contenant les routes API
    /// * `openapi` - Spécification OpenAPI générée par `utoipa`
    /// * `name` - Nom unique pour cette API, utilisé pour différencier le chemin Swagger UI et le JSON OpenAPI
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
    /// struct ApiDoc1;
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
    /// #[derive(utoipa::OpenApi)]
    /// #[openapi(
    ///     paths(get_products),
    ///     components(schemas(Product))
    /// )]
    /// struct ApiDoc2;
    ///
    /// #[utoipa::path(
    ///     get,
    ///     path = "/products",
    ///     responses((status = 200, description = "List products"))
    /// )]
    /// async fn get_products() -> Json<Vec<Product>> {
    ///     Json(vec![])
    /// }
    ///
    /// let api_router1 = Router::new().route("/users", get(get_users));
    /// let api_router2 = Router::new().route("/products", get(get_products));
    ///
    /// // Ajouter les deux API au serveur, chacune avec son nom unique
    /// server.add_openapi(api_router1, ApiDoc1::openapi(), "api1").await;
    /// server.add_openapi(api_router2, ApiDoc2::openapi(), "api2").await;
    /// ```
    ///
    /// Résultat :
    ///
    /// - `/api/api1/users` et `/api/api2/products` sont accessibles via Axum.
    /// - `/swagger-ui/api1` et `/swagger-ui/api2` affichent la documentation Swagger correspondante.
    /// - `/api-docs/api1.json` et `/api-docs/api2.json` fournissent les spécifications OpenAPI respectives.
    pub async fn add_openapi(
        &mut self,
        api_router: Router,
        openapi: utoipa::openapi::OpenApi,
        name: &str,
    ) {
        let mut api_r = self.api_router.write().await;
        *api_r = Some(api_router.clone());
        drop(api_r);

        let swagger_path = format!("/swagger-ui/{}", name);
        let swagger_path_static: &'static str = Box::leak(swagger_path.clone().into_boxed_str());

        let openapi_json_path = format!("/api-docs/{}.json", name);
        let openapi_json_path_static: &'static str =
            Box::leak(openapi_json_path.clone().into_boxed_str());

        // Compter le nombre d'endpoints dans l'OpenAPI spec
        let endpoint_count = openapi.paths.paths.len();

        // Extraire les informations de l'API depuis la spec OpenAPI
        let version = openapi.info.version.clone();
        let description = openapi.info.description.clone();
        let title = openapi.info.title.clone();

        // Enregistrer l'API dans le registre
        let registry_entry = ApiRegistryEntry {
            name: name.to_string(),
            path: format!("/api/{}", name),
            swagger_ui_path: swagger_path,
            openapi_json_path,
            endpoint_count,
            version,
            description,
            title,
        };

        let mut registry = self.api_registry.write().await;
        registry.push(registry_entry);
        drop(registry);

        let swagger = SwaggerUi::new(swagger_path_static).url(openapi_json_path_static, openapi);

        let base_path = format!("/api/{}", name);
        let nested_router = Router::new().nest(&base_path, api_router);

        let mut r = self.router.write().await;
        *r = std::mem::take(&mut *r).merge(nested_router).merge(swagger);
    }
    /// Ajoute un sous-router au serveur
    ///
    /// - Si `path` est "/", merge directement au router principal
    /// - Sinon, nest le router sous le chemin donné
    pub async fn add_router(&mut self, path: &str, sub_router: Router) {
        let mut r = self.router.write().await;

        let combined = if path == "/" {
            // Merge directement à la racine
            r.clone().merge(sub_router)
        } else {
            // Sous-chemin => nest
            let normalized = format!("/{}", path.trim_start_matches('/'));
            r.clone().nest(&normalized, sub_router)
        };

        *r = combined;
    }

    /// Démarre le serveur HTTP
    ///
    /// Lance le serveur sur le port configuré et met en place la gestion
    /// de Ctrl+C pour un arrêt gracieux.
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// # use pmoserver::Server;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut server = Server::new("Test", "http://localhost:3000", 3000);
    /// server.start().await;
    /// server.wait().await;  // Attend Ctrl+C
    /// # }
    /// ```
    pub async fn start(&mut self) {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.http_port));
        info!(
            "Server {} running at [http://{}:{}](http://{}:{})",
            self.name, self.base_url, self.http_port, self.base_url, self.http_port
        );

        let router = self.router.clone();
        let shutdown_token = self.shutdown_token.clone();

        // Créer un channel pour signaler l'arrêt gracieux
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        self.join_handle = Some(tokio::spawn(async move {
            let server_future = async {
                let r = router.read().await.clone();
                let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

                axum::serve(listener, r.into_make_service())
                    .with_graceful_shutdown(async move {
                        let _ = shutdown_rx.await;
                    })
                    .await
            };

            tokio::pin!(server_future);
            let ctrl_c = signal::ctrl_c();
            tokio::pin!(ctrl_c);

            tokio::select! {
                result = &mut server_future => {
                    if let Err(err) = result {
                        error!("Serveur HTTP arrêté avec une erreur: {}", err);
                    } else {
                        info!("Serveur HTTP arrêté proprement");
                    }
                }
                _ = &mut ctrl_c => {
                    info!("Ctrl+C reçu, arrêt gracieux");
                    shutdown_token.cancel();
                    let _ = shutdown_tx.send(());

                    if tokio::time::timeout(std::time::Duration::from_secs(5), &mut server_future).await.is_err() {
                        warn!("Arrêt gracieux trop long, fermeture forcée du serveur HTTP");
                    }
                }
            }
        }));
    }

    /// Attend la fin du serveur
    pub async fn wait(&mut self) {
        if let Some(h) = self.join_handle.take() {
            let _ = h.await;
        }
    }

    /// Retourne l'URL de base complète du serveur (schéma + hôte + port).
    ///
    /// La valeur configurable peut omettre le schéma ou le port ; cette méthode
    /// s'assure donc que les clients reçoivent toujours une URL exploitable comme
    /// `http://192.168.0.10:8080`.
    pub fn base_url(&self) -> String {
        let mut base = self.base_url.trim_end_matches('/').to_string();

        if !base.contains("://") {
            base = format!("http://{}", base);
        }

        let has_port = base
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse::<u16>().ok())
            .is_some();

        if has_port {
            base
        } else {
            format!("{}:{}", base, self.http_port)
        }
    }

    /// Récupère les infos du serveur
    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            name: self.name.clone(),
            base_url: self.base_url(),
            http_port: self.http_port,
        }
    }

    /// Initialise le système de logging et enregistre les routes de logs
    ///
    /// Cette méthode configure le système de tracing avec SSE et optionnellement la console,
    /// puis enregistre automatiquement les routes `/log-sse` et `/log-dump`.
    ///
    /// # Arguments
    ///
    /// * `options` - Options de configuration du logging
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// # use pmoserver::{ServerBuilder, logs::LoggingOptions};
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut server = ServerBuilder::new_configured().build();
    ///
    /// // Initialiser les logs
    /// server.init_logging().await;
    ///
    /// server.start().await;
    /// # }
    /// ```
    pub async fn init_logging(&mut self) {
        let log_state = init_logging();

        // Enregistrer automatiquement les routes de logging SSE
        self.add_handler_with_state("/log-sse", log_sse, log_state.clone())
            .await;
        self.add_handler_with_state("/log-dump", log_dump, log_state.clone())
            .await;

        // Enregistrer l'API REST de configuration des logs via OpenAPI
        self.add_openapi(
            crate::logs::create_logs_router(log_state.clone()),
            crate::logs::LogsApiDoc::openapi(),
            "logs",
        )
        .await;

        self.log_state = Some(log_state);
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
            http_port: config.get_http_port(),
        }
    }

    /// Construit le serveur
    ///
    /// Consomme le builder et retourne une instance de `Server` prête à l'emploi.
    ///
    /// # Exemple
    ///
    /// ```rust
    /// # use pmoserver::ServerBuilder;
    /// let mut server = ServerBuilder::new("MyAPI", "http://localhost:3000", 3000)
    ///     .build();
    /// ```
    pub fn build(self) -> Server {
        Server::new(self.name, self.base_url, self.http_port)
    }
}
