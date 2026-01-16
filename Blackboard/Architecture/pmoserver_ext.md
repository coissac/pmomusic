# Pattern d'extension PMOServer (`pmoserver_ext`)

## Vue d'ensemble

Le pattern `pmoserver_ext` permet d'étendre les fonctionnalités du serveur HTTP `pmoserver` de manière modulaire et découplée. Chaque crate spécialisée peut ajouter ses propres routes HTTP sans que `pmoserver` ne dépende de ces crates.

**Principe** : Définir un trait d'extension que `pmoserver::Server` implémente via une feature Cargo.

## Anatomie d'une extension

### 1. Structure du module

Créer un module `pmoserver_ext.rs` dans la crate :

```rust
// pmoXXX/src/pmoserver_ext.rs

#[cfg(feature = "pmoserver")]
use crate::{/* types internes de la crate */};
#[cfg(feature = "pmoserver")]
use async_trait::async_trait;
#[cfg(feature = "pmoserver")]
use axum::{Router, routing::get, Json, extract::{State, Path}};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
```

Déclarer le module dans `lib.rs` :

```rust
// pmoXXX/src/lib.rs
#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;

#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::XXXExt;
```

Ajouter la feature dans `Cargo.toml` :

```toml
[features]
pmoserver = ["dep:axum", "dep:async-trait"]

[dependencies]
axum = { version = "0.8", optional = true }
async-trait = { version = "0.1", optional = true }
pmoserver = { path = "../pmoserver" }
```

### 2. Définir le trait d'extension

**Convention de nommage** : `{Domaine}Ext` avec méthodes préfixées `init_*`

```rust
/// Trait pour étendre pmoserver avec les fonctionnalités XXX
#[cfg(feature = "pmoserver")]
#[async_trait]
pub trait XXXExt {
    /// Initialise l'extension XXX et enregistre les routes HTTP
    ///
    /// # Arguments
    /// * `param1` - Description du paramètre
    ///
    /// # Returns
    /// Instance partagée de la ressource créée
    ///
    /// # Exemple
    /// ```ignore
    /// use pmoserver::ServerBuilder;
    /// use pmoXXX::XXXExt;
    ///
    /// let mut server = ServerBuilder::new(...).build();
    /// let resource = server.init_xxx(param1).await?;
    /// ```
    async fn init_xxx(&mut self, param1: String) -> anyhow::Result<Arc<Resource>>;
}
```

### 3. Implémenter le trait

Implémenter le trait pour `pmoserver::Server` :

```rust
#[cfg(feature = "pmoserver")]
#[async_trait]
impl XXXExt for pmoserver::Server {
    async fn init_xxx(&mut self, param1: String) -> anyhow::Result<Arc<Resource>> {
        // 1. Créer la ressource interne
        let resource = Arc::new(Resource::new(param1)?);
        
        // 2. Créer l'état partagé pour les handlers
        let state = XxxState::new(resource.clone());
        
        // 3. Créer le router avec les routes
        let router = create_xxx_router(state);
        
        // 4. Enregistrer le router sur le serveur
        self.add_router("/api/xxx", router).await;
        
        // 5. Retourner la ressource pour usage ultérieur
        Ok(resource)
    }
}
```

### 4. État partagé (State)

Créer une structure d'état cloneable pour les handlers :

```rust
/// État partagé pour les handlers XXX
#[derive(Clone)]
pub struct XxxState {
    resource: Arc<Resource>,
}

impl XxxState {
    pub fn new(resource: Arc<Resource>) -> Self {
        Self { resource }
    }
}
```

### 5. Créer le router

Définir les routes et handlers :

```rust
/// Crée le router pour l'API XXX
fn create_xxx_router(state: XxxState) -> Router {
    Router::new()
        .route("/items", get(list_items).post(create_item))
        .route("/items/{id}", get(get_item).delete(delete_item))
        .with_state(state)
}

// Handlers
async fn list_items(
    State(state): State<XxxState>
) -> Json<Vec<ItemSummary>> {
    let items = state.resource.list_items();
    Json(items)
}

