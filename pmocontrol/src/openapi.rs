//! Documentation OpenAPI et DTOs pour l'API ControlPoint
//!
//! Ce module fournit les types de réponse / payloads pour l'API REST du ControlPoint,
//! ainsi que la documentation OpenAPI via `utoipa`.

#[cfg(feature = "pmoserver")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "pmoserver")]
use utoipa::{OpenApi, ToSchema};

// ============================================================================
// RENDERERS
// ============================================================================

/// Résumé d'un renderer découvert
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RendererSummary {
    /// ID unique du renderer
    pub id: String,
    /// Nom convivial
    pub friendly_name: String,
    /// Modèle du renderer
    pub model_name: String,
    /// Protocole (UPnP pur, OpenHome pur, hybride)
    pub protocol: RendererProtocolSummary,
    /// Capacités détectées
    pub capabilities: RendererCapabilitiesSummary,
    /// Renderer en ligne
    pub online: bool,
}

/// Protocole exposé par le renderer
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RendererProtocolSummary {
    Upnp,
    Openhome,
    Hybrid,
    Chromecast,
}

/// Drapeaux de capacités renderer (transport, volume, services OpenHome, etc.)
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RendererCapabilitiesSummary {
    pub has_avtransport: bool,
    pub has_avtransport_set_next: bool,
    pub has_rendering_control: bool,
    pub has_connection_manager: bool,
    pub has_linkplay_http: bool,
    pub has_arylic_tcp: bool,
    pub has_oh_playlist: bool,
    pub has_oh_volume: bool,
    pub has_oh_info: bool,
    pub has_oh_time: bool,
    pub has_oh_radio: bool,
    pub has_chromecast: bool,
}

/// État détaillé d'un renderer
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RendererState {
    /// ID unique du renderer
    pub id: String,
    /// Nom convivial
    pub friendly_name: String,
    /// État de transport ("PLAYING", "PAUSED", "STOPPED", etc.)
    pub transport_state: String,
    /// Position courante en millisecondes
    pub position_ms: Option<u64>,
    /// Durée totale en millisecondes
    pub duration_ms: Option<u64>,
    /// Volume (0-100)
    pub volume: Option<u8>,
    /// Mute actif
    pub mute: Option<bool>,
    /// Nombre d'items dans la queue
    pub queue_len: usize,
    /// Playlist attachée (si applicable)
    pub attached_playlist: Option<AttachedPlaylistInfo>,
    /// Métadonnées du morceau courant (si en lecture)
    pub current_track: Option<CurrentTrackMetadata>,
}

/// Métadonnées du morceau en cours de lecture
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CurrentTrackMetadata {
    /// Titre du morceau
    pub title: Option<String>,
    /// Artiste
    pub artist: Option<String>,
    /// Album
    pub album: Option<String>,
    /// URI de la pochette d'album
    pub album_art_uri: Option<String>,
}

/// Information sur la playlist attachée
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AttachedPlaylistInfo {
    /// ID du serveur de médias
    pub server_id: String,
    /// ID du container playlist
    pub container_id: String,
    /// True si au moins une mise à jour a été vue
    pub has_seen_update: bool,
}

// ============================================================================
// QUEUE
// ============================================================================

/// Item de la queue de lecture
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QueueItem {
    /// Index dans la queue (0-based)
    pub index: usize,
    /// URI de la ressource
    pub uri: String,
    /// Titre du morceau
    pub title: Option<String>,
    /// Artiste
    pub artist: Option<String>,
    /// Album
    pub album: Option<String>,
    /// URI de la pochette d'album
    pub album_art_uri: Option<String>,
    /// ID du serveur source
    pub server_id: Option<String>,
    /// ID de l'objet DIDL-Lite
    pub object_id: Option<String>,
}

/// Snapshot de la queue d'un renderer
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QueueSnapshot {
    /// ID du renderer
    pub renderer_id: String,
    /// Items de la queue (playlist complète)
    pub items: Vec<QueueItem>,
    /// Index courant dans la playlist (None si rien n'est en cours)
    pub current_index: Option<usize>,
}

// ============================================================================
// MEDIA SERVERS
// ============================================================================

/// Résumé d'un serveur de médias découvert
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MediaServerSummary {
    /// ID unique du serveur
    pub id: String,
    /// Nom convivial
    pub friendly_name: String,
    /// Modèle du serveur
    pub model_name: String,
    /// Serveur en ligne
    pub online: bool,
}

