# Pattern d'extension du serveur PMO (pmoserver_ext)

## Vue d'ensemble

Le pattern `pmoserver_ext` permet d'étendre les fonctionnalités du serveur HTTP `pmoserver` de manière modulaire et découplée. Chaque crate spécialisée (audio, images, UPnP, Radio Paradise, etc.) peut ajouter ses propres routes HTTP, API REST et documentation OpenAPI sans que `pmoserver` ne dépende de ces crates.

## Architecture du pattern

### Principe de base

Le pattern utilise le système de traits Rust pour définir une interface d'extension que `pmoserver::Server` implémente. Chaque crate fonctionnelle définit son propre trait d'extension avec des méthodes préfixées par convention (ex: `init_*`, `add_*`).

```
┌─────────────────────────────────────────────────────────┐
│                    pmoserver (core)                      │
│  ┌────────────────────────────────────────────┐         │
│  │          Server (struct)                    │         │
│  │  - Router Axum                              │         │
│  │  - add_handler(), add_router()              │         │
│  │  - add_openapi(), add_spa()                 │         │
│  └────────────────────────────────────────────┘         │
└─────────────────────────────────────────────────────────┘
                         ▲
                         │ impl Trait
          ┌──────────────┴──────────────┐
          │                             │
┌─────────┴──────────┐       ┌──────────┴─────────────┐
│  pmoaudiocache     │       │   pmoparadise          │
│  ┌──────────────┐  │       │  ┌──────────────────┐  │
│  │AudioCacheExt │  │       │  │RadioParadiseExt  │  │
│  └──────────────┘  │       │  └──────────────────┘  │
└────────────────────┘       └────────────────────────┘
```

### Avantages

1. **Découplage** : `pmoserver` ne connaît pas les crates spécialisées
2. **Modularité** : Chaque fonctionnalité est opt-in via features Cargo
3. **Cohérence** : Interface uniforme pour toutes les extensions
4. **Testabilité** : Chaque extension peut être testée indépendamment

## Composants du pattern

### 1. Trait d'extension

Définir un trait public avec des méthodes d'initialisation/configuration.

**Convention de nommage** :
- Trait : `{Domaine}Ext` (ex: `AudioCacheExt`, `RadioParadiseExt`)
- Méthodes : `init_{domaine}*`, `add_{domaine}*`

**Exemple** (pmoaudiocache/src/lib.rs:200-215):
```rust
#[cfg(feature = "pmoserver")]
pub trait AudioCacheExt {
    /// Initialise le cache audio et enregistre les routes HTTP
    async fn init_audio_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<Cache>>;

    /// Initialise avec configuration par défaut
    async fn init_audio_cache_configured(&mut self) 
        -> anyhow::Result<Arc<Cache>>;
}
```

### 2. Implémentation du trait

Implémenter le trait pour `pmoserver::Server` en utilisant les méthodes publiques du serveur.

**Méthodes disponibles du serveur** :
- `add_handler()` : Ajoute un handler simple
- `add_handler_with_state()` : Ajoute un handler avec état partagé
- `add_router()` : Monte un sous-router Axum
- `add_openapi()` : Enregistre une API avec documentation OpenAPI
- `add_spa()` : Sert une Single Page Application (RustEmbed)
- `base_url()` : Récupère l'URL de base du serveur

**Exemple** (pmoaudiocache/src/lib.rs:225-260):
```rust
#[cfg(feature = "pmoserver")]
impl AudioCacheExt for pmoserver::Server {
    async fn init_audio_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<Cache>> {
        // 1. Créer le cache
        let cache = Arc::new(new_cache(cache_dir, limit)?);

        // 2. Router pour servir les fichiers
        let file_router = create_file_router(
            cache.clone(),
            "audio/flac", // Content-Type
        );
        self.add_router("/", file_router).await;

        // 3. API REST avec état
        let api_router = Router::new()
            .route("/", get(list).post(add).delete(purge))
            .route("/{pk}", get(get_info).delete(delete))
            .with_state(cache.clone());

        // 4. Documentation OpenAPI
        let openapi = ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "audio").await;

        Ok(cache)
    }
}
```

### 3. État partagé (State)