async fn get_item(
    State(state): State<XxxState>,
    Path(id): Path<String>,
) -> Result<Json<Item>, StatusCode> {
    state.resource.get_item(&id)
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}
```

## Méthodes disponibles du serveur

`pmoserver::Server` expose ces méthodes pour enregistrer des routes :

| Méthode | Usage |
|---------|-------|
| `add_handler(path, handler)` | Ajoute un handler simple sans état |
| `add_handler_with_state(path, handler, state)` | Ajoute un handler avec état partagé |
| `add_router(path, router)` | Monte un sous-router Axum |
| `add_openapi(router, doc, tag)` | Enregistre une API avec documentation OpenAPI |
| `add_spa::<W>(path)` | Sert une Single Page Application (RustEmbed) |
| `base_url()` | Récupère l'URL de base du serveur |

## Documentation OpenAPI avec utoipa

La documentation OpenAPI est essentielle pour une extension `pmoserver`. Elle génère automatiquement une interface Swagger UI et documente les endpoints de l'API.

### Configuration de base

Ajouter `utoipa` dans `Cargo.toml` :

```toml
[dependencies]
utoipa = { version = "5", features = ["axum_extras"] }
serde = { version = "1", features = ["derive"] }
```

### 1. Définir les schémas de données

Annoter les structures de réponse/requête avec `#[derive(ToSchema)]` :

```rust
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;

/// Information sur un item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ItemInfo {
    /// ID unique de l'item
    #[schema(example = "item-123")]
    pub id: String,
    
    /// Nom de l'item
    #[schema(example = "Mon Item")]
    pub name: String,
    
    /// Description optionnelle
    #[schema(example = "Une description détaillée")]
    pub description: Option<String>,
    
    /// Timestamp de création (millisecondes)
    #[schema(example = 1234567890)]
    pub created_at: u64,
}

/// Liste d'items
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItemList {
    /// Nombre total d'items
    pub total: usize,
    
    /// Items de la page courante
    pub items: Vec<ItemInfo>,
}

/// Requête de création d'item
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateItemRequest {
    /// Nom de l'item à créer
    #[schema(example = "Nouvel Item")]
    pub name: String,
    
    /// Description optionnelle
    pub description: Option<String>,
}

/// Réponse d'erreur standard
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Message d'erreur
    #[schema(example = "Item not found")]
    pub error: String,
}
```

**Points clés** :
- `#[schema(example = "...")]` : Fournit des exemples pour la doc Swagger
- Documenter chaque champ avec `///` pour apparaître dans l'API
- Utiliser `Option<T>` pour les champs optionnels

### 2. Annoter les handlers

Utiliser `#[utoipa::path(...)]` pour documenter chaque endpoint :

```rust
/// GET /items - Liste tous les items
#[utoipa::path(
    get,
    path = "/items",
    params(
        ("limit" = Option<u32>, Query, description = "Nombre max d'items à retourner"),
        ("offset" = Option<u32>, Query, description = "Offset pour la pagination")
    ),
    responses(
        (status = 200, description = "Liste des items", body = ItemList),
        (status = 500, description = "Erreur serveur", body = ErrorResponse)
    ),
    tag = "items"
)]
async fn list_items(
    State(state): State<XxxState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ItemList>, (StatusCode, Json<ErrorResponse>)> {
    let items = state.resource.list_items(params.limit, params.offset)
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() })
        ))?;
    
    Ok(Json(ItemList {
        total: items.len(),
        items,
    }))
}

/// GET /items/{id} - Récupère un item spécifique
#[utoipa::path(
    get,
    path = "/items/{id}",
    params(
        ("id" = String, Path, description = "ID unique de l'item")
    ),
    responses(
        (status = 200, description = "Item trouvé", body = ItemInfo),
        (status = 404, description = "Item non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur serveur", body = ErrorResponse)
    ),
    tag = "items"
)]
async fn get_item(
    State(state): State<XxxState>,
    Path(id): Path<String>,
) -> Result<Json<ItemInfo>, (StatusCode, Json<ErrorResponse>)> {
    state.resource.get_item(&id)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Item {} not found", id)
            })
        ))
        .map(Json)
}

/// POST /items - Crée un nouvel item
#[utoipa::path(
    post,
    path = "/items",
    request_body = CreateItemRequest,
    responses(
        (status = 201, description = "Item créé", body = ItemInfo),
        (status = 400, description = "Requête invalide", body = ErrorResponse),
        (status = 500, description = "Erreur serveur", body = ErrorResponse)
    ),
    tag = "items"
)]
async fn create_item(
    State(state): State<XxxState>,
    Json(req): Json<CreateItemRequest>,
) -> Result<(StatusCode, Json<ItemInfo>), (StatusCode, Json<ErrorResponse>)> {
    let item = state.resource.create_item(req.name, req.description)
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() })
        ))?;
    
    Ok((StatusCode::CREATED, Json(item)))
}

/// DELETE /items/{id} - Supprime un item
#[utoipa::path(
    delete,
    path = "/items/{id}",
    params(
        ("id" = String, Path, description = "ID unique de l'item")
    ),
    responses(
        (status = 204, description = "Item supprimé"),
        (status = 404, description = "Item non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur serveur", body = ErrorResponse)
    ),
    tag = "items"
)]
async fn delete_item(
    State(state): State<XxxState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.resource.delete_item(&id)
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() })
        ))?;
    
    Ok(StatusCode::NO_CONTENT)
}
```

