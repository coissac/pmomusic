/**
 * Composable pour gérer les media servers
 * Architecture simple : l'API est la source de vérité, SSE invalide le cache
 */
import { ref, computed } from 'vue'
import { api } from '../services/pmocontrol/api'
import { sse } from '../services/pmocontrol/sse'
import type {
  MediaServerSummary,
  BrowseResponse
} from '../services/pmocontrol/types'

export interface BreadcrumbItem {
  id: string
  title: string
}

// Cache global partagé
const serversCache = ref<Map<string, MediaServerSummary>>(new Map())
const browseCache = ref<Map<string, BrowseResponse>>(new Map())
const currentPath = ref<BreadcrumbItem[]>([])

// Timestamps
const lastFetch = {
  servers: 0
}

const CACHE_DURATION_MS = 2000

// Connecter SSE une seule fois
let sseConnected = false
function ensureSSEConnected() {
  if (sseConnected) return

  sse.onMediaServerEvent((event) => {
    const serverId = event.server_id

    switch (event.type) {
      case 'global_updated':
        // Invalider tout le cache de ce serveur
        console.log(`[useMediaServers] GlobalUpdated pour ${serverId}`)
        const keysToDelete: string[] = []
        browseCache.value.forEach((_, key) => {
          if (key.startsWith(serverId + '/')) {
            keysToDelete.push(key)
          }
        })
        keysToDelete.forEach(key => browseCache.value.delete(key))
        break

      case 'containers_updated':
        // Invalider les containers spécifiques
        console.log(`[useMediaServers] ContainersUpdated pour ${serverId}:`, event.container_ids)
        event.container_ids.forEach(containerId => {
          const key = `${serverId}/${containerId}`
          browseCache.value.delete(key)
        })
        break
    }
  })

  sseConnected = true
}

/**
 * Composable principal pour gérer les media servers
 */
export function useMediaServers() {
  ensureSSEConnected()

  const loading = ref(false)
  const error = ref<string | null>(null)

  // Getters computed
  const allServers = computed(() => Array.from(serversCache.value.values()))
  const onlineServers = computed(() => allServers.value.filter(s => s.online))

  // Fetch servers list
  async function fetchServers(force = false) {
    const now = Date.now()
    if (!force && now - lastFetch.servers < CACHE_DURATION_MS) {
      return // Cache encore valide
    }

    try {
      loading.value = true
      error.value = null
      const data = await api.getServers()

      serversCache.value.clear()
      data.forEach(s => serversCache.value.set(s.id, s))
      lastFetch.servers = now
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur fetch servers'
      console.error('[useMediaServers] Erreur fetch:', e)
    } finally {
      loading.value = false
    }
  }

  // Browse container (avec cache automatique)
  async function browseContainer(serverId: string, containerId: string, useCache = true) {
    const key = `${serverId}/${containerId}`

    // Vérifier le cache
    if (useCache && browseCache.value.has(key)) {
      return browseCache.value.get(key)!
    }

    try {
      loading.value = true
      error.value = null

      const data = await api.browseContainer(serverId, containerId)

      // Mettre en cache
      browseCache.value.set(key, data)

      return data
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur browse container'
      console.error(`[useMediaServers] Erreur browse ${serverId}/${containerId}:`, e)
      throw e
    } finally {
      loading.value = false
    }
  }

  // Getters
  function getServerById(id: string) {
    return serversCache.value.get(id)
  }

  function getBrowseCached(serverId: string, containerId: string) {
    const key = `${serverId}/${containerId}`
    return browseCache.value.get(key)
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
      // Invalider un container spécifique
      const key = `${serverId}/${containerId}`
      browseCache.value.delete(key)
    } else {
      // Invalider tous les containers d'un serveur
      const keysToDelete: string[] = []
      browseCache.value.forEach((_, key) => {
        if (key.startsWith(serverId + '/')) {
          keysToDelete.push(key)
        }
      })
      keysToDelete.forEach(key => browseCache.value.delete(key))
    }
  }

  return {
    // État
    loading,
    error,
    currentPath,
    // Getters
    allServers,
    onlineServers,
    getServerById,
    getBrowseCached,
    // Actions
    fetchServers,
    browseContainer,
    setPath,
    clearPath,
    invalidateCache
  }
}
