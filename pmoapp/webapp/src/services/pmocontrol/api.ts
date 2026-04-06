// Client API REST pour PMOControl
// Communique avec /api/control/*

import type {
  RendererSummary,
  RendererState,
  FullRendererSnapshot,
  QueueSnapshot,
  AttachedPlaylistInfo,
  MediaServerSummary,
  BrowseResponse,
  VolumeSetRequest,
  AttachPlaylistRequest,
  PlayContentRequest,
  SleepTimerState,
  SuccessResponse,
  ErrorResponse,
} from "./types";

/**
 * Fetch avec timeout et AbortController
 */
function fetchWithTimeout(
  url: string,
  options: RequestInit = {},
  timeoutMs = 10_000,
): Promise<Response> {
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeoutMs);
  return fetch(url, { ...options, signal: controller.signal }).finally(
    () => clearTimeout(id),
  );
}

/**
 * Client API REST pour le Control Point PMOMusic
 */
class PMOControlAPI {
  private readonly baseURL = "/api/control";

  /**
   * Valide la structure de base d'une réponse
   * Jette une erreur si la réponse est invalide
   */
  private validateResponse<T>(data: unknown, path: string): T {
    // Vérification basique : null ou undefined
    if (data == null) {
      throw new Error(`[PMOControlAPI] Réponse nulle pour ${path}`);
    }

    // Vérification que c'est un objet
    if (typeof data !== 'object') {
      throw new Error(`[PMOControlAPI] Réponse invalide pour ${path}: attendu un objet`);
    }

    return data as T;
  }

  /**
   * Effectue une requête HTTP générique
   */
  private async request<T>(
    path: string,
    options: RequestInit = {},
  ): Promise<T> {
    const url = `${this.baseURL}${path}`;

    const response = await fetchWithTimeout(url, {
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
    });

    if (!response.ok) {
      const error: ErrorResponse = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(error.error);
    }

    const data = await response.json();
    
    // Validation de la réponse (P5)
    const validated = this.validateResponse<T>(data, path);
    
    if (import.meta.env.DEV && data == null) {
      console.warn(`[PMOControlAPI] Réponse vide pour ${path}`);
    }
    return validated;
  }

  // ============================================================================
  // RENDERERS
  // ============================================================================

  /**
   * Liste tous les renderers découverts
   * GET /api/control/renderers
   */
  async getRenderers(): Promise<RendererSummary[]> {
    return this.request<RendererSummary[]>("/renderers");
  }

  /**
   * Récupère l'état détaillé d'un renderer
   * GET /api/control/renderers/{id}
   */
  async getRendererState(id: string): Promise<RendererState> {
    return this.request<RendererState>(`/renderers/${encodeURIComponent(id)}`);
  }

  /**
   * Récupère le snapshot complet d'un renderer
   * GET /api/control/renderers/{id}/full
   */
  async getRendererFullSnapshot(id: string): Promise<FullRendererSnapshot> {
    return this.request<FullRendererSnapshot>(
      `/renderers/${encodeURIComponent(id)}/full`,
    );
  }

  /**
   * Récupère la queue d'un renderer (avec current_index)
   * GET /api/control/renderers/{id}/queue
   */
  async getQueue(id: string): Promise<QueueSnapshot> {
    return this.request<QueueSnapshot>(
      `/renderers/${encodeURIComponent(id)}/queue`,
    );
  }

  /**
   * Récupère le binding playlist d'un renderer
   * GET /api/control/renderers/{id}/binding
   */
  async getBinding(id: string): Promise<AttachedPlaylistInfo | null> {
    return this.request<AttachedPlaylistInfo | null>(
      `/renderers/${encodeURIComponent(id)}/binding`,
    );
  }

  // ============================================================================
  // CONTRÔLE TRANSPORT
  // ============================================================================

