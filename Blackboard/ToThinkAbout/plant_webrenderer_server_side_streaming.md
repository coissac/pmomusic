# Plan d'implémentation détaillé : WebRenderer streaming audio côté serveur

## Synthèse de l'exploration du code

### Architecture actuelle

L'architecture actuelle repose sur :

1. `pmowebrenderer/src/websocket.rs` - Handler WebSocket : crée ou reconnecte un device UPnP par navigateur, envoie des commandes JSON au navigateur
2. `pmowebrenderer/src/handlers.rs` - Action handlers UPnP qui forwardent vers le WebSocket via `SharedSender`
3. `pmowebrenderer/src/session.rs` - `SessionManager` : HashMap token → session, persistance UDN/sender entre reconnexions
4. `pmowebrenderer/src/state.rs` - `RendererState` + `SharedSender` (remplaçable à chaque reconnexion WS)
5. `pmowebrenderer/src/renderer.rs` - `WebRendererFactory` : construit les services UPnP (AVTransport, RenderingControl, ConnectionManager)
6. `pmowebrenderer/src/config.rs` - Trait `WebRendererExt` : enregistre la route WS dans pmoserver
7. Frontend : `useWebRenderer.ts` - composable Vue.js gérant WebSocket + `GaplessEngine` (deux `HTMLAudioElement` en ping-pong)

### Infrastructure réutilisable confirmée

- `StreamingFlacSink` + `StreamHandle` dans `pmoaudio-ext/src/sinks/streaming_flac_sink.rs` : broadcast multi-clients, gapless, backpressure, header FLAC caché pour late-joiners
- Pattern HTTP streaming dans `pmomediaserver/src/paradise_streaming.rs` : `Body::from_stream(ReaderStream::new(stream))` avec headers corrects
- `PlaylistSource` dans `pmoaudio-ext/src/sources/playlist_source.rs` : modèle pour `source_loader.rs`, incluant décodage FLAC via `pmoflac::decode_audio_stream`, gestion cache progressif
- `pmoserver::Server` : API `add_handler_with_state`, `add_post_handler_with_state`, `add_any_handler_with_state`, `add_router`

### Dépendances existantes de pmowebrenderer

Le crate actuel ne dépend pas de `pmoaudio-ext`, `pmoaudio`, `pmoflac` ou `pmoaudiocache`. Il faudra les ajouter.

---

## Ordre d'implémentation

Les étapes sont organisées de façon à avoir un système compilable et testable à chaque jalon, en allant des fondations vers l'intégration.

---

## Etape 1 : Préparer le `Cargo.toml` de pmowebrenderer

**Fichier concerné :** `pmowebrenderer/Cargo.toml`

**Modifications :**
```toml
[dependencies]
# Existants conservés
pmoupnp = { path = "../pmoupnp" }
pmomediarenderer = { path = "../pmomediarenderer" }
pmoserver = { path = "../pmoserver", optional = true }
pmocontrol = { path = "../pmocontrol", optional = true }
pmoconfig = { path = "../pmoconfig" }

# Nouveaux : pipeline audio
pmoaudio-ext = { path = "../pmoaudio-ext", features = ["http-stream", "playlist"], optional = true }
pmoaudio = { path = "../pmoaudio", optional = true }
pmoflac = { path = "../pmoflac", optional = true }
pmoaudiocache = { path = "../pmoaudiocache", optional = true }
pmometadata = { path = "../pmometadata", optional = true }

# Async runtime
tokio = { workspace = true, features = ["full"] }
async-trait = { workspace = true }

# HTTP streaming (on retire "ws" de axum si WebSocket supprimé en phase finale)
axum = { workspace = true, features = ["ws"] }  # garder "ws" pendant la migration
axum-extra = { version = "0.9", features = ["typed-header"] }
tower-http = { version = "0.6", features = ["fs", "trace"] }
futures = "0.3"
bytes = "1.0"
tokio-util = { version = "0.7", features = ["io"] }

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }

# Utilities
uuid = { workspace = true, features = ["v4", "serde"] }
parking_lot = "0.12"
thiserror = { workspace = true }
tracing = { workspace = true }

pmodidl = { path = "../pmodidl" }
pmoutils = { path = "../pmoutils" }

[features]
default = []
pmoserver = ["dep:pmoserver", "dep:pmocontrol"]
audio-pipeline = ["dep:pmoaudio-ext", "dep:pmoaudio", "dep:pmoflac", "dep:pmoaudiocache", "dep:pmometadata"]
```