**Structure de `#[utoipa::path]`** :
- **Méthode HTTP** : `get`, `post`, `put`, `delete`, `patch`
- **`path`** : Chemin de l'endpoint (doit correspondre au router)
- **`params`** : Paramètres Path ou Query avec description
- **`request_body`** : Type du body pour POST/PUT
- **`responses`** : Liste des réponses possibles avec codes HTTP
- **`tag`** : Groupe d'endpoints dans Swagger UI

### 3. Créer la structure OpenAPI

Définir une structure avec `#[derive(OpenApi)]` :

```rust
use utoipa::OpenApi;

/// Documentation OpenAPI pour l'API XXX
#[derive(OpenApi)]
#[openapi(
    info(
        title = "XXX API",
        version = "1.0.0",
        description = r#"
# API REST pour XXX

Cette API permet de gérer les items XXX avec les fonctionnalités suivantes :

## Fonctionnalités

- **CRUD complet** : Création, lecture, mise à jour et suppression d'items
- **Pagination** : Support de limit/offset pour les listes
- **Filtrage** : Recherche par critères multiples
- **Validation** : Vérification automatique des données

## Exemples d'utilisation

### Lister les items
```
GET /api/xxx/items?limit=10&offset=0
```

### Créer un item
```
POST /api/xxx/items
Content-Type: application/json

{
  "name": "Mon Item",
  "description": "Description détaillée"
}
```

### Récupérer un item
```
GET /api/xxx/items/item-123
```

### Supprimer un item
```
DELETE /api/xxx/items/item-123
```
        "#
    ),
    paths(
        list_items,
        get_item,
        create_item,
        delete_item,
    ),
    components(schemas(
        ItemInfo,
        ItemList,
        CreateItemRequest,
        ErrorResponse,
    )),
    tags(
        (name = "items", description = "Opérations sur les items")
    )
)]
pub struct ApiDoc;
```

**Sections importantes** :
- **`info`** : Titre, version et description Markdown de l'API
- **`paths`** : Liste des fonctions handler annotées
- **`components(schemas(...))`** : Liste des structures `ToSchema`
- **`tags`** : Organisation des endpoints en groupes

### 4. Enregistrer l'API avec OpenAPI

Dans l'implémentation du trait d'extension :

```rust
#[async_trait]
impl XxxExt for pmoserver::Server {
    async fn init_xxx(&mut self) -> anyhow::Result<Arc<Resource>> {
        let resource = Arc::new(Resource::new()?);
        let state = XxxState { resource: resource.clone() };
        
        // Créer le router avec les routes
        let router = Router::new()
            .route("/items", get(list_items).post(create_item))
            .route("/items/{id}", get(get_item).delete(delete_item))
            .with_state(state);
        
        // Enregistrer avec OpenAPI (génère aussi /swagger-ui/xxx)
        let openapi = ApiDoc::openapi();
        self.add_openapi(router, openapi, "xxx").await;
        
        Ok(resource)
    }
}
```

