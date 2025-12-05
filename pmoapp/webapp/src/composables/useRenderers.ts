/**
 * Composable pour gérer les renderers
 * Architecture simple : l'API est la source de vérité, SSE déclenche des re-fetch
 */
import { ref, computed, type Ref } from 'vue'
import { api } from '../services/pmocontrol/api'
import { sse } from '../services/pmocontrol/sse'
import type {
  RendererSummary,
  RendererState,
  QueueSnapshot,
  AttachedPlaylistInfo
} from '../services/pmocontrol/types'

// Cache global partagé entre toutes les instances du composable
const renderersCache = ref<Map<string, RendererSummary>>(new Map())
const statesCache = ref<Map<string, RendererState>>(new Map())
const queuesCache = ref<Map<string, QueueSnapshot>>(new Map())
const bindingsCache = ref<Map<string, AttachedPlaylistInfo | null>>(new Map())

// Timestamps pour éviter les re-fetch trop fréquents
const lastFetch = {
  renderers: 0,
  states: new Map<string, number>(),
  queues: new Map<string, number>(),
  bindings: new Map<string, number>()
}

const CACHE_DURATION_MS = 2000 // 2 secondes

// Connecter SSE une seule fois au module
let sseConnected = false
function ensureSSEConnected() {
  if (sseConnected) return

  sse.onRendererEvent((event) => {
    const rendererId = event.renderer_id

    switch (event.type) {
      case 'state_changed':
      case 'position_changed':
      case 'volume_changed':
      case 'mute_changed':
      case 'metadata_changed':
        // Invalider le cache de l'état et re-fetch
        lastFetch.states.delete(rendererId)
        api.getRendererState(rendererId).then(state => {
          statesCache.value.set(rendererId, state)
        })
        break

      case 'queue_updated':
        // Invalider le cache de la queue et re-fetch
        lastFetch.queues.delete(rendererId)
        api.getQueue(rendererId).then(queue => {
          queuesCache.value.set(rendererId, queue)
        })
        // Mettre à jour queue_len dans l'état
        api.getRendererState(rendererId).then(state => {
          statesCache.value.set(rendererId, state)
        })
        break

      case 'binding_changed':
        // Re-fetch binding et queue
        lastFetch.bindings.delete(rendererId)
        lastFetch.queues.delete(rendererId)
        api.getBinding(rendererId).then(binding => {
          bindingsCache.value.set(rendererId, binding)
        })
        api.getQueue(rendererId).then(queue => {
          queuesCache.value.set(rendererId, queue)
        })
        break
    }
  })

  sseConnected = true
}

/**
 * Composable principal pour gérer les renderers
 */