/// Entrée de navigation (container ou item)
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContainerEntry {
    /// ID de l'objet
    pub id: String,
    /// Titre
    pub title: String,
    /// Classe UPnP (object.container.*, object.item.*, etc.)
    pub class: String,
    /// True si c'est un container (navigable)
    pub is_container: bool,
    /// Nombre d'enfants (si container)
    pub child_count: Option<u32>,
    /// Artiste (si item audio)
    pub artist: Option<String>,
    /// Album (si item audio)
    pub album: Option<String>,
    /// URI de la pochette d'album
    pub album_art_uri: Option<String>,
}

/// Résultat de navigation dans un container
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BrowseResponse {
    /// ID du container browsé
    pub container_id: String,
    /// Entrées du container
    pub entries: Vec<ContainerEntry>,
}

// ============================================================================
// SNAPSHOT AGGRÉGÉ
// ============================================================================

/// Alias de lisibilité pour les view-models déjà existants.
#[cfg(feature = "pmoserver")]
pub type RendererStateView = RendererState;
#[cfg(feature = "pmoserver")]
pub type QueueSnapshotView = QueueSnapshot;
#[cfg(feature = "pmoserver")]
pub type RendererBindingView = AttachedPlaylistInfo;

/// Instantané complet et cohérent d'un renderer.
///
/// Le ControlPoint est la source de vérité : ce snapshot agrège l'état,
/// la queue et le binding observés atomiquement côté serveur.
#[cfg(feature = "pmoserver")]
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct FullRendererSnapshot {
    pub state: RendererStateView,
    pub queue: QueueSnapshotView,
    pub binding: Option<RendererBindingView>,
}

// ============================================================================
// PAYLOADS DE COMMANDES
// ============================================================================

/// Requête pour définir le volume
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct VolumeSetRequest {
    /// Nouveau volume (0-100)
    pub volume: u8,
}

/// Requête pour attacher une playlist
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AttachPlaylistRequest {
    /// ID du serveur de médias
    pub server_id: String,
    /// ID du container playlist
    pub container_id: String,
    /// Si true, démarre la lecture automatiquement après le refresh
    #[serde(default)]
    pub auto_play: bool,
}

/// Requête pour lire ou ajouter du contenu à la queue
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PlayContentRequest {
    /// ID du serveur de médias
    pub server_id: String,
    /// ID de l'objet (container ou item)
    pub object_id: String,
}

/// Requête pour sauter à un index spécifique dans la queue
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SeekQueueRequest {
    /// Index de l'item dans la queue (0-based)
    pub index: usize,
}

/// Requête pour seek à une position spécifique (en secondes)
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SeekRequest {
    /// Position en secondes
    pub seconds: u32,
}

/// Requête pour transférer une queue d'un renderer vers un autre
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TransferQueueRequest {
    /// ID du renderer de destination
    pub destination_renderer_id: String,
}

/// Requête pour démarrer ou mettre à jour le sleep timer
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SleepTimerRequest {
    /// Durée en secondes (maximum 7200 = 2 heures)
    pub duration_seconds: u32,
}

/// État du sleep timer
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SleepTimerState {
    /// Timer actif ou non
    pub active: bool,
    /// Durée totale configurée en secondes
    pub duration_seconds: u32,
    /// Secondes restantes (None si timer inactif)
    pub remaining_seconds: Option<u32>,
}

/// Réponse générique de succès
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SuccessResponse {
    /// Message de succès
    pub message: String,
}

/// Réponse d'erreur
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Message d'erreur
    pub error: String,
}

// ============================================================================
// DOCUMENTATION OPENAPI
// ============================================================================