**Ce que fait `add_openapi`** :
- Monte le router sur `/api/{tag}/`
- Génère la spec OpenAPI JSON sur `/api/{tag}/openapi.json`
- Crée une UI Swagger sur `/swagger-ui/{tag}/`

### 5. Exemple complet : Radio Paradise

**Extrait de** `pmoparadise/src/pmoserver_ext.rs:93-315`

```rust
/// Information sur un morceau
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SongInfo {
    /// Index dans le block
    pub index: usize,
    /// Artiste
    pub artist: String,
    /// Titre
    pub title: String,
    /// Album
    pub album: String,
    /// Année
    pub year: Option<u32>,
    /// Temps écoulé depuis le début du block (ms)
    pub elapsed_ms: u64,
    /// Durée du morceau (ms)
    pub duration_ms: u64,
    /// URL de la pochette
    pub cover_url: Option<String>,
}

/// Réponse pour l'URL de streaming
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StreamUrlResponse {
    /// Event ID du block
    #[schema(example = 1234567)]
    pub event: u64,
    /// URL de streaming FLAC
    #[schema(example = "https://apps.radioparadise.com/blocks/chan/0/4/1234567-1234580.flac")]
    pub stream_url: String,
    /// Durée totale (ms)
    #[schema(example = 900000)]
    pub length_ms: u64,
}

/// GET /stream-url/{event_id} - Récupère l'URL de streaming
#[utoipa::path(
    get,
    path = "/stream-url/{event_id}",
    params(
        ("event_id" = u64, Path, description = "Event ID du block"),
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "URL de streaming", body = StreamUrlResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_stream_url(
    State(state): State<RadioParadiseState>,
    Path(event_id): Path<u64>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<StreamUrlResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let block = client.get_block(Some(event_id)).await.map_err(|e| {
        tracing::error!("Failed to fetch block {}: {}", event_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(StreamUrlResponse {
        event: block.event,
        stream_url: block.url,
        length_ms: block.length,
    }))
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Radio Paradise API",
        version = "1.0.0",
        description = "API REST pour accéder aux métadonnées Radio Paradise"
    ),
    paths(
        get_now_playing,
        get_current_block,
        get_stream_url,
    ),
    components(schemas(
        SongInfo,
        StreamUrlResponse,
    )),
    tags(
        (name = "Radio Paradise", description = "Endpoints Radio Paradise")
    )
)]
pub struct RadioParadiseApiDoc;
```

### Résultat : Interface Swagger

Après avoir appelé `init_xxx()`, l'API est accessible :

- **API JSON** : `http://localhost:8080/api/xxx/`
- **Spec OpenAPI** : `http://localhost:8080/api/xxx/openapi.json`
- **Swagger UI** : `http://localhost:8080/swagger-ui/xxx/`

L'interface Swagger permet :
- Parcourir tous les endpoints avec leur documentation
- Tester les requêtes directement depuis le navigateur
- Voir les schémas de données avec exemples
- Consulter les codes de réponse HTTP possibles

## Patterns courants

### Pattern 1 : Extension simple avec router

**Exemple** : `pmoparadise` (pmoparadise/src/pmoserver_ext.rs:367-392)

```rust
#[async_trait]
impl RadioParadiseExt for pmoserver::Server {
    async fn init_radioparadise(&mut self) -> anyhow::Result<State> {
        let state = RadioParadiseState::new().await?;
        
        // Créer le router API
        let api_router = create_api_router(state.clone());
        
        // Enregistrer avec OpenAPI
        self.add_openapi(api_router, ApiDoc::openapi(), "radioparadise")
            .await;
        
        Ok(state)
    }
}
```

### Pattern 2 : Extension avec cache et fichiers

**Exemple** : `pmoaudiocache` (pmoaudiocache/src/lib.rs:225-260)