**Note :** Ajouter `pmoaudio-ext` dans le workspace `Cargo.toml` (il n'y est pas encore).

**Piège :** `pmoaudio-ext` n'est pas dans `[workspace.members]` du `Cargo.toml` racine. Il faut l'y ajouter. Vérifier aussi que `pmoaudio-ext` avec feature `playlist` n'introduit pas de dépendance circulaire via `pmoplaylist`.

---

## Etape 2 : Créer `pipeline.rs` — canal de contrôle et état par instance

**Fichier :** `pmowebrenderer/src/pipeline.rs` (nouveau)

Ce module définit les types partagés entre tous les handlers sans logique de pipeline encore.

```rust
use tokio::sync::mpsc;

/// Commandes envoyées au pipeline audio de l'instance
#[derive(Debug)]
pub enum PipelineControl {
    LoadUri(String),
    LoadNextUri(String),
    Play,
    Pause,
    Stop,
    Seek(f64),        // secondes
    SetVolume(u16),   // 0-100
    SetMute(bool),
}

/// Handle vers le pipeline audio d'une instance WebRenderer serveur
#[derive(Clone)]
pub struct PipelineHandle {
    /// Canal de contrôle vers la task pipeline
    pub control_tx: mpsc::Sender<PipelineControl>,
    /// Token d'annulation pour stopper le pipeline
    pub stop_token: tokio_util::sync::CancellationToken,
}

impl PipelineHandle {
    pub async fn send(&self, cmd: PipelineControl) {
        let _ = self.control_tx.send(cmd).await;
    }
}
```

**Points d'attention :**
- `PipelineControl` doit être `Send` (pas de `Arc<RwLock<...>>` dans les variantes).
- Le `CancellationToken` de `tokio_util` est déjà utilisé dans `pmoaudio-ext`, importer depuis là.

---

## Etape 3 : Modifier `state.rs` — ajouter stream handle et pipeline handle

**Fichier :** `pmowebrenderer/src/state.rs` (modifier)

Remplacer `SharedSender` (qui envoie vers le WS) par un `SharedStreamHandle` et un `PipelineHandle`.

```rust
use parking_lot::RwLock;
use std::sync::Arc;

#[cfg(feature = "audio-pipeline")]
use pmoaudio_ext::sinks::streaming_flac_sink::StreamHandle;

use crate::messages::PlaybackState;
#[cfg(feature = "audio-pipeline")]
use crate::pipeline::PipelineHandle;

/// État temps-réel du renderer (partagé backend ↔ pipeline)
#[derive(Debug, Clone)]
pub struct RendererState {
    pub playback_state: PlaybackState,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub next_uri: Option<String>,
    pub next_metadata: Option<String>,
    pub position: Option<String>,   // mis à jour par la task de suivi position
    pub duration: Option<String>,
    pub volume: u16,
    pub mute: bool,
}

impl Default for RendererState { /* identique à l'existant */ }

pub type SharedState = Arc<RwLock<RendererState>>;

/// Handle vers le flux FLAC HTTP d'une instance (remplace SharedSender)
#[cfg(feature = "audio-pipeline")]
#[derive(Clone)]
pub struct SharedStreamHandle(Arc<RwLock<Option<StreamHandle>>>);

#[cfg(feature = "audio-pipeline")]
impl SharedStreamHandle {
    pub fn new(handle: StreamHandle) -> Self {
        Self(Arc::new(RwLock::new(Some(handle))))
    }

    pub fn get(&self) -> Option<StreamHandle> {
        self.0.read().clone()
    }

    pub fn clear(&self) {
        *self.0.write() = None;
    }
}

// Pour la compatibilité avec les handlers qui utilisent encore SharedSender
// pendant la phase de migration, on conserve SharedSender dans le module
// mais on la rend conditionnelle au feature "pmoserver" seul (sans audio-pipeline).
```

**Points d'attention :**
- Conserver `SharedSender` derrière une feature flag pendant la migration. Les handlers UPnP existants (`handlers.rs`) l'utilisent encore.
- La `StreamHandle` de `pmoaudio-ext` est `Clone` — on peut la partager sans `Arc<RwLock<...>>` en réalité, mais un wrapper permet de la remplacer à la reconnexion.

---

## Etape 4 : Créer `source_loader.rs` — ouverture d'URI arbitraires

**Fichier :** `pmowebrenderer/src/source_loader.rs` (nouveau)

Ce module sait ouvrir une URI quelconque (URL HTTP, chemin fichier local, chemin Samba) et la transformer en `AsyncRead` de bytes PCM via `pmoflac::decode_audio_stream`.

```rust
use pmoflac::decode_audio_stream;
use std::path::Path;
use tokio::io::AsyncRead;

pub enum SourceKind {
    LocalFile(std::path::PathBuf),
    HttpUrl(String),
}

pub fn classify_uri(uri: &str) -> SourceKind {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        SourceKind::HttpUrl(uri.to_string())
    } else {
        // Chemin fichier (absolu ou Samba monté)
        SourceKind::LocalFile(std::path::PathBuf::from(uri))
    }
}

/// Ouvre une source audio quelconque et retourne le stream PCM décodé.
/// Retourne (stream, sample_rate, bits_per_sample, channels)
pub async fn open_uri(
    uri: &str,
) -> Result<(impl AsyncRead + Send + Unpin, pmoflac::StreamInfo), SourceError> {
    match classify_uri(uri) {
        SourceKind::LocalFile(path) => {
            let file = tokio::fs::File::open(&path).await
                .map_err(|e| SourceError::Io(e.to_string()))?;
            let stream = decode_audio_stream(file).await
                .map_err(|e| SourceError::Decode(e.to_string()))?;
            let info = stream.info().clone();
            Ok((stream, info))
        }
        SourceKind::HttpUrl(url) => {
            // Utiliser reqwest pour streamer l'URL HTTP externe
            // Même approche que pmoaudiocache qui télécharge depuis URLs Qobuz
            let response = reqwest::get(&url).await
                .map_err(|e| SourceError::Http(e.to_string()))?;
            let byte_stream = response.bytes_stream();
            // Convertir en AsyncRead
            use tokio_util::io::StreamReader;
            use futures::TryStreamExt;
            let reader = StreamReader::new(
                byte_stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            );
            let stream = decode_audio_stream(reader).await
                .map_err(|e| SourceError::Decode(e.to_string()))?;
            let info = stream.info().clone();
            Ok((stream, info))
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Decode error: {0}")]
    Decode(String),
    #[error("HTTP error: {0}")]
    Http(String),
}
```

**Points d'attention :**
- `decode_audio_stream` prend un `AsyncRead + Send + Unpin`. `tokio::fs::File` et `StreamReader` satisfont ces contraintes.
- Pour les URLs Qobuz (URLs signées avec expiration), la source sera ouverte immédiatement par le handler `SetAVTransportURI` — pas de retry automatique. Si l'URL expire pendant la lecture, le pipeline s'arrêtera proprement via `StopReason`.
- Les chemins Samba supposent que le partage est monté localement sur le serveur. Aucun traitement spécial nécessaire, `tokio::fs::File::open` suffit.
- Ajouter `reqwest` en dépendance de pmowebrenderer avec feature `stream`.

---

## Etape 5 : Créer `pipeline.rs` — logique complète du pipeline audio

**Fichier :** `pmowebrenderer/src/pipeline.rs` (compléter l'étape 2)

La task pipeline tourne en background pour chaque instance. Elle reçoit des `PipelineControl`, ouvre les sources via `source_loader`, alimente la `StreamingFlacSink`.

```rust
use pmoaudio::{
    AudioChunk, AudioChunkData, AudioError, AudioSegment, _AudioSegment,
    pipeline::{AudioPipelineNode, PipelineHandle as AudioPipelineHandle},
};
use pmoaudio_ext::sinks::streaming_flac_sink::{StreamHandle, StreamingFlacSink};
use pmoflac::EncoderOptions;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct InstancePipeline {
    pub stream_handle: StreamHandle,
    pub control_tx: mpsc::Sender<PipelineControl>,
    pub stop_token: CancellationToken,
}

impl InstancePipeline {
    /// Crée et démarre un pipeline pour une instance WebRenderer.
    /// Retourne immédiatement ; la task tourne en background.
    pub fn start() -> Self {
        let stop_token = CancellationToken::new();
        let (control_tx, mut control_rx) = mpsc::channel::<PipelineControl>(16);
        let (sink, stream_handle) = StreamingFlacSink::new(
            EncoderOptions::default(),
            24, // bits_per_sample — 24 bits pour haute qualité
        );

        // La task de pipeline gère le cycle de vie
        let stop_token_clone = stop_token.clone();
        let sink_arc: Arc<Mutex<Option<Box<dyn AudioPipelineNode>>>> =
            Arc::new(Mutex::new(None));

        tokio::spawn(pipeline_task(
            control_rx,
            stop_token_clone,
            sink,
        ));

        Self {
            stream_handle,
            control_tx,
            stop_token,
        }
    }
}

async fn pipeline_task(
    mut control_rx: mpsc::Receiver<PipelineControl>,
    stop_token: CancellationToken,
    sink: StreamingFlacSink,
) {
    // État interne de la task
    let mut current_pipeline_handle: Option<AudioPipelineHandle> = None;
    let mut pending_next_uri: Option<String> = None;

    // La StreamingFlacSink est démarrée une fois, tourne en continu.
    // Le canal PCM (sink.get_tx()) reçoit les AudioSegments.
    // Pour l'alimenter, on lance une task source séparée par piste.

    let sink_tx = sink.get_tx().expect("StreamingFlacSink must have a tx");
    let sink_stop = stop_token.clone();
    tokio::spawn(async move {
        Box::new(sink).run(sink_stop).await.ok();
    });

    loop {
        tokio::select! {
            _ = stop_token.cancelled() => {
                // Stopper la source courante
                if let Some(h) = current_pipeline_handle.take() {
                    h.stop();
                }
                break;
            }

            cmd = control_rx.recv() => {
                match cmd {
                    None => break,
                    Some(PipelineControl::LoadUri(uri)) => {
                        // Stopper la source courante si elle existe
                        if let Some(h) = current_pipeline_handle.take() {
                            h.stop();
                        }
                        // Lancer une nouvelle source
                        current_pipeline_handle = Some(
                            spawn_source_task(uri, sink_tx.clone(), stop_token.clone()).await
                        );
                    }
                    Some(PipelineControl::LoadNextUri(uri)) => {
                        pending_next_uri = Some(uri);
                    }
                    Some(PipelineControl::Play) => {
                        // Pipeline serveur : Play ne fait rien de spécial
                        // La source alimente automatiquement dès LoadUri
                    }
                    Some(PipelineControl::Stop) => {
                        if let Some(h) = current_pipeline_handle.take() {
                            h.stop();
                        }
                        // Envoyer EndOfStream au sink
                        let _ = sink_tx.send(Arc::new(AudioSegment::new_end_of_stream(0, 0.0))).await;
                    }
                    Some(PipelineControl::Seek(pos_sec)) => {
                        // Pour seek : stopper la source, relancer depuis la position
                        if let Some(current_uri) = get_current_uri() {
                            if let Some(h) = current_pipeline_handle.take() {
                                h.stop();
                            }
                            current_pipeline_handle = Some(
                                spawn_source_task_from(current_uri, pos_sec, sink_tx.clone(), stop_token.clone()).await
                            );
                        }
                    }
                    Some(PipelineControl::SetVolume(vol)) => {
                        // TODO : Volume DSP côté serveur (hors scope initial)
                    }
                    Some(PipelineControl::SetMute(mute)) => {
                        // TODO
                    }
                    _ => {}
                }
            }
        }
    }
}
```

**Points d'attention critiques :**

1. **Gapless** : Le gapless avec `StreamingFlacSink` en mode `restart_encoder_on_track_boundary: false` signifie que le flux FLAC est continu. La frontière de piste est gérée par `SyncMarker::TrackBoundary`. Pour enchaîner deux sources indépendantes, il faut envoyer un `SyncMarker::TrackBoundary` entre les deux flux PCM vers le sink. La `StreamingFlacSink` ne restarte pas l'encodeur — le navigateur ne voit pas d'interruption.

2. **Architecture source** : La source doit envoyer des `Arc<AudioSegment>` directement au `sink_tx` (le canal d'entrée de `StreamingFlacSink`). Ce n'est pas la même architecture que `Node<PlaylistSourceLogic>` qui utilise les `children`. Ici, on alimente directement le sink via son `Sender<Arc<AudioSegment>>`. C'est exact car `sink.get_tx()` retourne le `Sender` d'entrée du `Node<StreamingFlacSinkLogic>`.

3. **Position tracking** : Sans retour du navigateur (le navigateur ne connaît que le flux FLAC), la position doit être trackée côté serveur. La `StreamingFlacSink` maintient un `current_timestamp` accessible via `StreamHandle` (indirectement via les métadonnées). Une task périodique lit ce timestamp et met à jour `RendererState.position`.

4. **Seek** : Un seek implique de relancer la source depuis une position donnée. Pour les fichiers FLAC locaux, `pmoflac::decode_audio_stream` ne supporte pas le seek natif sur un stream. Il faudra rouvrir le fichier et lire en avançant les frames. Approche pragmatique : seek = stop + reopen + skip samples (coûteux mais simple). Le navigateur rebuffère ~1s comme indiqué dans l'archi.

---

## Etape 6 : Créer `register.rs` — handlers POST /register et DELETE /{id}

**Fichier :** `pmowebrenderer/src/register.rs` (nouveau)

Ce module remplace `websocket.rs`. La logique de création du device UPnP est ici, adaptée du code de `create_renderer_for_browser` dans `websocket.rs`.

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::registry::RendererRegistry;
use crate::pipeline::InstancePipeline;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub instance_id: String,
    pub user_agent: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub stream_url: String,
}

/// POST /api/webrenderer/register
pub async fn register_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    match registry.register_or_reconnect(&req.instance_id, &req.user_agent).await {
        Ok(stream_url) => (StatusCode::OK, Json(RegisterResponse { stream_url })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// DELETE /api/webrenderer/{id}
pub async fn unregister_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    registry.unregister(&instance_id).await;
    StatusCode::NO_CONTENT
}
```

**Points d'attention :**
- La logique de `create_renderer_for_browser` dans `websocket.rs` gère 4 cas (reconnexion session active, device dans registry mais session expirée, première connexion, etc.). Cette logique doit être reprise quasi à l'identique dans `RendererRegistry::register_or_reconnect`.
- La `stream_url` retournée est `/api/webrenderer/{instance_id}/stream` — URL relative, correcte en local et via proxy.
- Plus de `SharedSender` dans `WebRendererSession`. À la place : `stream_handle: SharedStreamHandle` + `pipeline: PipelineHandle`.

---

## Etape 7 : Créer `stream.rs` — handler HTTP GET /stream

**Fichier :** `pmowebrenderer/src/stream.rs` (nouveau)

Ce module suit exactement le pattern de `pmomediaserver/src/paradise_streaming.rs`.

```rust
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        StatusCode,
        header::{ACCEPT_RANGES, CACHE_CONTROL, CONNECTION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;

use crate::registry::RendererRegistry;

/// GET /api/webrenderer/{id}/stream
///
/// Retourne le flux FLAC continu de l'instance.
/// La déconnexion du client est détectée automatiquement par la coupure du flux.
pub async fn stream_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let handle = match registry.get_stream_handle(&instance_id).await {
        Some(h) => h,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // Abonner ce client au flux FLAC
    let flac_stream = handle.subscribe_flac();

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "audio/flac")
        .header(CACHE_CONTROL, "no-store, no-transform")
        .header(CONNECTION, "keep-alive")
        .header(ACCEPT_RANGES, "none")
        .body(Body::from_stream(ReaderStream::new(flac_stream)))
        .unwrap()
        .into_response()
}
```

**Points d'attention :**
- `ReaderStream` de `tokio-util` convertit un `AsyncRead` en `Stream<Item=Result<Bytes>>`. `FlacClientStream` implémente `AsyncRead`. Le pattern est identique à `paradise_streaming.rs`.
- La détection de déconnexion se fait via `FlacClientStream::Drop` qui décrémente le compteur de clients dans `StreamHandle::client_disconnected()`. La `StreamingFlacSink` peut être configurée avec `set_auto_stop(true)` pour stopper le pipeline si plus aucun client n'écoute.
- Un client qui se reconnecte (`reload` de page) reçoit d'abord le header FLAC caché dans `SharedStreamHandleInner::header`, puis la suite du flux courant via `register_client()`. Ce mécanisme est déjà dans `SharedStreamHandleInner` — vérifier que `subscribe_flac()` envoie bien le header en premier (c'est le cas dans l'implémentation actuelle via `header_cache`).

---

## Etape 8 : Créer `registry.rs` — remplace `session.rs`

**Fichier :** `pmowebrenderer/src/registry.rs` (nouveau)

Le `RendererRegistry` remplace `SessionManager`. La session est maintenant liée au flux FLAC, pas au WebSocket.

```rust
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use pmoupnp::devices::DeviceInstance;

#[cfg(feature = "audio-pipeline")]
use crate::pipeline::{InstancePipeline, PipelineHandle};
#[cfg(feature = "audio-pipeline")]
use crate::state::SharedStreamHandle;
use crate::state::SharedState;

/// Instance WebRenderer côté serveur
pub struct WebRendererInstance {
    pub instance_id: String,       // UUID stable du navigateur (localStorage)
    pub udn: String,               // "uuid:{instance_id}"
    pub device_instance: Arc<DeviceInstance>,
    pub state: SharedState,
    #[cfg(feature = "audio-pipeline")]
    pub stream_handle: SharedStreamHandle,
    #[cfg(feature = "audio-pipeline")]
    pub pipeline: PipelineHandle,
    pub created_at: SystemTime,
    pub last_stream_connect: Arc<RwLock<Option<SystemTime>>>,
}

/// Registre global des instances WebRenderer actives
pub struct RendererRegistry {
    instances: Arc<RwLock<HashMap<String, Arc<WebRendererInstance>>>>,
    /// Map UDN → instance, pour retrouver par UDN depuis les handlers UPnP
    by_udn: Arc<RwLock<HashMap<String, Arc<WebRendererInstance>>>>,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<pmocontrol::ControlPoint>,
}

impl RendererRegistry {
    /// Enregistre ou reconnecte une instance.
    /// Retourne l'URL de stream relative.
    pub async fn register_or_reconnect(
        &self,
        instance_id: &str,
        user_agent: &str,
    ) -> Result<String, crate::error::WebRendererError> {
        // Vérifier si une instance existe déjà pour cet instance_id
        {
            let instances = self.instances.read();
            if let Some(existing) = instances.get(instance_id) {
                // Reconnexion : l'instance existe, le pipeline tourne toujours
                // Réannoncer auprès du ControlPoint
                #[cfg(feature = "pmoserver")]
                self.register_with_control_point(&existing.device_instance)?;

                tracing::info!(instance_id, "WebRenderer: reconnected");
                return Ok(format!("/api/webrenderer/{}/stream", instance_id));
            }
        }

        // Première connexion : créer device UPnP + pipeline
        // (Reprend la logique de create_renderer_for_browser dans websocket.rs)
        let instance = self.create_instance(instance_id, user_agent).await?;
        let stream_url = format!("/api/webrenderer/{}/stream", instance_id);

        let instance = Arc::new(instance);
        {
            let mut instances = self.instances.write();
            instances.insert(instance_id.to_string(), instance.clone());
        }
        {
            let mut by_udn = self.by_udn.write();
            by_udn.insert(instance.udn.clone(), instance.clone());
        }

        tracing::info!(instance_id, "WebRenderer: registered new instance");
        Ok(stream_url)
    }

    /// Retourne le StreamHandle pour l'endpoint /stream
    pub async fn get_stream_handle(&self, instance_id: &str) -> Option<StreamHandle> {
        self.instances.read()
            .get(instance_id)
            .and_then(|i| i.stream_handle.get())
    }

    /// Retourne le PipelineHandle pour les handlers UPnP
    pub fn get_pipeline_by_udn(&self, udn: &str) -> Option<PipelineHandle> {
        self.by_udn.read()
            .get(udn)
            .map(|i| i.pipeline.clone())
    }

    pub fn get_state_by_udn(&self, udn: &str) -> Option<SharedState> {
        self.by_udn.read()
            .get(udn)
            .map(|i| i.state.clone())
    }

    pub async fn unregister(&self, instance_id: &str) {
        if let Some(instance) = self.instances.write().remove(instance_id) {
            self.by_udn.write().remove(&instance.udn);
            // Stopper le pipeline
            instance.pipeline.stop_token.cancel();
            // Annoncer SSDP byebye
            #[cfg(feature = "pmoserver")]
            if let Ok(mut registry) = self.control_point.registry().write() {
                registry.device_says_byebye(&instance.udn);
            }
            tracing::info!(instance_id, "WebRenderer: unregistered");
        }
    }
}
```

**Points d'attention :**
- La méthode `create_instance` reprend quasi mot pour mot la logique de `create_renderer_for_browser` dans `websocket.rs` (vérifier registry DEVICE_REGISTRY, créer device model, `server.register_device`, `register_with_control_point`), mais au lieu de créer `SharedSender`, elle crée `InstancePipeline::start()`.
- Le cleanup "session expirée" de l'ancien `SessionManager` est remplacé par la détection de déconnexion du flux FLAC : quand `FlacClientStream` est droppé, `client_disconnected()` est appelé. Si `auto_stop` est activé, le pipeline s'arrête.
- Cleanup proactif via une task de polling : si le dernier client FLAC s'est déconnecté depuis plus de 5 minutes et que le ControlPoint indique que le renderer n'est plus en usage, on peut unregister.

---

## Etape 9 : Modifier `handlers.rs` — brancher sur le pipeline au lieu du WS

**Fichier :** `pmowebrenderer/src/handlers.rs` (modifier)

Les handlers UPnP ne font plus `ws.send(...)` mais `pipeline.send(PipelineControl::...)`.

```rust
// Nouveau play_handler
pub fn play_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Play).await;
            state.write().playback_state = PlaybackState::Playing;
            Ok(data)
        })
    })
}

