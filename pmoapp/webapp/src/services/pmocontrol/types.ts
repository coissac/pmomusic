// Types TypeScript pour l'API PMOControl
// Synchronisés avec pmocontrol/src/openapi.rs

// ============================================================================
// RENDERERS
// ============================================================================

export type RendererProtocolSummary =
  | "upnp"
  | "openhome"
  | "hybrid"
  | "chromecast";

export interface RendererCapabilitiesSummary {
  has_avtransport: boolean;
  has_avtransport_set_next: boolean;
  has_rendering_control: boolean;
  has_connection_manager: boolean;
  has_linkplay_http: boolean;
  has_arylic_tcp: boolean;
  has_oh_playlist: boolean;
  has_oh_volume: boolean;
  has_oh_info: boolean;
  has_oh_time: boolean;
  has_oh_radio: boolean;
}

export interface RendererSummary {
  id: string;
  friendly_name: string;
  model_name: string;
  protocol: RendererProtocolSummary;
  capabilities: RendererCapabilitiesSummary;
  online: boolean;
}

export interface RendererState {
  id: string;
  friendly_name: string;
  transport_state:
    | "PLAYING"
    | "PAUSED"
    | "STOPPED"
    | "TRANSITIONING"
    | "NO_MEDIA"
    | "UNKNOWN";
  position_ms: number | null;
  duration_ms: number | null;
  volume: number | null; // 0-100
  mute: boolean | null;
  queue_len: number;
  attached_playlist: AttachedPlaylistInfo | null;
  current_track: CurrentTrackMetadata | null;
}

export interface CurrentTrackMetadata {
  title: string | null;
  artist: string | null;
  album: string | null;
  album_art_uri: string | null;
}

export interface AttachedPlaylistInfo {
  server_id: string;
  container_id: string;
  has_seen_update: boolean;
}

// ============================================================================
// QUEUE (avec current_index)
// ============================================================================

export interface QueueItem {
  index: number; // 0-based
  uri: string;
  title: string | null;
  artist: string | null;
  album: string | null;
  album_art_uri: string | null;
  server_id: string | null;
  object_id: string | null;
}

export interface QueueSnapshot {
  renderer_id: string;
  items: QueueItem[];
  current_index: number | null; // Index de la piste en cours (null si rien en lecture)
}

export interface FullRendererSnapshot {
  state: RendererState;
  queue: QueueSnapshot;
  binding: AttachedPlaylistInfo | null;
}

// ============================================================================
// MEDIA SERVERS
// ============================================================================

export interface MediaServerSummary {
  id: string;
  friendly_name: string;
  model_name: string;
  online: boolean;
}

export interface ContainerEntry {
  id: string;
  title: string;
  class: string; // UPnP class
  is_container: boolean;
  child_count: number | null;
  artist: string | null;
  album: string | null;
  album_art_uri: string | null; // ⚠️ Nom exact: album_art_uri
}

export interface BrowseResponse {
  container_id: string;
  entries: ContainerEntry[];
}

// ============================================================================
// COMMANDES
// ============================================================================

export interface VolumeSetRequest {
  volume: number; // 0-100
}

export interface AttachPlaylistRequest {
  server_id: string;
  container_id: string;
  auto_play?: boolean;
}

export interface PlayContentRequest {
  server_id: string;
  object_id: string;
}

export interface SuccessResponse {
  message: string;
}

export interface ErrorResponse {
  error: string;
}

// ============================================================================
// ÉVÉNEMENTS SSE
// ============================================================================

export type RendererEventPayload =
  | {
      type: "state_changed";
      renderer_id: string;
      state: string;
      timestamp: string;
    }
  | {
      type: "position_changed";
      renderer_id: string;
      track: number | null;
      rel_time: string | null;
      track_duration: string | null;
      timestamp: string;
    }
  | {
      type: "volume_changed";
      renderer_id: string;
      volume: number;
      timestamp: string;
    }
  | {
      type: "mute_changed";
      renderer_id: string;
      mute: boolean;
      timestamp: string;
    }
  | {
      type: "metadata_changed";
      renderer_id: string;
      title: string | null;
      artist: string | null;
      album: string | null;
      album_art_uri: string | null;
      timestamp: string;
    }
  | {
      type: "queue_updated";
      renderer_id: string;
      queue_length: number;
      timestamp: string;
    }
  | {
      type: "binding_changed";
      renderer_id: string;
      server_id: string | null;
      container_id: string | null;
      timestamp: string;
    }
  | {
      type: "timer_started";
      renderer_id: string;
      duration_seconds: number;
      remaining_seconds: number;
      timestamp: string;
    }
  | {
      type: "timer_updated";
      renderer_id: string;
      duration_seconds: number;
      remaining_seconds: number;
      timestamp: string;
    }
  | {
      type: "timer_tick";
      renderer_id: string;
      remaining_seconds: number;
      timestamp: string;
    }
  | {
      type: "timer_expired";
      renderer_id: string;
      timestamp: string;
    }
  | {
      type: "timer_cancelled";
      renderer_id: string;
      timestamp: string;
    }
  | {
      type: "online";
      renderer_id: string;
      friendly_name: string;
      model_name: string;
      manufacturer: string;
      timestamp: string;
    }
  | { type: "offline"; renderer_id: string; timestamp: string };

export type MediaServerEventPayload =
  | {
      type: "global_updated";
      server_id: string;
      system_update_id: number | null;
      timestamp: string;
    }
  | {
      type: "containers_updated";
      server_id: string;
      container_ids: string[];
      timestamp: string;
    }
  | {
      type: "online";
      server_id: string;
      friendly_name: string;
      model_name: string;
      manufacturer: string;
      timestamp: string;
    }
  | { type: "offline"; server_id: string; timestamp: string };

export type UnifiedEventPayload =
  | ({ category: "renderer" } & RendererEventPayload)
  | ({ category: "media_server" } & MediaServerEventPayload);

// ============================================================================
// MÉTADONNÉES PISTE
// ============================================================================

export interface TrackMetadata {
  title: string | null;
  artist: string | null;
  album: string | null;
  album_art_uri: string | null;
  duration_ms: number | null;
}

export interface PositionInfo {
  track: number | null;
  rel_time: string | null; // Format HH:MM:SS
  track_duration: string | null; // Format HH:MM:SS
}

// ============================================================================
// SLEEP TIMER
// ============================================================================

export interface SleepTimerState {
  active: boolean;
  duration_seconds: number;
  remaining_seconds: number | null;
}

export interface SleepTimerRequest {
  duration_seconds: number; // 0-7200 (0-2 heures)
}