```rust
#[async_trait]
impl AudioCacheExt for pmoserver::Server {
    async fn init_audio_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<Cache>> {
        let cache = Arc::new(new_cache(cache_dir, limit)?);
        
        // Router pour servir les fichiers FLAC
        let file_router = create_file_router(cache.clone(), "audio/flac");
        self.add_router("/", file_router).await;
        
        // API REST
        let api_router = Router::new()
            .route("/", get(list).post(add))
            .route("/{pk}", get(get_info).delete(delete))
            .with_state(cache.clone());
        
        self.add_openapi(api_router, ApiDoc::openapi(), "audio").await;
        
        Ok(cache)
    }
}
```

### Pattern 3 : Extension avec routes dynamiques

**Exemple** : `pmomediaserver` (pmomediaserver/src/paradise_streaming.rs:70-148)

```rust
#[async_trait]
impl ParadiseStreamingExt for pmoserver::Server {
    async fn init_paradise_streaming(&mut self) -> Result<Arc<Manager>> {
        // 1. Récupérer/créer les ressources partagées
        let audio_cache = get_or_init_audio_cache(self).await?;
        let manager = Arc::new(Manager::new(audio_cache).await?);
        
        // 2. Créer l'état partagé
        let state = Arc::new(StreamingState { manager: manager.clone() });
        
        // 3. Enregistrer les routes pour chaque canal
        for descriptor in ALL_CHANNELS.iter() {
            let slug = descriptor.slug;
            
            // Route streaming FLAC
            let path = format!("/stream/{}/flac", slug);
            self.add_handler_with_state(
                &path,
                move |State(s): State<Arc<StreamingState>>| async move {
                    stream_flac(s.manager.clone(), descriptor.id).await
                },
                state.clone(),
            ).await;
            
            // Route streaming OGG
            let path = format!("/stream/{}/ogg", slug);
            self.add_handler_with_state(
                &path,
                move |State(s): State<Arc<StreamingState>>| async move {
                    stream_ogg(s.manager.clone(), descriptor.id).await
                },
                state.clone(),
            ).await;
        }
        
        Ok(manager)
    }
}
```

## Gestion des opérations longues

### Utiliser `spawn_blocking` pour le code synchrone

Pour éviter de bloquer le runtime Tokio avec du code synchrone :

```rust
async fn list_renderers(
    State(state): State<ControlPointState>
) -> Json<Vec<Summary>> {
    let control_point = state.control_point.clone();
    
    let summaries = tokio::task::spawn_blocking(move || {
        let renderers = control_point.list_music_renderers();
        renderers.into_iter()
            .map(|r| Summary::from(&r))
            .collect()
    })
    .await
    .unwrap_or_default();
    
    Json(summaries)
}
```

### Ajouter des timeouts pour les opérations réseau

```rust
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

async fn play_renderer(
    State(state): State<ControlPointState>,
    Path(id): Path<String>,
) -> Result<Json<Response>, (StatusCode, Json<Error>)> {
    let renderer = state.get_renderer(&id)
        .ok_or((StatusCode::NOT_FOUND, Json(Error::not_found())))?;
    
    let play_task = tokio::task::spawn_blocking(move || renderer.play());
    
    time::timeout(COMMAND_TIMEOUT, play_task)
        .await
        .map_err(|_| (
            StatusCode::GATEWAY_TIMEOUT,
            Json(Error::timeout())
        ))?
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Error::internal(e))
        ))??;
    
    Ok(Json(Response::success()))
}
```

### Utiliser `spawn` pour les tâches en arrière-plan

Pour les opérations qui ne nécessitent pas d'attendre le résultat :

```rust
async fn trigger_action(
    State(state): State<XxxState>,
    Json(req): Json<Request>,
) -> Json<Response> {
    // Valider la requête
    state.validate(&req)?;
    
    // Lancer l'action en arrière-plan
    let state_clone = state.clone();
    tokio::task::spawn(async move {
        match state_clone.perform_action(req).await {
            Ok(_) => debug!("Action completed"),
            Err(e) => warn!("Action failed: {}", e),
        }
    });
    
    // Retourner immédiatement
    Json(Response::accepted())
}
```