export function useRenderers() {
  ensureSSEConnected()

  const loading = ref(false)
  const error = ref<string | null>(null)

  // Getters computed
  const allRenderers = computed(() => Array.from(renderersCache.value.values()))
  const onlineRenderers = computed(() => allRenderers.value.filter(r => r.online))

  const allStates = computed(() => Array.from(statesCache.value.values()))
  const playingRenderers = computed(() =>
    allStates.value.filter(s => s.transport_state === 'PLAYING')
  )

  // Fetch renderers list
  async function fetchRenderers(force = false) {
    const now = Date.now()
    if (!force && now - lastFetch.renderers < CACHE_DURATION_MS) {
      return // Cache encore valide
    }

    try {
      loading.value = true
      error.value = null
      const data = await api.getRenderers()

      renderersCache.value.clear()
      data.forEach(r => renderersCache.value.set(r.id, r))
      lastFetch.renderers = now
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur fetch renderers'
      console.error('[useRenderers] Erreur fetch:', e)
    } finally {
      loading.value = false
    }
  }

  // Fetch renderer state
  async function fetchRendererState(id: string, force = false) {
    const now = Date.now()
    const last = lastFetch.states.get(id) || 0
    if (!force && now - last < CACHE_DURATION_MS) {
      return // Cache encore valide
    }

    try {
      const state = await api.getRendererState(id)
      statesCache.value.set(id, state)
      lastFetch.states.set(id, now)
    } catch (e) {
      console.error(`[useRenderers] Erreur fetch state ${id}:`, e)
    }
  }

  // Fetch queue
  async function fetchQueue(id: string, force = false) {
    const now = Date.now()
    const last = lastFetch.queues.get(id) || 0
    if (!force && now - last < CACHE_DURATION_MS) {
      return // Cache encore valide
    }

    try {
      const queue = await api.getQueue(id)
      queuesCache.value.set(id, queue)
      lastFetch.queues.set(id, now)
    } catch (e) {
      console.error(`[useRenderers] Erreur fetch queue ${id}:`, e)
    }
  }

  // Fetch binding
  async function fetchBinding(id: string, force = false) {
    const now = Date.now()
    const last = lastFetch.bindings.get(id) || 0
    if (!force && now - last < CACHE_DURATION_MS) {
      return // Cache encore valide
    }

    try {
      const binding = await api.getBinding(id)
      bindingsCache.value.set(id, binding)
      lastFetch.bindings.set(id, now)
    } catch (e) {
      console.error(`[useRenderers] Erreur fetch binding ${id}:`, e)
    }
  }

  // Transport controls (pas de cache, juste des commandes)
  async function play(id: string) {
    await api.play(id)
    // SSE mettra à jour l'état automatiquement
  }

  async function resumeOrPlayFromQueue(id: string) {
    const state = statesCache.value.get(id)
    if (!state) {
      throw new Error(`Renderer ${id} non trouvé`)
    }

    if (state.transport_state === 'PAUSED') {
      return play(id)
    }

    if ((state.transport_state === 'STOPPED' || state.transport_state === 'NO_MEDIA') &&
        state.queue_len && state.queue_len > 0) {
      return api.resume(id)
    }

    throw new Error('La file d\'attente est vide. Ajoutez des morceaux avant de démarrer la lecture.')
  }

  async function pause(id: string) {
    await api.pause(id)
  }

  async function stop(id: string) {
    await api.stop(id)
  }

  async function next(id: string) {
    await api.next(id)
  }

  // Volume controls
  async function setVolume(id: string, volume: number) {
    await api.setVolume(id, volume)
  }

  async function volumeUp(id: string) {
    await api.volumeUp(id)
  }

  async function volumeDown(id: string) {
    await api.volumeDown(id)
  }

  async function toggleMute(id: string) {
    await api.toggleMute(id)
  }

  // Playlist binding
  async function attachPlaylist(rendererId: string, serverId: string, containerId: string) {
    await api.attachPlaylist(rendererId, serverId, containerId)
    // Re-fetch binding et queue
    await fetchBinding(rendererId, true)
    await fetchQueue(rendererId, true)
  }

  async function detachPlaylist(rendererId: string) {
    await api.detachPlaylist(rendererId)
    bindingsCache.value.set(rendererId, null)
  }

  async function attachAndPlayPlaylist(rendererId: string, serverId: string, containerId: string) {
    await api.attachPlaylist(rendererId, serverId, containerId)
    await fetchBinding(rendererId, true)
    await fetchQueue(rendererId, true)
  }

  // Queue content
  async function playContent(rendererId: string, serverId: string, objectId: string) {
    await api.playContent(rendererId, serverId, objectId)
    // SSE mettra à jour la queue
  }

  async function addToQueue(rendererId: string, serverId: string, objectId: string) {
    await api.addToQueue(rendererId, serverId, objectId)
    // SSE mettra à jour la queue
  }

  // Getters pour un renderer spécifique
  function getRendererById(id: string) {
    return renderersCache.value.get(id)
  }

  function getStateById(id: string) {
    return statesCache.value.get(id)
  }

  function getQueueById(id: string) {
    return queuesCache.value.get(id)
  }

  function getBindingById(id: string) {
    return bindingsCache.value.get(id)
  }

  return {
    // État
    loading,
    error,
    // Getters
    allRenderers,
    onlineRenderers,
    playingRenderers,
    getRendererById,
    getStateById,
    getQueueById,
    getBindingById,
    // Actions fetch
    fetchRenderers,
    fetchRendererState,
    fetchQueue,
    fetchBinding,
    // Transport controls
    play,
    resumeOrPlayFromQueue,
    pause,
    stop,
    next,
    // Volume controls
    setVolume,
    volumeUp,
    volumeDown,
    toggleMute,
    // Playlist binding
    attachPlaylist,
    detachPlaylist,
    attachAndPlayPlaylist,
    // Queue content
    playContent,
    addToQueue
  }
}

/**
 * Composable pour un renderer spécifique (avec auto-refresh)
 */
export function useRenderer(rendererId: Ref<string>) {
  ensureSSEConnected()

  const renderer = computed(() => renderersCache.value.get(rendererId.value))
  const state = computed(() => statesCache.value.get(rendererId.value))
  const queue = computed(() => queuesCache.value.get(rendererId.value))
  const binding = computed(() => bindingsCache.value.get(rendererId.value))

  // Auto-refresh au montage
  const { fetchRendererState, fetchQueue, fetchBinding } = useRenderers()

  async function refresh() {
    await Promise.all([
      fetchRendererState(rendererId.value, true),
      fetchQueue(rendererId.value, true),
      fetchBinding(rendererId.value, true)
    ])
  }

  return {
    renderer,
    state,
    queue,
    binding,
    refresh
  }
}