/// Documentation OpenAPI pour l'API ControlPoint
#[cfg(feature = "pmoserver")]
#[derive(OpenApi)]
#[openapi(
    info(
        title = "PMOMusic Control Point API",
        version = "1.0.0",
        description = r#"
# API REST pour le Control Point PMOMusic

Cette API permet de contrôler les renderers audio et de naviguer dans les serveurs de médias de manière agnostique du backend.

## Fonctionnalités

### Renderers
- **Découverte** : Liste des renderers disponibles (tous types: UPnP AV, OpenHome, LinkPlay, Chromecast)
- **État** : Récupération de l'état détaillé d'un renderer
- **Contrôle transport** : Play, pause, stop, resume, next
- **Contrôle volume** : Lecture et modification du volume / mute
- **Queue unifiée** : Gestion de la queue de lecture (indépendante du backend)
- **Navigation dans la queue** : Saut à un index spécifique

### Playlists
- **Binding** : Attachement de la queue à un container playlist d'un serveur
- **Synchronisation automatique** : Mise à jour de la queue lors des changements côté serveur
- **Auto-play** : Option pour démarrer la lecture automatiquement

### Serveurs de médias
- **Découverte** : Liste des serveurs disponibles
- **Navigation** : Exploration de la hiérarchie des containers

## Architecture

Le Control Point PMOMusic est un point de contrôle multi-backend qui :
1. Découvre automatiquement les renderers et serveurs via SSDP et mDNS
2. Maintient un registre unifié des devices actifs
3. Abstrait les différences entre backends (UPnP AV, OpenHome, LinkPlay, Arylic TCP, Chromecast)
4. Gère une queue de lecture unifiée avec synchronisation optionnelle aux playlists serveur
5. Expose une API REST cohérente indépendante du type de renderer

## Exemples d'utilisation

### Lister les renderers
```
GET /control/renderers
```

### Obtenir l'état complet d'un renderer
```
GET /control/renderers/{renderer_id}/full
```

### Contrôler la lecture
```
POST /control/renderers/{renderer_id}/play
POST /control/renderers/{renderer_id}/pause
POST /control/renderers/{renderer_id}/stop
POST /control/renderers/{renderer_id}/resume
POST /control/renderers/{renderer_id}/next
```

### Naviguer dans la queue
```
POST /control/renderers/{renderer_id}/queue/seek
  Body: {"index": 5}
```

### Contrôler le volume
```
POST /control/renderers/{renderer_id}/volume/set
  Body: {"volume": 50}
POST /control/renderers/{renderer_id}/volume/up
POST /control/renderers/{renderer_id}/volume/down
POST /control/renderers/{renderer_id}/mute/toggle
```

### Attacher une playlist
```
POST /control/renderers/{renderer_id}/binding/attach
  Body: {
    "server_id": "uuid:...",
    "container_id": "0$/Music/MyPlaylist",
    "auto_play": true
  }
```

### Jouer du contenu
```
POST /control/renderers/{renderer_id}/queue/play
  Body: {
    "server_id": "uuid:...",
    "object_id": "0$/Music/Track.flac"
  }
```

### Naviguer dans un serveur
```
GET /control/servers/{server_id}/containers/{container_id}
```
        "#,
        contact(
            name = "PMOMusic",
        ),
        license(
            name = "MIT",
        ),
    ),
    paths(
        crate::pmoserver_ext::list_renderers,
        crate::pmoserver_ext::get_renderer_state,
        crate::pmoserver_ext::get_renderer_full_snapshot,
        crate::pmoserver_ext::get_renderer_queue,
        crate::pmoserver_ext::get_renderer_binding,
        crate::pmoserver_ext::play_renderer,
        crate::pmoserver_ext::pause_renderer,
        crate::pmoserver_ext::stop_renderer,
        crate::pmoserver_ext::resume_renderer,
        crate::pmoserver_ext::next_renderer,
        crate::pmoserver_ext::seek_renderer,
        crate::pmoserver_ext::seek_queue_index,
        crate::pmoserver_ext::set_renderer_volume,
        crate::pmoserver_ext::volume_up_renderer,
        crate::pmoserver_ext::volume_down_renderer,
        crate::pmoserver_ext::toggle_mute_renderer,
        crate::pmoserver_ext::start_sleep_timer,
        crate::pmoserver_ext::update_sleep_timer,
        crate::pmoserver_ext::cancel_sleep_timer,
        crate::pmoserver_ext::get_sleep_timer_state,
        crate::pmoserver_ext::attach_playlist_binding,
        crate::pmoserver_ext::detach_playlist_binding,
        crate::pmoserver_ext::play_content,
        crate::pmoserver_ext::add_to_queue,
        crate::pmoserver_ext::transfer_queue,
        crate::pmoserver_ext::list_servers,
        crate::pmoserver_ext::browse_container,
        crate::sse::all_events_sse,
        crate::sse::renderer_events_sse,
        crate::sse::media_server_events_sse,
    ),
    components(schemas(
        RendererSummary,
        RendererProtocolSummary,
        RendererCapabilitiesSummary,
        RendererState,
        CurrentTrackMetadata,
        AttachedPlaylistInfo,
        FullRendererSnapshot,
        QueueItem,
        QueueSnapshot,
        MediaServerSummary,
        ContainerEntry,
        BrowseResponse,
        VolumeSetRequest,
        AttachPlaylistRequest,
        PlayContentRequest,
        SeekQueueRequest,
        SeekRequest,
        TransferQueueRequest,
        SleepTimerRequest,
        SleepTimerState,
        SuccessResponse,
        ErrorResponse,
    )),
    tags(
        (name = "control", description = "Contrôle des renderers et navigation des serveurs")
    )
)]
pub struct ApiDoc;
