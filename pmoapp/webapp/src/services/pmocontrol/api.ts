// Client API REST pour PMOControl
// Communique avec /api/control/*

import type {
  RendererSummary,
  RendererState,
  QueueSnapshot,
  AttachedPlaylistInfo,
  MediaServerSummary,
  BrowseResponse,
  VolumeSetRequest,
  AttachPlaylistRequest,
  SuccessResponse,
  ErrorResponse
} from './types'

/**
 * Client API REST pour le Control Point PMOMusic
 */
class PMOControlAPI {
  private readonly baseURL = '/api/control'

  /**
   * Effectue une requête HTTP générique
   */
  private async request<T>(
    path: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseURL}${path}`

    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
    })

    if (!response.ok) {
      const error: ErrorResponse = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }))
      throw new Error(error.error)
    }

    return response.json()
  }

  // ============================================================================
  // RENDERERS
  // ============================================================================

  /**
   * Liste tous les renderers découverts
   * GET /api/control/renderers
   */
  async getRenderers(): Promise<RendererSummary[]> {
    return this.request<RendererSummary[]>('/renderers')
  }

  /**
   * Récupère l'état détaillé d'un renderer
   * GET /api/control/renderers/{id}
   */
  async getRendererState(id: string): Promise<RendererState> {
    return this.request<RendererState>(`/renderers/${encodeURIComponent(id)}`)
  }

  /**
   * Récupère la queue d'un renderer (avec current_index)
   * GET /api/control/renderers/{id}/queue
   */
  async getQueue(id: string): Promise<QueueSnapshot> {
    return this.request<QueueSnapshot>(`/renderers/${encodeURIComponent(id)}/queue`)
  }

  /**
   * Récupère le binding playlist d'un renderer
   * GET /api/control/renderers/{id}/binding
   */
  async getBinding(id: string): Promise<AttachedPlaylistInfo | null> {
    return this.request<AttachedPlaylistInfo | null>(`/renderers/${encodeURIComponent(id)}/binding`)
  }

  // ============================================================================
  // CONTRÔLE TRANSPORT
  // ============================================================================

  /**
   * Démarre la lecture sur un renderer
   * POST /api/control/renderers/{id}/play
   */
  async play(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/play`, {
      method: 'POST',
    })
  }

  /**
   * Met en pause la lecture sur un renderer
   * POST /api/control/renderers/{id}/pause
   */
  async pause(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/pause`, {
      method: 'POST',
    })
  }

  /**
   * Arrête la lecture sur un renderer
   * POST /api/control/renderers/{id}/stop
   */
  async stop(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/stop`, {
      method: 'POST',
    })
  }

  /**
   * Passe au morceau suivant dans la queue
   * POST /api/control/renderers/{id}/next
   */
  async next(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/next`, {
      method: 'POST',
    })
  }

  // ============================================================================
  // CONTRÔLE VOLUME
  // ============================================================================

  /**
   * Définit le volume d'un renderer (0-100)
   * POST /api/control/renderers/{id}/volume/set
   */
  async setVolume(id: string, volume: number): Promise<SuccessResponse> {
    const payload: VolumeSetRequest = { volume }
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/volume/set`, {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  }

  /**
   * Augmente le volume de 5%
   * POST /api/control/renderers/{id}/volume/up
   */
  async volumeUp(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/volume/up`, {
      method: 'POST',
    })
  }

  /**
   * Diminue le volume de 5%
   * POST /api/control/renderers/{id}/volume/down
   */
  async volumeDown(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/volume/down`, {
      method: 'POST',
    })
  }

  /**
   * Bascule le mute d'un renderer
   * POST /api/control/renderers/{id}/mute/toggle
   */
  async toggleMute(id: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(id)}/mute/toggle`, {
      method: 'POST',
    })
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
    containerId: string
  ): Promise<SuccessResponse> {
    const payload: AttachPlaylistRequest = { server_id: serverId, container_id: containerId }
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(rendererId)}/binding/attach`, {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  }

  /**
   * Détache la queue d'un renderer de sa playlist
   * POST /api/control/renderers/{id}/binding/detach
   */
  async detachPlaylist(rendererId: string): Promise<SuccessResponse> {
    return this.request<SuccessResponse>(`/renderers/${encodeURIComponent(rendererId)}/binding/detach`, {
      method: 'POST',
    })
  }

  // ============================================================================
  // MEDIA SERVERS
  // ============================================================================

  /**
   * Liste tous les serveurs de médias découverts
   * GET /api/control/servers
   */
  async getServers(): Promise<MediaServerSummary[]> {
    return this.request<MediaServerSummary[]>('/servers')
  }

  /**
   * Browse le contenu d'un container sur un serveur
   * GET /api/control/servers/{serverId}/containers/{containerId}
   */
  async browseContainer(serverId: string, containerId: string): Promise<BrowseResponse> {
    return this.request<BrowseResponse>(
      `/servers/${encodeURIComponent(serverId)}/containers/${encodeURIComponent(containerId)}`
    )
  }
}

// Export singleton
export const api = new PMOControlAPI()