// Nouveau set_uri_handler — la différence clé
pub fn set_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let uri: String = get!(&data, "CurrentURI", String);
            let metadata: String = /* ... identique à l'existant ... */;

            // Envoyer au pipeline serveur (plus de WebSocket)
            pipeline.send(PipelineControl::LoadUri(uri.clone())).await;

            {
                let mut s = state.write();
                s.current_uri = Some(uri);
                s.current_metadata = Some(metadata);
                s.playback_state = PlaybackState::Transitioning;
            }
            Ok(data)
        })
    })
}

// set_next_uri_handler
pub fn set_next_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let uri: String = get!(&data, "NextURI", String);
            let metadata: String = /* ... */;
            pipeline.send(PipelineControl::LoadNextUri(uri.clone())).await;
            {
                let mut s = state.write();
                s.next_uri = Some(uri);
                s.next_metadata = Some(metadata);
            }
            Ok(data)
        })
    })
}

// seek_handler
pub fn seek_handler(pipeline: PipelineHandle) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        Box::pin(async move {
            let target: String = get!(&data, "Target", String);
            // Convertir "H:MM:SS" en secondes
            let pos_sec = upnp_time_to_seconds(&target);
            pipeline.send(PipelineControl::Seek(pos_sec)).await;
            Ok(data)
        })
    })
}

