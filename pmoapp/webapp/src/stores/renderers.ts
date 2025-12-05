// Store Pinia pour les renderers
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '../services/pmocontrol/api'
import type {
  RendererSummary,
  RendererState,
  QueueSnapshot,
  AttachedPlaylistInfo,
  RendererEventPayload
} from '../services/pmocontrol/types'

export const useRenderersStore = defineStore('renderers', () => {
  // État
  const renderers = ref<Map<string, RendererSummary>>(new Map())
  const states = ref<Map<string, RendererState>>(new Map())
  const queues = ref<Map<string, QueueSnapshot>>(new Map())
  const bindings = ref<Map<string, AttachedPlaylistInfo | null>>(new Map())
  const loading = ref(false)
  const error = ref<string | null>(null)

  // Getters
  const onlineRenderers = computed(() => {
    return Array.from(renderers.value.values()).filter(r => r.online)
  })

  const playingRenderers = computed(() => {
    return Array.from(states.value.values()).filter(
      s => s.transport_state === 'PLAYING'
    )
  })

  const getRendererById = (id: string) => {
    return renderers.value.get(id)
  }

  const getStateById = (id: string) => {
    return states.value.get(id)
  }

  const getQueueById = (id: string) => {
    return queues.value.get(id)
  }

  const getBindingById = (id: string) => {
    return bindings.value.get(id)
  }

  // Actions - Fetch data
  async function fetchRenderers() {
    try {
      loading.value = true
      error.value = null
      const data = await api.getRenderers()

      // Mettre à jour le Map
      renderers.value.clear()
      data.forEach(r => renderers.value.set(r.id, r))
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur fetch renderers'
      console.error('[Store Renderers] Erreur fetch:', e)
    } finally {
      loading.value = false
    }
  }

  async function fetchRendererState(id: string) {
    try {
      const state = await api.getRendererState(id)
      states.value.set(id, state)
    } catch (e) {
      console.error(`[Store Renderers] Erreur fetch state ${id}:`, e)
    }
  }

  async function fetchQueue(id: string) {
    try {
      const queue = await api.getQueue(id)
      queues.value.set(id, queue)
    } catch (e) {
      console.error(`[Store Renderers] Erreur fetch queue ${id}:`, e)
    }
  }

  async function fetchBinding(id: string) {
    try {
      const binding = await api.getBinding(id)
      bindings.value.set(id, binding)
    } catch (e) {
      console.error(`[Store Renderers] Erreur fetch binding ${id}:`, e)
    }
  }

  // Actions - Transport controls
  async function play(id: string) {
    try {
      await api.play(id)
      // L'état sera mis à jour via SSE
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur play'
      console.error(`[Store Renderers] Erreur play ${id}:`, e)
      throw e
    }
  }

  /**
   * Démarre ou reprend la lecture de manière intelligente:
   * - Si PAUSED: reprend avec AVTransport.Play (resume)
   * - Si STOPPED/NO_MEDIA avec queue non vide: reprend depuis le morceau actuel (via /resume)
   * - Sinon: erreur (queue vide)
   */
  async function resumeOrPlayFromQueue(id: string) {
    const state = getStateById(id)
    if (!state) {
      throw new Error(`Renderer ${id} non trouvé`)
    }

    // Cas 1: En pause → reprendre normalement
    if (state.transport_state === 'PAUSED') {
      return play(id)
    }

    // Cas 2: Arrêté ou NO_MEDIA avec une queue → reprendre depuis le morceau actuel
    if ((state.transport_state === 'STOPPED' || state.transport_state === 'NO_MEDIA') &&
        state.queue_len && state.queue_len > 0) {
      // Utiliser /resume qui appelle play_current_from_queue côté backend
      // Cela reprend la lecture depuis le morceau actuel sans avancer
      return resume(id)
    }

    // Cas 3: Queue vide → erreur explicite
    throw new Error('La file d\'attente est vide. Ajoutez des morceaux avant de démarrer la lecture.')
  }

  async function pause(id: string) {
    try {
      await api.pause(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur pause'
      console.error(`[Store Renderers] Erreur pause ${id}:`, e)
      throw e
    }
  }

  async function stop(id: string) {
    try {
      await api.stop(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur stop'
      console.error(`[Store Renderers] Erreur stop ${id}:`, e)
      throw e
    }
  }

  async function resume(id: string) {
    try {
      await api.resume(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur resume'
      console.error(`[Store Renderers] Erreur resume ${id}:`, e)
      throw e
    }
  }

  async function next(id: string) {
    try {
      await api.next(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur next'
      console.error(`[Store Renderers] Erreur next ${id}:`, e)
      throw e
    }
  }

  // Actions - Volume controls
  async function setVolume(id: string, volume: number) {
    try {
      await api.setVolume(id, volume)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur set volume'
      console.error(`[Store Renderers] Erreur set volume ${id}:`, e)
      throw e
    }
  }

  async function volumeUp(id: string) {
    try {
      await api.volumeUp(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur volume up'
      console.error(`[Store Renderers] Erreur volume up ${id}:`, e)
      throw e
    }
  }

  async function volumeDown(id: string) {
    try {
      await api.volumeDown(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur volume down'
      console.error(`[Store Renderers] Erreur volume down ${id}:`, e)
      throw e
    }
  }

  async function toggleMute(id: string) {
    try {
      await api.toggleMute(id)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur toggle mute'
      console.error(`[Store Renderers] Erreur toggle mute ${id}:`, e)
      throw e
    }
  }

  // Actions - Playlist binding
  async function attachPlaylist(rendererId: string, serverId: string, containerId: string) {
    try {
      await api.attachPlaylist(rendererId, serverId, containerId)
      // Rafraîchir le binding
      await fetchBinding(rendererId)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur attach playlist'
      console.error(`[Store Renderers] Erreur attach playlist ${rendererId}:`, e)
      throw e
    }
  }

  async function detachPlaylist(rendererId: string) {
    try {
      await api.detachPlaylist(rendererId)
      bindings.value.set(rendererId, null)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur detach playlist'
      console.error(`[Store Renderers] Erreur detach playlist ${rendererId}:`, e)
      throw e
    }
  }

  /**
   * Attache une playlist à un renderer et démarre la lecture
   *
   * Note: Le backend démarre automatiquement la lecture lors de l'attachement
   * via start_queue_playback_if_idle, donc pas besoin d'appeler play() explicitement.
   * On rafraîchit seulement le binding et la queue pour l'UI.
   */
  async function attachAndPlayPlaylist(rendererId: string, serverId: string, containerId: string) {
    try {
      // 1. Attacher la playlist au renderer
      // Le backend va automatiquement démarrer la lecture via start_queue_playback_if_idle
      await api.attachPlaylist(rendererId, serverId, containerId)

      // 2. Rafraîchir le binding local
      await fetchBinding(rendererId)

      // 3. Recharger la queue pour avoir le contenu immédiatement dans l'UI
      await fetchQueue(rendererId)

      // Note: Pas besoin d'appeler play() ici - le backend le fait automatiquement!
      // Appeler play() ici créerait une race condition entre AVTransport.Play
      // et le play_next_from_queue du backend.
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur attach and play playlist'
      console.error(`[Store Renderers] Erreur attach and play playlist ${rendererId}:`, e)
      throw e
    }
  }

  // Actions - Queue content
  async function playContent(rendererId: string, serverId: string, objectId: string) {
    try {
      await api.playContent(rendererId, serverId, objectId)
      // L'état et la queue seront mis à jour via SSE
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur play content'
      console.error(`[Store Renderers] Erreur play content ${rendererId}:`, e)
      throw e
    }
  }

  async function addToQueue(rendererId: string, serverId: string, objectId: string) {
    try {
      await api.addToQueue(rendererId, serverId, objectId)
      // La queue sera mise à jour via SSE
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur add to queue'
      console.error(`[Store Renderers] Erreur add to queue ${rendererId}:`, e)
      throw e
    }
  }

  // SSE event handling
  function updateFromSSE(event: RendererEventPayload) {
    const rendererId = event.renderer_id

    switch (event.type) {
      case 'state_changed': {
        const state = states.value.get(rendererId)
        if (state) {
          state.transport_state = event.state as any
        }
        break
      }

      case 'position_changed': {
        const state = states.value.get(rendererId)
        if (state && event.rel_time && event.track_duration) {
          // Convertir HH:MM:SS en millisecondes
          state.position_ms = parseHMStoMs(event.rel_time)
          state.duration_ms = parseHMStoMs(event.track_duration)
        }
        break
      }

      case 'volume_changed': {
        const state = states.value.get(rendererId)
        if (state) {
          state.volume = event.volume
        }
        break
      }

      case 'mute_changed': {
        const state = states.value.get(rendererId)
        if (state) {
          state.mute = event.mute
        }
        break
      }

      case 'queue_updated': {
        const state = states.value.get(rendererId)
        if (state) {
          state.queue_len = event.queue_length
        }
        // Rafraîchir la queue complète
        fetchQueue(rendererId)
        break
      }

      case 'metadata_changed': {
        // Les métadonnées sont gérées par le store playback
        break
      }

      case 'binding_changed': {
        if (event.server_id && event.container_id) {
          // Binding créé ou mis à jour
          bindings.value.set(rendererId, {
            server_id: event.server_id,
            container_id: event.container_id,
            has_seen_update: false,
          })
          // Recharger la queue pour refléter le nouveau binding
          fetchQueue(rendererId)
        } else {
          // Binding supprimé
          bindings.value.set(rendererId, null)
        }
        break
      }
    }
  }

  // Utilitaire: convertit HH:MM:SS en millisecondes
  function parseHMStoMs(hms: string): number {
    const parts = hms.split(':').map(Number)
    if (parts.length === 3 && parts.every(p => !isNaN(p))) {
      const hours = parts[0]!
      const minutes = parts[1]!
      const seconds = parts[2]!
      return (hours * 3600 + minutes * 60 + seconds) * 1000
    }
    return 0
  }

  return {
    // État
    renderers,
    states,
    queues,
    bindings,
    loading,
    error,
    // Getters
    onlineRenderers,
    playingRenderers,
    getRendererById,
    getStateById,
    getQueueById,
    getBindingById,
    // Actions
    fetchRenderers,
    fetchRendererState,
    fetchQueue,
    fetchBinding,
    play,
    resumeOrPlayFromQueue,
    pause,
    stop,
    resume,
    next,
    setVolume,
    volumeUp,
    volumeDown,
    toggleMute,
    attachPlaylist,
    detachPlaylist,
    attachAndPlayPlaylist,
    playContent,
    addToQueue,
    updateFromSSE,
  }
})
