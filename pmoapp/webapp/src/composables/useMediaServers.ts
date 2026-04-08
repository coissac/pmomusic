/**
 * Composable pour gérer les media servers
 * Architecture simple : l'API est la source de vérité, SSE invalide le cache
 */
import { ref, computed } from 'vue'
import { api } from '../services/pmocontrol/api'
import { useSSE } from './useSSE'
import { apiCache } from './apiCache'
import type {
  MediaServerSummary,
  ContainerEntry,
} from '../services/pmocontrol/types'

export interface BreadcrumbItem {
  id: string
  title: string
}

export interface BrowseState {
  container_id: string
  entries: ContainerEntry[]
  total_count: number
  hasMore?: boolean
  currentOffset?: number
}

// Cache global partagé
const serversCache = ref<Map<string, MediaServerSummary>>(new Map())
const browseCache = ref<Map<string, BrowseState>>(new Map())

function browseCacheKey(serverId: string, containerId: string): string {
  return `${encodeURIComponent(serverId)}:${encodeURIComponent(containerId)}`
}
const currentPath = ref<BreadcrumbItem[]>([])
const searchResults = ref<BrowseState | null>(null)
const searchQuery = ref<string>('')

const CACHE_DURATION_MS = 2000
const BROWSE_WINDOW_SIZE = 200

// Initialiser SSE une seule fois via le composable centralisé
let sseInitialized = false
function ensureSSEInitialized() {
  if (sseInitialized) return

  const { onMediaServerEvent, connect } = useSSE()
  
  // Démarrer la connexion SSE
  connect()

  onMediaServerEvent((event) => {
    const serverId = event.server_id

    switch (event.type) {
      case 'online':
        // Ajouter au cache avec les infos disponibles
        const server: MediaServerSummary = {
          id: serverId,
          friendly_name: event.friendly_name,
          model_name: event.model_name,
          online: true,
        }
        serversCache.value.set(serverId, server)

        // Fetch la liste complète pour avoir les bonnes infos
        // On ne le fait pas ici car on pourrait déclencher trop de requêtes
        // La liste se mettra à jour au prochain refresh automatique
        break

      case 'offline':
        // Marquer comme offline dans le cache
        const existingServer = serversCache.value.get(serverId)
        if (existingServer) {
          existingServer.online = false
          serversCache.value.set(serverId, existingServer)
        }

        // Invalider tout le cache browse de ce serveur
        const encodedServerId = encodeURIComponent(serverId)
        const keysToDelete: string[] = []
        browseCache.value.forEach((_, key) => {
          if (key.startsWith(encodedServerId + ':')) {
            keysToDelete.push(key)
          }
        })
        keysToDelete.forEach(key => browseCache.value.delete(key))
        break

      case 'global_updated':
        // Invalider tout le cache de ce serveur
        const encodedServerIdGlobal = encodeURIComponent(serverId)
        const globalKeysToDelete: string[] = []
        browseCache.value.forEach((_, key) => {
          if (key.startsWith(encodedServerIdGlobal + ':')) {
            globalKeysToDelete.push(key)
          }
        })
        globalKeysToDelete.forEach(key => browseCache.value.delete(key))
        break

      case 'containers_updated':
        // Invalider les containers spécifiques
        event.container_ids.forEach(containerId => {
          const key = browseCacheKey(serverId, containerId)
          browseCache.value.delete(key)
        })
        break
    }
  })

  sseInitialized = true
}

/**
 * Composable principal pour gérer les media servers
 */