// Volume et Mute : côté serveur ou navigateur ?
// Phase initiale : volume = 100 fixe côté serveur, navigateur gère avec l'élément <audio>
// Donc set_volume_handler et set_mute_handler mettent à jour l'état mais n'envoient
// pas encore au pipeline (pas de DSP volume implémenté).
pub fn set_volume_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let volume: u16 = get!(&data, "DesiredVolume", u16);
            pipeline.send(PipelineControl::SetVolume(volume)).await;
            state.write().volume = volume;
            Ok(data)
        })
    })
}
```

**Points d'attention :**
- Supprimer toutes les références à `SharedSender` et `ServerMessage` dans `handlers.rs`.
- La conversion "H:MM:SS" → secondes était côté navigateur (dans `GaplessEngine::seek`). Il faut la déplacer côté serveur dans `handlers.rs`.
- `get_position_info_handler` et `get_transport_info_handler` restent inchangés (lisent `SharedState`).

---

## Etape 10 : Modifier `renderer.rs` — `WebRendererFactory` avec pipeline

**Fichier :** `pmowebrenderer/src/renderer.rs` (modifier)

Remplacer la signature de `create_device_with_name` : au lieu de prendre un `ws_sender`, prendre un `PipelineHandle`.

```rust
impl WebRendererFactory {
    pub fn create_device_with_pipeline(
        device_name: &str,
        browser_ua: &str,
        pipeline: PipelineHandle,
        state: SharedState,
    ) -> Result<Device, FactoryError> {
        let avtransport = Self::build_avtransport(pipeline.clone(), state.clone())?;
        let renderingcontrol = Self::build_renderingcontrol(pipeline.clone(), state.clone())?;
        let connectionmanager = Self::build_connectionmanager()?;

        let short_name = extract_browser_name(browser_ua);
        let device = Device::new(
            device_name.to_string(),
            "MediaRenderer".to_string(),
            format!("Web Audio – {}", short_name),
        );
        // ...
        Ok(device)
    }
}
```

**Points d'attention :**
- `build_avtransport` et `build_renderingcontrol` prennent maintenant `PipelineHandle` au lieu de `SharedSender`.
- Le `ConnectionManager` déclarait des formats supportés dans `get_protocol_info_handler`. Mettre à jour pour n'annoncer que `audio/flac` puisque le navigateur ne gère plus que du FLAC serveur-side.

---

## Etape 11 : Modifier `config.rs` — nouvelles routes, supprimer WS

**Fichier :** `pmowebrenderer/src/config.rs` (modifier)

Remplacer l'enregistrement de la route WebSocket par les trois nouvelles routes.

```rust
#[async_trait]
impl WebRendererExt for pmoserver::Server {
    async fn register_web_renderer(
        &mut self,
        control_point: Arc<ControlPoint>,
    ) -> Result<(), WebRendererError> {
        let registry = Arc::new(RendererRegistry::new(control_point));

        // POST /api/webrenderer/register
        self.add_post_handler_with_state(
            "/api/webrenderer/register",
            register_handler,
            registry.clone(),
        ).await;

        // GET /api/webrenderer/{id}/stream  (sous-router pour param dynamique)
        let stream_router = Router::new()
            .route("/{id}/stream", get(stream_handler))
            .with_state(registry.clone());
        self.add_router("/api/webrenderer", stream_router).await;

        // DELETE /api/webrenderer/{id}
        let delete_router = Router::new()
            .route("/{id}", axum::routing::delete(unregister_handler))
            .with_state(registry.clone());
        self.add_router("/api/webrenderer", delete_router).await;

        tracing::info!("WebRenderer server-side streaming endpoints registered");
        Ok(())
    }
}
```

**Points d'attention :**
- `add_post_handler_with_state` existe dans `pmoserver::Server` (vu dans `server.rs`).
- Pour les routes avec paramètres dynamiques (`/{id}/stream`), utiliser `add_router` avec un sous-router Axum, car `add_handler_with_state` utilise `Router::route("/", ...)` qui ne supporte pas les paramètres.
- Supprimer l'import de `websocket_handler` et `WebSocketState`.

---

## Etape 12 : Modifier `lib.rs` — mettre à jour les exports

**Fichier :** `pmowebrenderer/src/lib.rs` (modifier)

```rust
mod error;
mod handlers;
mod pipeline;
mod registry;
mod renderer;
mod source_loader;
mod state;
mod stream;
mod register;

