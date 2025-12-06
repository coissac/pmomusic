// Service SSE (Server-Sent Events) pour PMOControl
// Gère la connexion temps réel à /api/control/events

import type { RendererEventPayload, MediaServerEventPayload, UnifiedEventPayload } from './types'

type RendererEventCallback = (event: RendererEventPayload) => void
type MediaServerEventCallback = (event: MediaServerEventPayload) => void
type ConnectionCallback = (connected: boolean) => void

/**
 * Service SSE pour recevoir les événements du Control Point en temps réel
 */
export class PMOControlSSE {
  private eventSource: EventSource | null = null
  private reconnectAttempts = 0
  private maxReconnectDelay = 30000  // 30 secondes max
  private reconnectTimer: number | null = null

  private rendererCallbacks: Set<RendererEventCallback> = new Set()
  private serverCallbacks: Set<MediaServerEventCallback> = new Set()
  private connectionCallbacks: Set<ConnectionCallback> = new Set()

  private isConnected = false

  /**
   * Connecte au flux SSE
   */
  connect(): void {
    if (this.eventSource) {
      console.warn('[SSE] Connexion déjà active')
      return
    }

    console.log('[SSE] Connexion à /api/control/events...')

    try {
      this.eventSource = new EventSource('/api/control/events')

      this.eventSource.onopen = () => {
        console.log('[SSE] Connexion établie')
        this.reconnectAttempts = 0
        this.isConnected = true
        this.notifyConnectionCallbacks(true)
      }

      this.eventSource.addEventListener('control', (e: MessageEvent) => {
        try {
          const event: UnifiedEventPayload = JSON.parse(e.data)
          this.handleEvent(event)
        } catch (error) {
          console.error('[SSE] Erreur parsing événement:', error)
        }
      })

      this.eventSource.onerror = () => {
        console.error('[SSE] Erreur de connexion')
        this.isConnected = false
        this.notifyConnectionCallbacks(false)
        this.disconnect()
        this.scheduleReconnect()
      }
    } catch (error) {
      console.error('[SSE] Erreur création EventSource:', error)
      this.scheduleReconnect()
    }
  }

  /**
   * Déconnecte du flux SSE
   */
  disconnect(): void {
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }

    if (this.eventSource) {
      console.log('[SSE] Déconnexion')
      this.eventSource.close()
      this.eventSource = null
      this.isConnected = false
      this.notifyConnectionCallbacks(false)
    }
  }

  /**
   * Programme une reconnexion avec backoff exponentiel
   */
  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) {
      return
    }

    this.reconnectAttempts++

    // Backoff exponentiel: 1s, 2s, 4s, 8s, 16s, 30s (max)
    const delay = Math.min(
      1000 * Math.pow(2, this.reconnectAttempts - 1),
      this.maxReconnectDelay
    )

    console.log(`[SSE] Reconnexion dans ${delay / 1000}s (tentative ${this.reconnectAttempts})`)

    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null
      this.connect()
    }, delay)
  }

  /**
   * Dispatch un événement aux callbacks appropriés
   */
  private handleEvent(event: UnifiedEventPayload): void {
    if (event.category === 'renderer') {
      // Extraire le payload renderer (sans le champ category)
      const { category, ...rendererEvent } = event
      this.notifyRendererCallbacks(rendererEvent as RendererEventPayload)
    } else if (event.category === 'media_server') {
      // Extraire le payload server (sans le champ category)
      const { category, ...serverEvent } = event
      this.notifyServerCallbacks(serverEvent as MediaServerEventPayload)
    }
  }

  /**
   * Enregistre un callback pour les événements renderer
   */
  onRendererEvent(callback: RendererEventCallback): () => void {
    this.rendererCallbacks.add(callback)
    // Retourne une fonction de cleanup
    return () => {
      this.rendererCallbacks.delete(callback)
    }
  }

  /**
   * Enregistre un callback pour les événements media server
   */
  onMediaServerEvent(callback: MediaServerEventCallback): () => void {
    this.serverCallbacks.add(callback)
    // Retourne une fonction de cleanup
    return () => {
      this.serverCallbacks.delete(callback)
    }
  }

  /**
   * Enregistre un callback pour les changements de connexion
   */
  onConnectionChange(callback: ConnectionCallback): () => void {
    this.connectionCallbacks.add(callback)
    // Appeler immédiatement avec l'état actuel
    callback(this.isConnected)
    // Retourne une fonction de cleanup
    return () => {
      this.connectionCallbacks.delete(callback)
    }
  }

  /**
   * Notifie tous les callbacks renderer
   */
  private notifyRendererCallbacks(event: RendererEventPayload): void {
    this.rendererCallbacks.forEach(callback => {
      try {
        callback(event)
      } catch (error) {
        console.error('[SSE] Erreur dans callback renderer:', error)
      }
    })
  }

  /**
   * Notifie tous les callbacks server
   */
  private notifyServerCallbacks(event: MediaServerEventPayload): void {
    this.serverCallbacks.forEach(callback => {
      try {
        callback(event)
      } catch (error) {
        console.error('[SSE] Erreur dans callback server:', error)
      }
    })
  }

  /**
   * Notifie tous les callbacks de connexion
   */
  private notifyConnectionCallbacks(connected: boolean): void {
    this.connectionCallbacks.forEach(callback => {
      try {
        callback(connected)
      } catch (error) {
        console.error('[SSE] Erreur dans callback connexion:', error)
      }
    })
  }

  /**
   * Retourne l'état de connexion actuel
   */
  isConnectedState(): boolean {
    return this.isConnected
  }
}

// Export singleton
export const sse = new PMOControlSSE()
