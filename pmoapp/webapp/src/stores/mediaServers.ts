// Store Pinia pour les media servers
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '../services/pmocontrol/api'
import type {
  MediaServerSummary,
  BrowseResponse,
  MediaServerEventPayload
} from '../services/pmocontrol/types'

export interface BreadcrumbItem {
  id: string
  title: string
}

export const useMediaServersStore = defineStore('mediaServers', () => {
  // État
  const servers = ref<Map<string, MediaServerSummary>>(new Map())
  const browseCache = ref<Map<string, BrowseResponse>>(new Map())
  const currentPath = ref<BreadcrumbItem[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  // Getters
  const onlineServers = computed(() => {
    return Array.from(servers.value.values()).filter(s => s.online)
  })

  const getServerById = (id: string) => {
    return servers.value.get(id)
  }

  const getBrowseCached = (serverId: string, containerId: string) => {
    const key = `${serverId}/${containerId}`
    return browseCache.value.get(key)
  }

  // Actions
  async function fetchServers() {
    try {
      loading.value = true
      error.value = null
      const data = await api.getServers()

      // Mettre à jour le Map
      servers.value.clear()
      data.forEach(s => servers.value.set(s.id, s))
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur fetch servers'
      console.error('[Store MediaServers] Erreur fetch:', e)
    } finally {
      loading.value = false
    }
  }

  async function browseContainer(serverId: string, containerId: string) {
    try {
      loading.value = true
      error.value = null

      const data = await api.browseContainer(serverId, containerId)

      // Mettre en cache
      const key = `${serverId}/${containerId}`
      browseCache.value.set(key, data)

      return data
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Erreur browse container'
      console.error(`[Store MediaServers] Erreur browse ${serverId}/${containerId}:`, e)
      throw e
    } finally {
      loading.value = false
    }
  }

  function setPath(path: BreadcrumbItem[]) {
    currentPath.value = path
  }

  function clearPath() {
    currentPath.value = []
  }

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

  // SSE event handling
  function updateFromSSE(event: MediaServerEventPayload) {
    const serverId = event.server_id

    switch (event.type) {
      case 'global_updated': {
        // Invalider tout le cache de ce serveur
        console.log(`[Store MediaServers] GlobalUpdated pour ${serverId}`)
        invalidateCache(serverId)
        break
      }

      case 'containers_updated': {
        // Invalider les containers spécifiques
        console.log(`[Store MediaServers] ContainersUpdated pour ${serverId}:`, event.container_ids)
        event.container_ids.forEach(containerId => {
          invalidateCache(serverId, containerId)
        })
        break
      }
    }
  }

  return {
    // État
    servers,
    browseCache,
    currentPath,
    loading,
    error,
    // Getters
    onlineServers,
    getServerById,
    getBrowseCached,
    // Actions
    fetchServers,
    browseContainer,
    setPath,
    clearPath,
    invalidateCache,
    updateFromSSE,
  }
})