#[cfg(feature = "pmoserver")]
mod config;

// Supprimer :
// mod messages;  (ou garder pour PlaybackState seulement)
// mod session;
// mod websocket;

pub use error::WebRendererError;
pub use pipeline::PipelineControl;
pub use renderer::WebRendererFactory;
pub use registry::{RendererRegistry, WebRendererInstance};
pub use state::{RendererState, SharedState};

// Garder PlaybackState depuis messages.rs si d'autres crates en dépendent
// Sinon le déplacer dans state.rs
pub use messages::PlaybackState;

#[cfg(feature = "pmoserver")]
pub use config::WebRendererExt;
```

**Points d'attention :**
- Vérifier si `PlaybackState` est importé par d'autres crates depuis `pmowebrenderer`. Si oui, le conserver dans `messages.rs` ou le déplacer dans `state.rs` en mettant à jour les imports.
- `BrowserCapabilities`, `RendererInfo` ne sont plus nécessaires côté serveur mais peuvent être gardées si le frontend les utilise encore via une API dédiée.

---

## Etape 13 : Modifier le frontend — `useWebRenderer.ts`

**Fichier :** `pmoapp/webapp/src/composables/useWebRenderer.ts` (modifier)

Remplacer la connexion WebSocket et le `GaplessEngine` ping-pong par :
1. Un `POST /api/webrenderer/register` au montage
2. Un élément `<audio>` unique dont la `src` est la `stream_url`
3. Plus de gestion de commandes (tout est côté serveur)

```typescript
export function useWebRenderer() {
    const connected = ref(false);
    const rendererInfo = ref<RendererInfo | null>(null);
    const streamUrl = ref<string | null>(null);

    const INSTANCE_ID_KEY = "pmomusic_webrenderer_instance_id";

    function getOrCreateInstanceId(): string {
        // ... identique à l'existant ...
    }

    async function register() {
        const instanceId = getOrCreateInstanceId();

        try {
            const response = await fetch("/api/webrenderer/register", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    instance_id: instanceId,
                    user_agent: navigator.userAgent,
                }),
            });

            if (!response.ok) throw new Error(`HTTP ${response.status}`);

            const data = await response.json() as { stream_url: string };
            streamUrl.value = data.stream_url;
            connected.value = true;

            // Ici on attend que le SSE global notifie l'apparition du renderer
            // (le ControlPoint émettra un event SSE "renderer_added")
            rendererInfo.value = {
                udn: `uuid:${instanceId.toLowerCase()}`,
                // friendly_name récupéré depuis le SSE ou une API
            };
            onConnectedCallback?.();

        } catch (e) {
            console.error("[WebRenderer] Failed to register:", e);
        }
    }

    async function unregister() {
        const instanceId = getOrCreateInstanceId();
        try {
            await fetch(`/api/webrenderer/${instanceId}`, { method: "DELETE" });
        } catch { /* ignore */ }
        connected.value = false;
        streamUrl.value = null;
    }

    onMounted(async () => {
        await register();
        window.addEventListener("beforeunload", () => void unregister());
    });

    onUnmounted(() => void unregister());

    return {
        connected: readonly(connected),
        rendererInfo: readonly(rendererInfo),
        streamUrl: readonly(streamUrl),  // exposé pour le template <audio :src="streamUrl">
        onConnected(fn: () => void) { onConnectedCallback = fn; },
    };
}
```

**Points d'attention :**
- L'élément `<audio :src="streamUrl">` doit être dans le template du composant qui utilise `useWebRenderer` (probablement `UnifiedControlView.vue`). La `src` pointe vers `/api/webrenderer/{instance_id}/stream`.
- Le navigateur doit démarrer la lecture automatiquement quand il reçoit le flux FLAC. Cependant, l'autoplay est bloqué sans interaction utilisateur. Solution : afficher un bouton "Activer l'audio" qui déclenche `audioElement.play()` au premier clic.
- La position, durée, titre/artiste ne viennent plus du navigateur. Ils viennent du SSE existant du ControlPoint. Vérifier que les événements SSE `state_changed`, `position_changed`, `metadata_changed` sont bien émis par le serveur lors des transitions de piste.
- Supprimer `GaplessEngine`, la classe TypeScript entière et ses états internes.
- La détection de fermeture de page (`beforeunload`) déclenche un `DELETE` qui est synchrone ou en best-effort. Utiliser `navigator.sendBeacon` pour garantir l'envoi.

---

## Etape 14 : Modifier `UnifiedControlView.vue` — intégrer le flux audio

**Fichier :** `pmoapp/webapp/src/views/UnifiedControlView.vue` (modifier)

Ajouter l'élément `<audio>` au template et connecter `streamUrl`.

```vue
<template>
  <!-- ... template existant ... -->

  <!-- Élément audio caché pour le flux WebRenderer serveur -->
  <audio
    v-if="webRenderer.streamUrl.value"
    ref="audioElement"
    :src="webRenderer.streamUrl.value"
    preload="none"
    style="display: none"
  />

  <!-- Bouton d'activation si autoplay bloqué -->
  <button
    v-if="webRenderer.connected.value && !audioStarted"
    @click="startAudio"
  >
    Activer l'audio
  </button>