export function useMediaServers() {
  ensureSSEInitialized()

  const loading = ref(false)
  const loadingMore = ref(false)
  const error = ref<string | null>(null)

  // Getters computed
  const allServers = computed(() => Array.from(serversCache.value.values()))
  const onlineServers = computed(() => allServers.value.filter(s => s.online))

  // Fetch servers list
  async function fetchServers(force = false) {
    try {
      loading.value = true
      error.value = null

      // Utiliser le cache API centralisé
      const data = await apiCache.fetch(
        '/servers',
        undefined,
        () => api.getServers(),
        { force, ttl: CACHE_DURATION_MS }
      )

      serversCache.value.clear()
      data.forEach(s => serversCache.value.set(s.id, s))
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur fetch servers'
      console.error('[useMediaServers] Erreur fetch:', e)
    } finally {
      loading.value = false
    }
  }

  // Charge la première page (remplace le cache)
  async function browseContainer(serverId: string, containerId: string, useCache = true) {
    const key = browseCacheKey(serverId, containerId)

    if (useCache && browseCache.value.has(key)) {
      return browseCache.value.get(key)!
    }

    try {
      loading.value = true
      error.value = null

      const data = await api.browseContainer(serverId, containerId, 0, 50)

      browseCache.value.set(key, {
        container_id: data.container_id,
        entries: data.entries,
        total_count: data.total_count,
        hasMore: data.entries.length < data.total_count,
        currentOffset: data.entries.length,
      })

      return browseCache.value.get(key)!
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur browse container'
      console.error(`[useMediaServers] Erreur browse ${serverId}/${containerId}:`, e)
      throw e
    } finally {
      loading.value = false
    }
  }

  // Charge la page suivante et accumule (infinite scroll)
  async function loadMoreBrowse(serverId: string, containerId: string) {
    const key = browseCacheKey(serverId, containerId)
    const state = browseCache.value.get(key)

    if (!state) return
    if (!('hasMore' in state) || !state.hasMore) return
    if (loadingMore.value) return

    try {
      loadingMore.value = true

      // Le type cast est nécessaire car les anciens cached entries n'ont pas hasMore
      const state = browseCache.value.get(key) as BrowseState & { hasMore?: boolean; currentOffset?: number }
      const offset = state.currentOffset ?? state.entries.length
      const data = await api.browseContainer(serverId, containerId, offset)

      // Accumuler les nouvelles entrées avec fenêtre glissante
      const combined = [...state.entries, ...data.entries]
      // Ne garder que les últimos BROWSE_WINDOW_SIZE items
      state.entries = combined.slice(-BROWSE_WINDOW_SIZE)
      state.total_count = data.total_count
      state.currentOffset = (state.currentOffset ?? 0) + data.entries.length
      state.hasMore = state.currentOffset < state.total_count
      // Forcer la réactivité
      browseCache.value.set(key, { ...state })
    } catch (e) {
      console.error(`[useMediaServers] Erreur load more ${serverId}/${containerId}:`, e)
    } finally {
      loadingMore.value = false
    }
  }

  // Recherche dans un serveur
  async function searchServer(serverId: string, query: string) {
    console.log(`[useMediaServers] searchServer called: serverId=${serverId}, query=${query}`);
    if (!query.trim()) {
      searchResults.value = null
      searchQuery.value = ''
      return
    }

    try {
      loading.value = true
      error.value = null
      searchQuery.value = query
      console.log(`[useMediaServers] Calling API searchServer for server ${serverId}`);

      const data = await api.searchServer(serverId, query)
      console.log(`[useMediaServers] Search returned ${data.entries.length} entries, total=${data.total_count}`);
      searchResults.value = {
        container_id: 'search',
        entries: data.entries,
        total_count: data.total_count,
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur recherche'
      console.error(`[useMediaServers] Erreur search ${serverId}:`, e)
      throw e
    } finally {
      loading.value = false
    }
  }

  function clearSearch() {
    searchResults.value = null
    searchQuery.value = ''
  }

  // Getters
  function getServerById(id: string) {
    return serversCache.value.get(id)
  }

  function getBrowseCached(serverId: string, containerId: string) {
    const key = browseCacheKey(serverId, containerId)
    return browseCache.value.get(key)
  }

  function hasMore(serverId: string, containerId: string): boolean {
    const key = browseCacheKey(serverId, containerId)
    const state = browseCache.value.get(key)
    if (!state) return false
    return state.entries.length < state.total_count
  }

  // Breadcrumb path management
  function setPath(path: BreadcrumbItem[]) {
    currentPath.value = path
  }

  function clearPath() {
    currentPath.value = []
  }

  // Invalidation du cache
  function invalidateCache(serverId: string, containerId?: string) {
    if (containerId) {
      browseCache.value.delete(browseCacheKey(serverId, containerId))
    } else {
      const encodedServerId = encodeURIComponent(serverId)
      const keysToDelete: string[] = []
      browseCache.value.forEach((_, key) => {
        if (key.startsWith(encodedServerId + ':')) {
          keysToDelete.push(key)
        }
      })
      keysToDelete.forEach(key => browseCache.value.delete(key))
    }
  }

  return {
    // État
    loading,
    loadingMore,
    error,
    currentPath,
    // Getters
    allServers,
    onlineServers,
    getServerById,
    getBrowseCached,
    hasMore,
    // Search
    searchResults,
    searchQuery,
    searchServer,
    clearSearch,
    // Actions
    fetchServers,
    browseContainer,
    loadMoreBrowse,
    setPath,
    clearPath,
    invalidateCache
  }
}