## Checklist d'implémentation

### Configuration de base
- [ ] Créer le module `pmoserver_ext.rs` avec `#[cfg(feature = "pmoserver")]`
- [ ] Ajouter la feature `pmoserver` dans `Cargo.toml` avec dépendances optionnelles
- [ ] Re-exporter le trait dans `lib.rs`

### Définition du trait
- [ ] Définir le trait `{Domaine}Ext` avec méthode `init_*`
- [ ] Créer la structure `{Domaine}State` avec `#[derive(Clone)]`
- [ ] Implémenter le trait pour `pmoserver::Server`

### Documentation OpenAPI
- [ ] Ajouter `utoipa` dans les dépendances
- [ ] Définir les schémas de réponse/requête avec `#[derive(ToSchema)]`
- [ ] Ajouter des exemples avec `#[schema(example = "...")]`
- [ ] Annoter chaque handler avec `#[utoipa::path(...)]`
- [ ] Créer la structure `#[derive(OpenApi)]` avec documentation complète
- [ ] Lister tous les paths et schemas dans `#[openapi(...)]`

### Handlers et routes
- [ ] Créer les handlers avec les extracteurs Axum appropriés
- [ ] Gérer les erreurs avec des codes HTTP sémantiques
- [ ] Créer le router et l'enregistrer avec `add_openapi()`
- [ ] Ajouter des logs (debug, info, warn, error)

### Performance et robustesse
- [ ] Utiliser `spawn_blocking` pour le code synchrone
- [ ] Ajouter des timeouts pour les opérations réseau
- [ ] Utiliser `spawn` pour les tâches en arrière-plan si nécessaire

## Exemple complet minimal

```rust
// pmoexample/src/pmoserver_ext.rs

#[cfg(feature = "pmoserver")]
use async_trait::async_trait;
#[cfg(feature = "pmoserver")]
use axum::{Router, routing::get, Json, extract::State};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use crate::ExampleResource;

#[cfg(feature = "pmoserver")]
#[derive(Clone)]
pub struct ExampleState {
    resource: Arc<ExampleResource>,
}

#[cfg(feature = "pmoserver")]
#[async_trait]
pub trait ExampleExt {
    async fn init_example(&mut self) -> anyhow::Result<Arc<ExampleResource>>;
}

#[cfg(feature = "pmoserver")]
#[async_trait]
impl ExampleExt for pmoserver::Server {
    async fn init_example(&mut self) -> anyhow::Result<Arc<ExampleResource>> {
        let resource = Arc::new(ExampleResource::new());
        let state = ExampleState { resource: resource.clone() };
        
        let router = Router::new()
            .route("/items", get(list_items))
            .with_state(state);
        
        self.add_router("/api/example", router).await;
        
        Ok(resource)
    }
}

#[cfg(feature = "pmoserver")]
async fn list_items(State(state): State<ExampleState>) -> Json<Vec<String>> {
    let items = state.resource.list();
    Json(items)
}
```

## Références

### Exemples dans le codebase

| Crate | Fichier | Pattern |
|-------|---------|---------|
| `pmoparadise` | `src/pmoserver_ext.rs:367-392` | Extension simple avec OpenAPI |
| `pmoaudiocache` | `src/lib.rs:225-260` | Extension avec cache et fichiers |
| `pmomediaserver` | `src/paradise_streaming.rs:70-148` | Extension avec routes dynamiques |
| `pmocontrol` | `src/pmoserver_ext.rs:68-92` | Handlers avec `spawn_blocking` |
| `pmoapp` | `src/lib.rs:145-165` | Extension SPA avec RustEmbed |

### Dépendances communes

- `axum` : Framework HTTP (Router, handlers, extracteurs)
- `async-trait` : Support des traits async
- `tokio` : Runtime async (spawn, spawn_blocking, timeout)
- `anyhow` : Gestion d'erreurs pour init
- `tracing` : Logging structuré
- `utoipa` : Documentation OpenAPI/Swagger
- `serde` : Sérialisation JSON