</template>

<script setup lang="ts">
// ...
const audioElement = ref<HTMLAudioElement | null>(null);
const audioStarted = ref(false);

async function startAudio() {
    if (audioElement.value) {
        try {
            await audioElement.value.play();
            audioStarted.value = true;
        } catch (e) {
            console.warn("Cannot play audio:", e);
        }
    }
}
</script>
```

**Points d'attention :**
- L'attribut `preload="none"` évite de démarrer le buffering avant que l'utilisateur ait interagi. Changer en `preload="auto"` après interaction initiale.
- Une seule connexion au flux (un seul `<audio>`) par onglet suffit. Pas de ping-pong nécessaire.

---

## Etape 15 : Tâche de suivi de position côté serveur

**Fichier :** `pmowebrenderer/src/registry.rs` ou `pipeline.rs` (modifier)

Ajouter une task background qui lit la position depuis la `StreamingFlacSink` et met à jour `RendererState.position` et les variables UPnP.

```rust
/// Spawn une task qui met à jour la position dans l'état partagé toutes les secondes.
fn spawn_position_tracker(
    stream_handle: StreamHandle,
    device_instance: Arc<DeviceInstance>,
    state: SharedState,
    stop_token: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = stop_token.cancelled() => break,
                _ = interval.tick() => {
                    // La StreamHandle expose get_metadata() qui contient audio_timestamp_sec
                    let meta = stream_handle.get_metadata().await;
                    let pos_sec = meta.audio_timestamp_sec;
                    let pos_str = seconds_to_upnp_time(pos_sec);

                    {
                        let mut s = state.write();
                        s.position = Some(pos_str.clone());
                        // duration reste celle du dernier TrackBoundary metadata
                    }

                    // Mettre à jour les variables UPnP pour que GetPositionInfo retourne les bonnes valeurs
                    update_position_vars(&device_instance, &pos_str, "").await;
                }
            }
        }
    });
}
```

**Points d'attention :**
- `MetadataSnapshot::audio_timestamp_sec` est mis à jour par la `StreamingFlacSink` via `update_metadata`. Vérifier que ce champ correspond bien à la position de lecture (timestamp courant du broadcaster).
- La durée d'une piste n'est connue qu'à partir des métadonnées FLAC (STREAMINFO). Après `open_uri`, `stream.info().total_samples` divisé par `sample_rate` donne la durée. Stocker cette info dans `RendererState.duration` lors du `LoadUri`.

---

## Etape 16 : Gestion du TrackEnded côté serveur

Dans l'ancienne architecture, le navigateur envoyait `TrackEnded` quand une piste se terminait, ce qui déclenchait `advance_queue_and_prefetch`. Maintenant, c'est le pipeline qui détecte la fin d'une source.

**Dans `pipeline_task`**, quand la source courante se termine naturellement (EOF sans interruption) :

```rust
// Quand la source termine (EOF détecté dans spawn_source_task)
let (next_uri, next_metadata) = {
    let mut s = state.write();
    let uri = s.next_uri.take();
    let meta = s.next_metadata.take();
    s.current_uri = uri.clone();
    s.current_metadata = meta.clone();
    s.next_uri = None;
    s.next_metadata = None;
    if uri.is_some() {
        s.playback_state = PlaybackState::Playing;
    } else {
        s.playback_state = PlaybackState::Stopped;
    }
    (uri, meta)
};