Pour les handlers qui nécessitent un état, créer une structure dédiée cloneable.

**Convention** :
- Nom : `{Domaine}State`
- Doit implémenter `Clone`
- Contient des `Arc<T>` pour les ressources partagées

**Exemple** (pmoparadise/src/pmoserver_ext.rs:18-22):
```rust
#[derive(Clone)]
pub struct RadioParadiseState {
    client: Arc<RwLock<RadioParadiseClient>>,
}

impl RadioParadiseState {
    pub async fn new() -> anyhow::Result<Self> {
        let client = RadioParadiseClient::new().await?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
        })
    }
}
```

### 4. Handlers HTTP

Définir les handlers comme des fonctions async avec les extracteurs Axum.

**Extracteurs courants** :
- `State<T>` : Accès à l'état partagé
- `Path<T>` : Paramètres d'URL
- `Query<T>` : Paramètres de query string
- `Json<T>` : Body JSON

**Exemple** (pmoparadise/src/pmoserver_ext.rs:168-180):
```rust
#[utoipa::path(
    get,
    path = "/now-playing",
    params(("channel" = Option<u8>, Query)),
    responses((status = 200, body = NowPlayingResponse)),
    tag = "Radio Paradise"
)]
async fn get_now_playing(
    State(state): State<RadioParadiseState>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<NowPlayingResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let now_playing = client.now_playing().await?;
    Ok(Json(now_playing.into()))
}
```

### 5. Documentation OpenAPI (optionnel)

Utiliser `utoipa` pour générer automatiquement la documentation Swagger.

**Étapes** :
1. Annoter les handlers avec `#[utoipa::path(...)]`
2. Définir les schémas avec `#[derive(ToSchema)]`
3. Créer une structure `#[derive(OpenApi)]`

**Exemple** (pmoparadise/src/pmoserver_ext.rs:315-350):
```rust
use utoipa::{OpenApi, ToSchema};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NowPlayingResponse {
    pub event: u64,
    pub stream_url: String,
    pub songs: Vec<SongInfo>,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Radio Paradise API",
        version = "1.0.0",
        description = "API REST pour Radio Paradise"
    ),
    paths(
        get_now_playing,
        get_current_block,
    ),
    components(schemas(
        NowPlayingResponse,
        SongInfo,
    )),
    tags((name = "Radio Paradise"))
)]
pub struct RadioParadiseApiDoc;
```

### 6. Router Axum

Pour des endpoints complexes, créer un sous-router réutilisable.

**Exemple** (pmoparadise/src/pmoserver_ext.rs:354-365):
```rust
pub fn create_api_router(state: RadioParadiseState) -> Router {
    Router::new()
        .route("/now-playing", get(get_now_playing))
        .route("/block/current", get(get_current_block))
        .route("/block/{event_id}", get(get_block_by_id))
        .route("/channels", get(get_channels))
        .with_state(state)
}
```

## Pattern avancé : Extension avec async-trait

Pour les extensions nécessitant des opérations asynchrones complexes, utiliser `async_trait`.

**Exemple** (pmomediaserver/src/paradise_streaming.rs:31-68):
```rust
use async_trait::async_trait;

#[async_trait]
pub trait ParadiseStreamingExt {
    async fn init_paradise_streaming(&mut self) 
        -> Result<Arc<ParadiseChannelManager>>;
}

#[async_trait]
impl ParadiseStreamingExt for pmoserver::Server {
    async fn init_paradise_streaming(&mut self) 
        -> Result<Arc<ParadiseChannelManager>> 
    {
        // 1. Récupérer/initialiser des caches singletons
        let audio_cache = match get_audio_cache() {
            Some(cache) => cache,
            None => {
                let cache = self.init_audio_cache_configured().await?;
                register_audio_cache(cache.clone());
                cache
            }
        };

        // 2. Créer le manager de canaux
        let manager = Arc::new(
            ParadiseChannelManager::with_defaults(base_url).await?
        );
        register_global_manager(manager.clone());

        // 3. Enregistrer les routes pour chaque canal
        for descriptor in ALL_CHANNELS.iter() {
            let path = format!("/stream/{}/flac", descriptor.slug);
            self.add_handler_with_state(&path, handler, state).await;
        }

        Ok(manager)
    }
}
```

