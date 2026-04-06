/**
 * Composable centralisé pour la gestion des Server-Sent Events
 * 
 * Ce composable fournit une interface unifiée pour:
 * - Une seule connexion SSE (évite les connexions multiples)
 * - Des abonnements typés aux événements (renderers, servers)
 * - Le suivi de l'état de connexion
 * 
 * Usage:
 *   const { connected, onRendererEvent, onMediaServerEvent } = useSSE()
 */
import { ref, readonly, onUnmounted } from 'vue'
import { sse } from '../services/pmocontrol/sse'
import type { 
  RendererEventPayload, 
  MediaServerEventPayload 
} from '../services/pmocontrol/types'

// État global partagé
const connected = ref(sse.isConnectedState());
const connectionCallbacks: Set<(connected: boolean) => void> = new Set();

// Flag pour éviter les double-connexions SSE avec lock
let connectionLock = false;

// Abonnement à l'état de connexion global
function setupConnectionListener() {
  // Vérifier avec lock pour éviter les conditions de course
  if (connectionCallbacks.size === 0 && !connectionLock) {
    connectionLock = true;
    sse.onConnectionChange((isConnected) => {
      connected.value = isConnected
      connectionCallbacks.forEach(cb => cb(isConnected))
    })
  }
}

/**
 * Hook principal pour utiliser SSE
 */
export function useSSE() {
  // S'assurer que le listener de connexion est configuré
  setupConnectionListener()

  /**
   * Abonnement aux événements de type renderer
   * Retourne une fonction de cleanup
   */
  function onRendererEvent(
    callback: (event: RendererEventPayload) => void
  ): () => void {
    return sse.onRendererEvent(callback)
  }

  /**
   * Abonnement aux événements de type media server
   * Retourne une fonction de cleanup
   */
  function onMediaServerEvent(
    callback: (event: MediaServerEventPayload) => void
  ): () => void {
    return sse.onMediaServerEvent(callback)
  }

  /**
   * Abonnement aux changements de connexion
   * Retourne une fonction de cleanup
   */
  function onConnectionChange(
    callback: (connected: boolean) => void
  ): () => void {
    connectionCallbacks.add(callback)
    // Appeler immédiatement avec l'état actuel
    callback(connected.value)
    
    // Retourner fonction de cleanup
    return () => {
      connectionCallbacks.delete(callback)
    }
  }

  /**
   * Force la connexion SSE
   */
  function connect(): void {
    sse.connect()
  }

  /**
   * Force la déconnexion SSE
   */
  function disconnect(): void {
    sse.disconnect()
  }

  /**
   * Vérifie si actuellement connecté
   */
  function isConnected(): boolean {
    return connected.value
  }

  return {
    // État (readonly pour éviter les modifications directes)
    connected: readonly(connected),
    
    // Abonnements
    onRendererEvent,
    onMediaServerEvent,
    onConnectionChange,
    
    // Actions
    connect,
    disconnect,
    isConnected,
  }
}

/**
 * Hook pour s'abonner à un type spécifique d'événement renderer
 * avec filtrage optionnel par rendererId
 */
export function useRendererEvents(
  rendererId: () => string | null,
  options?: {
    onStateChanged?: (event: RendererEventPayload) => void
    onPositionChanged?: (event: RendererEventPayload) => void
    onVolumeChanged?: (event: RendererEventPayload) => void
    onMetadataChanged?: (event: RendererEventPayload) => void
    onQueueUpdated?: (event: RendererEventPayload) => void
    onBindingChanged?: (event: RendererEventPayload) => void
    onTimerEvent?: (event: RendererEventPayload) => void
  }
) {
  const { onRendererEvent } = useSSE()
  
  let cleanup: (() => void) | null = null

  function setup() {
    cleanup = onRendererEvent((event) => {
      const currentId = rendererId()
      
      // Si un rendererId est spécifié, filtrer
      if (currentId && event.renderer_id !== currentId) {
        return
      }

      // Dispatch vers le handler approprié
      switch (event.type) {
        case 'state_changed':
        case 'online':
        case 'offline':
          options?.onStateChanged?.(event)
          break
        case 'position_changed':
          options?.onPositionChanged?.(event)
          break
        case 'volume_changed':
        case 'mute_changed':
          options?.onVolumeChanged?.(event)
          break
        case 'metadata_changed':
          options?.onMetadataChanged?.(event)
          break
        case 'queue_updated':
        case 'queue_refreshing':
          options?.onQueueUpdated?.(event)
          break
        case 'binding_changed':
          options?.onBindingChanged?.(event)
          break
        case 'timer_started':
        case 'timer_updated':
        case 'timer_tick':
        case 'timer_expired':
        case 'timer_cancelled':
          options?.onTimerEvent?.(event)
          break
      }
    })
  }

  setup()

  // Cleanup automatique au unmount du composant
  onUnmounted(() => {
    cleanup?.()
  })

  return {
    refresh: setup, // Permet de recréer l'abonnement si besoin
  }
}

/**
 * Hook pour s'abonner aux événements media server
 * avec filtrage optionnel par serverId
 */
export function useMediaServerEvents(
  serverId: () => string | null,
  options?: {
    onOnline?: (event: MediaServerEventPayload) => void
    onOffline?: (event: MediaServerEventPayload) => void
    onGlobalUpdated?: (event: MediaServerEventPayload) => void
    onContainersUpdated?: (event: MediaServerEventPayload) => void
  }
) {
  const { onMediaServerEvent } = useSSE()
  
  let cleanup: (() => void) | null = null

  function setup() {
    cleanup = onMediaServerEvent((event) => {
      const currentId = serverId()
      
      // Si un serverId est spécifié, filtrer
      if (currentId && event.server_id !== currentId) {
        return
      }

      // Dispatch vers le handler approprié
      switch (event.type) {
        case 'online':
          options?.onOnline?.(event)
          break
        case 'offline':
          options?.onOffline?.(event)
          break
        case 'global_updated':
          options?.onGlobalUpdated?.(event)
          break
        case 'containers_updated':
          options?.onContainersUpdated?.(event)
          break
      }
    })
  }

  setup()

  onUnmounted(() => {
    cleanup?.()
  })

  return {
    refresh: setup,
  }
}