// Mettre à jour les variables UPnP
update_uri_vars(&device_instance, ...).await;
update_transport_state_var(&device_instance, &new_state).await;

// Notifier le ControlPoint (advance_queue_and_prefetch)
if let Some(cp) = control_point.as_ref() {
    let udn = device_instance.udn().to_string();
    let cp = cp.clone();
    tokio::spawn(async move {
        cp.advance_queue_and_prefetch(&DeviceId(udn));
    });
}

// Si next_uri existe, lancer automatiquement la piste suivante
if let Some(uri) = next_uri {
    spawn_source_task(uri, sink_tx.clone(), stop_token.clone()).await;
}
```

**Points d'attention :**
- La logique de `TrackEnded` dans `websocket.rs` (lignes 195-256) doit être reprise presque à l'identique dans la task pipeline.
- Le `pipeline_task` a besoin d'accéder à `device_instance` et `control_point` — les passer en paramètre à la création ou via un `Arc<WebRendererInstance>`.

---

## Etape 17 : Cleanup — supprimer websocket.rs et messages.rs

**Fichiers à supprimer :** `pmowebrenderer/src/websocket.rs`, `pmowebrenderer/src/messages.rs`

**Attention :** `messages.rs` contient `PlaybackState` qui est utilisé dans `state.rs`, `handlers.rs`, etc. Avant de supprimer, déplacer `PlaybackState` dans `state.rs` ou créer un `types.rs`.

La suppression se fait en dernier, après que tout le reste compile.

---

## Récapitulatif de l'ordre d'implémentation

| Etape | Fichier | Action |
|-------|---------|--------|
| 1 | `pmowebrenderer/Cargo.toml` | Ajouter dépendances audio |
| 1 | Workspace `Cargo.toml` | Ajouter `pmoaudio-ext` aux membres |
| 2-3 | `pipeline.rs` (squelette) + `state.rs` | Types PipelineControl, SharedStreamHandle |
| 4 | `source_loader.rs` | Ouverture URI arbitraires |
| 5 | `pipeline.rs` (complet) | Task pipeline avec spawn_source_task |
| 6 | `register.rs` | Handlers POST/DELETE |
| 7 | `stream.rs` | Handler GET /stream |
| 8 | `registry.rs` | Remplace session.rs |
| 9 | `handlers.rs` | Brancher sur PipelineHandle |
| 10 | `renderer.rs` | Factory avec PipelineHandle |
| 11 | `config.rs` | Nouvelles routes, supprimer WS |
| 12 | `lib.rs` | Mettre à jour exports |
| 13 | `useWebRenderer.ts` | POST register + audio element |
| 14 | `UnifiedControlView.vue` | Intégrer audio element |
| 15 | `registry.rs` ou `pipeline.rs` | Position tracker task |
| 16 | `pipeline.rs` | TrackEnded côté serveur |
| 17 | `websocket.rs`, `messages.rs` | Supprimer |

---

## Points d'attention généraux (pièges à éviter)

### Pièges Rust

1. **Dépendances cycliques** : `pmoaudio-ext` avec feature `playlist` dépend de `pmoplaylist` → `pmoaudiocache`. S'assurer que `pmowebrenderer` ne crée pas de cycle. La feature `http-stream` seule suffit pour `StreamingFlacSink` sans dépendre de `pmoplaylist`.

2. **Lifetime du `StreamingFlacSink`** : La `StreamingFlacSink` doit vivre tant que des clients sont connectés. Elle ne peut pas être droppée tant que le `StreamHandle` existe. Stocker le `PipelineHandle` (contenant le `CancellationToken` et le `JoinHandle`) dans `WebRendererInstance` pour maintenir le pipeline en vie.

3. **`spawn_source_task` et formats audio hétérogènes** : `PlaylistSource` produit du PCM hétérogène. `StreamingFlacSink` attend un type cohérent. Si la source est un FLAC 24bits/44100Hz, le sink l'encodera en FLAC 24bits. Si une piste suivante est 16bits/48000Hz, il faudrait un `ResamplingNode`. Pour la phase initiale, documenter cette limitation et accepter que des pistes de formats différents peuvent causer des artefacts dans le flux continu. La `StreamingFlacSink` détecte le sample_rate au premier chunk — un changement de sample_rate en cours de flux n'est pas géré proprement.

4. **Seek pour les fichiers locaux** : `pmoflac::decode_audio_stream` retourne un stream séquentiel. Pour seek, ouvrir le fichier, décoder et dropper les frames jusqu'à la position cible. C'est O(n) mais acceptable pour les fichiers locaux. Pour les URLs HTTP (Qobuz), le seek implique une nouvelle requête HTTP avec `Range: bytes=N-` si le serveur le supporte, ou sinon un re-téléchargement depuis le début.

5. **`add_post_handler_with_state` vs `add_handler_with_state`** : Vérifier la signature exacte dans `pmoserver`. `add_post_handler_with_state` utilise `routing::post(handler)` et `add_handler_with_state` utilise `routing::get(handler)`. Il faut `post` pour le register.

### Pièges Frontend

6. **Autoplay policy** : Chrome/Firefox bloquent `audio.play()` sans interaction utilisateur. La solution la plus propre est de démarrer le flux seulement après un premier clic de l'utilisateur (bouton "Activer WebRenderer" ou premier clic sur Play).

7. **Reconnexion et header FLAC** : Si le navigateur recharge la page, la `stream_url` est la même. La `StreamingFlacSink` envoie le header FLAC caché aux late-joiners via `register_client()`. Tester que cette mécanique fonctionne correctement quand le pipeline est en cours de lecture.

8. **Suppression de `filterRenderers`** : La fonction `filterRenderers` dans `UnifiedControlView.vue` utilise `model_name === "WebRenderer"` et `renderer.id === myUdn`. Ces valeurs viennent de `RendererInfo` retourné par WebSocket. Dans la nouvelle architecture, le renderer UPnP est toujours créé avec `model_name = "WebRenderer"` (à conserver dans `WebRendererFactory`). L'UDN reste `uuid:{instance_id}`. Le filtre continuera de fonctionner si `rendererInfo.value.udn` est correctement alimenté depuis la réponse du `POST /register` ou du SSE.

---

### Critical Files for Implementation

- `/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmowebrenderer/src/websocket.rs` - Logique de création de device UPnP à reprendre dans `registry.rs` et `register.rs` ; contient aussi `register_with_control_point` et `update_*_vars` à migrer
- `/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoaudio-ext/src/sinks/streaming_flac_sink.rs` - Infrastructure principale réutilisée : `StreamingFlacSink::new()`, `StreamHandle::subscribe_flac()`, gestion header cache et auto_stop
- `/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoaudio-ext/src/sources/playlist_source.rs` - Modèle pour `source_loader.rs` : pattern `decode_and_emit_track`, gestion EOF, envoi `AudioSegment` vers children
- `/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmomediaserver/src/paradise_streaming.rs` - Pattern exact du handler HTTP streaming à copier dans `stream.rs` : headers, `Body::from_stream(ReaderStream::new(stream))`
- `/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoapp/webapp/src/composables/useWebRenderer.ts` - Toute la logique frontend à remplacer : `GaplessEngine`, WebSocket, `connect()`, `execCommand()`, `handleMessage()` — remplacer par `fetch POST /register` + `<audio src=streamUrl>`