## Pattern d'intégration : Control Point

Le Control Point illustre une extension avec API REST complète incluant gestion d'état, timeouts et spawn de tâches.

### Structure de l'état

**Exemple** (pmocontrol/src/pmoserver_ext.rs:42-51):
```rust
#[derive(Clone)]
pub struct ControlPointState {
    control_point: Arc<ControlPoint>,
}

impl ControlPointState {
    pub fn new(control_point: Arc<ControlPoint>) -> Self {
        Self { control_point }
    }
}
```

### Handlers avec spawn_blocking

Pour les opérations synchrones UPnP, utiliser `spawn_blocking` pour éviter de bloquer le runtime Tokio.

**Exemple** (pmocontrol/src/pmoserver_ext.rs:68-92):
```rust
async fn list_renderers(
    State(state): State<ControlPointState>
) -> Json<Vec<RendererSummary>> {
    // Déporter le travail synchrone sur un thread dédié
    let control_point = state.control_point.clone();
    let summaries = tokio::task::spawn_blocking(move || {
        let renderers = control_point.list_music_renderers();
        renderers.into_iter()
            .map(|r| RendererSummary {
                id: r.id().0.clone(),
                friendly_name: r.friendly_name().to_string(),
                online: r.is_online(),
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    Json(summaries)
}
```

### Handlers avec timeouts

Pour les commandes réseau, toujours utiliser des timeouts pour éviter les blocages.

**Exemple** (pmocontrol/src/pmoserver_ext.rs:240-280):
```rust
const TRANSPORT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

async fn play_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = DeviceId(renderer_id.clone());
    let renderer = state.control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Renderer {} not found", renderer_id)
            })
        ))?;

    // Spawn blocking task avec timeout
    let play_task = tokio::task::spawn_blocking(move || 
        renderer.play()
    );

    time::timeout(TRANSPORT_COMMAND_TIMEOUT, play_task)
        .await
        .map_err(|_| {
            warn!("Play command timeout");
            (StatusCode::GATEWAY_TIMEOUT, Json(ErrorResponse {
                error: "Command timed out".to_string()
            }))
        })?
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: format!("Task error: {}", e)
            }))
        })?
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: format!("Failed to play: {}", e)
            }))
        })?;

    Ok(Json(SuccessResponse {
        message: "Playback started".to_string()
    }))
}
```

### Handlers avec spawn en arrière-plan

Pour les opérations longues qui ne nécessitent pas d'attente, utiliser `tokio::spawn` et retourner immédiatement.

**Exemple** (pmocontrol/src/pmoserver_ext.rs:635-675):
```rust
async fn seek_queue_index(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
    Json(payload): Json<SeekQueueRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = DeviceId(renderer_id.clone());
    state.control_point.music_renderer_by_id(&rid)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Renderer {} not found", renderer_id)
            })
        ))?;

    // Lancer la commande en arrière-plan et retourner immédiatement
    // L'UI sera mise à jour via SSE
    let control_point = Arc::clone(&state.control_point);
    let rid_for_task = rid.clone();
    let index = payload.index;

    tokio::task::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            control_point.play_queue_index(&rid_for_task, index)
        }).await;

        match result {
            Ok(Ok(())) => {
                debug!("Successfully started playback at index {}", index);
            }
            Ok(Err(e)) => {
                warn!("Failed to seek to index {}: {}", index, e);
            }
            Err(e) => {
                warn!("Task join error: {}", e);
            }
        }
    });

    // Retour immédiat
    Ok(Json(SuccessResponse {
        message: format!("Playing item at index {}", index)
    }))
}
```

## Pattern d'intégration : WebApp (SPA)

Le module `pmoapp` illustre l'intégration d'une application Vue.js via RustEmbed.

### Trait d'extension SPA

**Exemple** (pmoapp/src/lib.rs:145-165):
```rust
pub trait WebAppExt {
    /// Ajoute une Single Page Application
    async fn add_webapp<W>(&mut self, path: &str)
    where
        W: RustEmbed + Clone + Send + Sync + 'static;

    /// Ajoute une webapp avec redirection automatique
    async fn add_webapp_with_redirect<W>(&mut self, path: &str)
    where
        W: RustEmbed + Clone + Send + Sync + 'static;
}
```