  /**
   * Démarre la lecture sur un renderer
   * POST /api/control/renderers/{id}/play
   */
  async play(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/play`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Met en pause la lecture sur un renderer
   * POST /api/control/renderers/{id}/pause
   */
  async pause(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/pause`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Arrête la lecture sur un renderer
   * POST /api/control/renderers/{id}/stop
   */
  async stop(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/stop`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Reprend la lecture depuis le morceau actuel de la queue
   * POST /api/control/renderers/{id}/resume
   */
  async resume(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/resume`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Passe au morceau suivant dans la queue
   * POST /api/control/renderers/{id}/next
   */
  async next(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/next`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Seek à une position spécifique (en secondes)
   * POST /api/control/renderers/{id}/seek
   */
  async seekTo(id: string, seconds: number): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/seek`,
      {
        method: "POST",
        body: JSON.stringify({ seconds }),
      },
    );
  }

  /**
   * Saute à un index spécifique dans la queue
   * POST /api/control/renderers/{id}/queue/seek
   */
  async seekQueueIndex(id: string, index: number): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/queue/seek`,
      {
        method: "POST",
        body: JSON.stringify({ index }),
      },
    );
  }

  /**
   * Mélange la queue de lecture et démarre au premier morceau
   * POST /api/control/renderers/{id}/queue/shuffle
   */
  async shuffleQueue(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/queue/shuffle`,
      {
        method: "POST",
      },
    );
  }

  // ============================================================================
  // CONTRÔLE VOLUME
  // ============================================================================

  /**
   * Définit le volume d'un renderer (0-100)
   * POST /api/control/renderers/{id}/volume/set
   */
  async setVolume(id: string, volume: number): Promise<SuccessResponse> {
    const payload: VolumeSetRequest = { volume };
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/volume/set`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  /**
   * Augmente le volume de 5%
   * POST /api/control/renderers/{id}/volume/up
   */
  async volumeUp(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/volume/up`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Diminue le volume de 5%
   * POST /api/control/renderers/{id}/volume/down
   */
  async volumeDown(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/volume/down`,
      {
        method: "POST",
      },
    );
  }

  /**
   * Bascule le mute d'un renderer
   * POST /api/control/renderers/{id}/mute/toggle
   */
  async toggleMute(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(id)}/mute/toggle`,
      {
        method: "POST",
      },
    );
  }

  // ============================================================================
  // PLAYLIST BINDING
  // ============================================================================

  /**
   * Attache la queue d'un renderer à une playlist d'un serveur
   * POST /api/control/renderers/{id}/binding/attach
   */
  async attachPlaylist(
    rendererId: string,
    serverId: string,
    containerId: string,
    autoPlay = false,
  ): Promise<SuccessResponse> {
    const payload: AttachPlaylistRequest = {
      server_id: serverId,
      container_id: containerId,
      auto_play: autoPlay,
    };
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(rendererId)}/binding/attach`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  /**
   * Détache la queue d'un renderer de sa playlist
   * POST /api/control/renderers/{id}/binding/detach
   */
  async detachPlaylist(rendererId: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(rendererId)}/binding/detach`,
      {
        method: "POST",
      },
    );
  }

  // ============================================================================
  // QUEUE CONTENT
  // ============================================================================

  /**
   * Lire du contenu immédiatement (clear queue + enqueue + play)
   * POST /api/control/renderers/{id}/queue/play
   */
  async playContent(
    rendererId: string,
    serverId: string,
    objectId: string,
  ): Promise<SuccessResponse> {
    const payload: PlayContentRequest = {
      server_id: serverId,
      object_id: objectId,
    };
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(rendererId)}/queue/play`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  /**
   * Ajouter du contenu à la queue (sans démarrer la lecture)
   * POST /api/control/renderers/{id}/queue/add
   */
  async addToQueue(
    rendererId: string,
    serverId: string,
    objectId: string,
  ): Promise<SuccessResponse> {
    const payload: PlayContentRequest = {
      server_id: serverId,
      object_id: objectId,
    };
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(rendererId)}/queue/add`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  /**
   * Ajouter du contenu après le morceau actuel
   * POST /api/control/renderers/{id}/queue/add-after
   */
  async addAfterCurrent(
    rendererId: string,
    serverId: string,
    objectId: string,
  ): Promise<SuccessResponse> {
    const payload: PlayContentRequest = {
      server_id: serverId,
      object_id: objectId,
    };
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(rendererId)}/queue/add-after`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  /**
   * Transfère la queue d'un renderer vers un autre
   * POST /api/control/renderers/{id}/queue/transfer
   */
  async transferQueue(
    sourceRendererId: string,
    destinationRendererId: string,
  ): Promise<SuccessResponse> {
    const payload = {
      destination_renderer_id: destinationRendererId,
    };
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(sourceRendererId)}/queue/transfer`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  // ============================================================================
  // MEDIA SERVERS
  // ============================================================================

  /**
   * Liste tous les serveurs de médias découverts
   * GET /api/control/servers
   */
  async getServers(): Promise<MediaServerSummary[]> {
    return this.request<MediaServerSummary[]>("/servers");
  }

  /**
   * Browse le contenu d'un container sur un serveur
   * GET /api/control/servers/{serverId}/containers/{containerId}
   */
  async browseContainer(
    serverId: string,
    containerId: string,
    offset = 0,
    limit = 50,
  ): Promise<BrowseResponse> {
    return this.request<BrowseResponse>(
      `/servers/${encodeURIComponent(serverId)}/containers/${encodeURIComponent(containerId)}?offset=${offset}&limit=${limit}`,
    );
  }

  /**
   * Recherche dans un serveur media
   * GET /api/control/servers/{serverId}/search?q={query}
   */
  async searchServer(serverId: string, query: string): Promise<BrowseResponse> {
    return this.request<BrowseResponse>(
      `/servers/${encodeURIComponent(serverId)}/search?q=${encodeURIComponent(query)}`,
    );
  }

  // ============================================================================
  // SLEEP TIMER
  // ============================================================================

  /**
   * Récupère l'état du sleep timer
   * GET /api/control/renderers/{rendererId}/timer
   */
  async getSleepTimer(rendererId: string): Promise<SleepTimerState> {
    return this.request<SleepTimerState>(
      `/renderers/${encodeURIComponent(rendererId)}/timer`,
    );
  }

  /**
   * Démarre le sleep timer
   * POST /api/control/renderers/{rendererId}/timer/start
   */
  async startSleepTimer(
    rendererId: string,
    durationSeconds: number,
  ): Promise<SleepTimerState> {
    return this.request<SleepTimerState>(
      `/renderers/${encodeURIComponent(rendererId)}/timer/start`,
      {
        method: "POST",
        body: JSON.stringify({ duration_seconds: durationSeconds }),
      },
    );
  }

  /**
   * Met à jour le sleep timer (modifie la durée)
   * POST /api/control/renderers/{rendererId}/timer/update
   */
  async updateSleepTimer(
    rendererId: string,
    durationSeconds: number,
  ): Promise<SleepTimerState> {
    return this.request<SleepTimerState>(
      `/renderers/${encodeURIComponent(rendererId)}/timer/update`,
      {
        method: "POST",
        body: JSON.stringify({ duration_seconds: durationSeconds }),
      },
    );
  }

  /**
   * Annule le sleep timer
   * POST /api/control/renderers/{rendererId}/timer/cancel
   */
  async cancelSleepTimer(rendererId: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(
      `/renderers/${encodeURIComponent(rendererId)}/timer/cancel`,
      {
        method: "POST",
      },
    );
  }
}

// Export singleton
export const api = new PMOControlAPI();