### Structure RustEmbed

**Exemple** (pmoapp/src/lib.rs:130-140):
```rust
use rust_embed::RustEmbed;

#[derive(RustEmbed, Clone)]
#[folder = "webapp/dist"]
pub struct Webapp;
```

### Implémentation (feature-gated)

**Fichier** : pmoapp/src/pmoserver_impl.rs
```rust
#[cfg(feature = "pmoserver")]
impl WebAppExt for pmoserver::Server {
    async fn add_webapp<W>(&mut self, path: &str)
    where
        W: RustEmbed + Clone + Send + Sync + 'static
    {
        self.add_spa::<W>(path).await
    }

    async fn add_webapp_with_redirect<W>(&mut self, path: &str)
    where
        W: RustEmbed + Clone + Send + Sync + 'static
    {
        self.add_spa::<W>(path).await;
        self.add_redirect("/", path).await;
    }
}
```

## Registres globaux (Singletons)

Pour les ressources partagées entre extensions, utiliser des registres globaux.

### Pattern de singleton

**Exemple** (pmoaudiocache/src/lib.rs:268-295):
```rust
use once_cell::sync::OnceCell;
use std::sync::Arc;

static AUDIO_CACHE: OnceCell<Arc<Cache>> = OnceCell::new();

/// Enregistre le cache audio global
pub fn register_audio_cache(cache: Arc<Cache>) {
    let _ = AUDIO_CACHE.set(cache);
}

/// Accès global au cache audio
pub fn get_audio_cache() -> Option<Arc<Cache>> {
    AUDIO_CACHE.get().cloned()
}
```

### Utilisation dans les extensions

**Exemple** (pmomediaserver/src/paradise_streaming.rs:77-95):
```rust
async fn init_paradise_streaming(&mut self) -> Result<Arc<Manager>> {
    // Récupérer ou initialiser le singleton
    let audio_cache = match get_audio_cache() {
        Some(cache) => {
            info!("Using existing audio cache singleton");
            cache
        }
        None => {
            info!("Initializing new audio cache singleton");
            let cache = self.init_audio_cache_configured().await?;
            register_audio_cache(cache.clone());
            cache
        }
    };

    // Utiliser le cache dans le manager
    let manager = Manager::new(audio_cache).await?;
    Ok(Arc::new(manager))
}
```

## Checklist d'implémentation

Pour implémenter une nouvelle extension `pmoserver`, suivre ces étapes :

### 1. Structure du module

- [ ] Créer un module `pmoserver_ext.rs` dans la crate
- [ ] Ajouter la feature `pmoserver` dans `Cargo.toml`
- [ ] Importer le module avec `#[cfg(feature = "pmoserver")]`

### 2. Définition du trait

- [ ] Créer un trait public `{Domaine}Ext`
- [ ] Ajouter des méthodes préfixées `init_*` ou `add_*`
- [ ] Documenter chaque méthode avec des exemples
- [ ] Ajouter `#[async_trait]` si nécessaire

### 3. État partagé

- [ ] Créer une structure `{Domaine}State`
- [ ] Implémenter `Clone`
- [ ] Utiliser `Arc<T>` pour les ressources partagées
- [ ] Ajouter des méthodes helpers si nécessaire

### 4. Handlers HTTP

- [ ] Définir les fonctions handler async
- [ ] Utiliser les extracteurs Axum appropriés
- [ ] Gérer les erreurs avec `Result<T, StatusCode>`
- [ ] Ajouter des logs (info, warn, error)

### 5. Documentation OpenAPI (optionnel)

- [ ] Annoter les handlers avec `#[utoipa::path(...)]`
- [ ] Définir les schémas avec `#[derive(ToSchema)]`
- [ ] Créer une structure `#[derive(OpenApi)]`
- [ ] Inclure des exemples dans la documentation

### 6. Implémentation du trait

- [ ] Implémenter le trait pour `pmoserver::Server`
- [ ] Utiliser les méthodes du serveur pour enregistrer les routes
- [ ] Gérer les erreurs avec `anyhow::Result`
- [ ] Retourner les ressources créées si nécessaire

### 7. Tests

- [ ] Tester les handlers indépendamment
- [ ] Tester l'intégration avec le serveur
- [ ] Vérifier la documentation OpenAPI générée

## Bonnes pratiques

### 1. Gestion des erreurs

- Utiliser `anyhow::Result` pour les méthodes d'initialisation
- Utiliser `Result<T, StatusCode>` pour les handlers HTTP
- Toujours logger les erreurs avant de les retourner
- Préférer les codes HTTP sémantiques (404, 500, 504, etc.)

### 2. Performance

- Utiliser `spawn_blocking` pour les opérations synchrones
- Toujours ajouter des timeouts pour les opérations réseau
- Cloner l'état minimal nécessaire dans les closures
- Utiliser des `Arc<T>` plutôt que `Mutex<T>` quand possible

### 3. Concurrence

- Préférer `tokio::spawn` pour les tâches en arrière-plan
- Retourner immédiatement pour les opérations longues
- Utiliser des channels pour communiquer entre tâches
- Éviter les `RwLock` dans les handlers (préférer `spawn_blocking`)

### 4. Documentation

- Documenter chaque fonction publique avec des exemples
- Utiliser les annotations `utoipa` pour l'OpenAPI
- Inclure des exemples d'utilisation dans la doc du trait
- Documenter les routes HTTP créées

### 5. Features Cargo

- Toujours feature-gate les extensions avec `#[cfg(feature = "pmoserver")]`
- Déclarer les dépendances comme optionnelles
- Documenter les features dans le README de la crate

## Exemples d'utilisation

### Initialisation simple

```rust
use pmoserver::ServerBuilder;
use pmoaudiocache::AudioCacheExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut server = ServerBuilder::new("MyApp", "http://localhost", 8080)
        .build();

    // Initialiser le cache audio
    server.init_audio_cache("./cache", 500).await?;

    server.start().await;
    server.wait().await;
    Ok(())
}
```

### Initialisation avec configuration

```rust
use pmoserver::ServerBuilder;
use pmoaudiocache::AudioCacheExt;
use pmocovers::CoverCacheExt;
use pmoparadise::RadioParadiseExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut server = ServerBuilder::new("MyApp", "http://localhost", 8080)
        .build();

    // Initialiser plusieurs extensions
    server.init_audio_cache_configured().await?;
    server.init_cover_cache_configured().await?;
    server.init_radioparadise().await?;

    server.start().await;
    server.wait().await;
    Ok(())
}
```

### Initialisation avec état partagé

```rust
use pmoserver::ServerBuilder;
use pmomediaserver::ParadiseStreamingExt;
use pmocontrol::{ControlPoint, ControlPointState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut server = ServerBuilder::new("MyApp", "http://localhost", 8080)
        .build();

    // Initialiser Radio Paradise avec caches
    let manager = server.init_paradise_streaming().await?;

    // Créer et enregistrer le Control Point
    let control_point = Arc::new(ControlPoint::new());
    let state = ControlPointState::new(control_point);
    // ... enregistrer les routes du control point ...

    server.start().await;
    server.wait().await;
    Ok(())
}
```

## Références

### Fichiers sources analysés

- `pmoapp/src/lib.rs` : Pattern SPA avec RustEmbed
- `pmocontrol/src/pmoserver_ext.rs` : API REST complète avec Control Point
- `pmoparadise/src/pmoserver_ext.rs` : API REST avec client externe
- `pmoaudiocache/src/lib.rs` : Cache avec routes de fichiers
- `pmomediaserver/src/paradise_streaming.rs` : Extension complexe avec streaming

### Dépendances communes

- `axum` : Framework HTTP
- `async-trait` : Traits async
- `utoipa` : Documentation OpenAPI
- `tokio` : Runtime async
- `anyhow` : Gestion d'erreurs
- `serde` : Sérialisation JSON
- `tracing` : Logging

## Conclusion

Le pattern `pmoserver_ext` offre une architecture extensible et modulaire pour ajouter des fonctionnalités au serveur HTTP PMOMusic. En suivant les conventions établies et les bonnes pratiques, chaque nouvelle extension peut être développée indépendamment tout en s'intégrant de manière cohérente avec l'écosystème existant.
