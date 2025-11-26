/**
 * Service pour interagir avec l'API pmosource générique
 *
 * Ce service utilise uniquement l'API REST définie dans pmosource::api
 * et ne dépend d'aucune implémentation spécifique (comme pmoparadise)
 */

const API_BASE = '/api/sources'

// Types correspondant aux structures de l'API pmosource

export interface SourceInfo {
  id: string
  name: string
  supports_fifo: boolean
  capabilities: SourceCapabilities
}

export interface SourceCapabilities {
  supports_search: boolean
  supports_favorites: boolean
  supports_playlists: boolean
  supports_user_content: boolean
  supports_high_res_audio: boolean
  max_sample_rate: number | null
  supports_multiple_formats: boolean
  supports_advanced_search: boolean
  supports_pagination: boolean
}

export interface SourcesList {
  count: number
  sources: SourceInfo[]
}

export interface BrowseContainer {
  id: string
  parent_id: string
  title: string
  class: string
  child_count: string | null
  restricted: string | null
}

export interface BrowseItemResource {
  url: string
  protocol_info: string
  duration: string | null
}

export interface BrowseItem {
  id: string
  parent_id: string
  title: string
  class: string
  artist: string | null
  album: string | null
  creator: string | null
  album_art: string | null
  resources: BrowseItemResource[]
}

export interface BrowseResponse {
  object_id: string
  containers: BrowseContainer[]
  items: BrowseItem[]
  returned_containers: number
  returned_items: number
  total: number
  update_id: number
}

export interface ResolveUriResponse {
  object_id: string
  uri: string
}

export interface SourceRootContainer {
  id: string
  parent_id: string
  title: string
  class: string
  child_count: string | null
  searchable: string | null
}

/**
 * Liste toutes les sources musicales enregistrées
 */
export async function listSources(): Promise<SourcesList> {
  const response = await fetch(`${API_BASE}`)
  if (!response.ok) {
    throw new Error(`Failed to list sources: ${response.status} ${response.statusText}`)
  }
  return response.json()
}

/**
 * Récupère les informations d'une source spécifique
 */
export async function getSource(sourceId: string): Promise<SourceInfo> {
  const response = await fetch(`${API_BASE}/${sourceId}`)
  if (!response.ok) {
    throw new Error(`Failed to get source: ${response.status} ${response.statusText}`)
  }
  return response.json()
}

/**
 * Récupère le container racine d'une source
 */
export async function getSourceRoot(sourceId: string): Promise<SourceRootContainer> {
  const response = await fetch(`${API_BASE}/${sourceId}/root`)
  if (!response.ok) {
    throw new Error(`Failed to get source root: ${response.status} ${response.statusText}`)
  }
  return response.json()
}

/**
 * Parcourt un container d'une source
 *
 * @param sourceId - ID de la source
 * @param objectId - ID de l'objet à parcourir (optionnel, par défaut utilise la racine)
 * @param startingIndex - Index de départ pour la pagination
 * @param requestedCount - Nombre d'éléments demandés
 */
export async function browseSource(
  sourceId: string,
  objectId?: string,
  startingIndex?: number,
  requestedCount?: number
): Promise<BrowseResponse> {
  const params = new URLSearchParams()
  if (objectId) params.set('object_id', objectId)
  if (startingIndex !== undefined) params.set('starting_index', startingIndex.toString())
  if (requestedCount !== undefined) params.set('requested_count', requestedCount.toString())

  const url = `${API_BASE}/${sourceId}/browse?${params.toString()}`
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`Failed to browse source: ${response.status} ${response.statusText}`)
  }
  return response.json()
}

/**
 * Résout l'URI réelle d'un objet (pour le streaming)
 *
 * @param sourceId - ID de la source
 * @param objectId - ID de l'objet à résoudre
 */
export async function resolveUri(sourceId: string, objectId: string): Promise<ResolveUriResponse> {
  const params = new URLSearchParams({ object_id: objectId })
  const url = `${API_BASE}/${sourceId}/resolve?${params.toString()}`

  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`Failed to resolve URI: ${response.status} ${response.statusText}`)
  }
  return response.json()
}

/**
 * Récupère l'URL de l'image par défaut d'une source
 *
 * @param sourceId - ID de la source
 * @returns L'URL de l'image
 */
export function getSourceImageUrl(sourceId: string): string {
  return `${API_BASE}/${sourceId}/image`
}

/**
 * Récupère les capacités d'une source
 */
export async function getSourceCapabilities(sourceId: string): Promise<SourceCapabilities> {
  const response = await fetch(`${API_BASE}/${sourceId}/capabilities`)
  if (!response.ok) {
    throw new Error(`Failed to get source capabilities: ${response.status} ${response.statusText}`)
  }
  return response.json()
}

/**
 * Récupère les métadonnées détaillées d'un item spécifique
 *
 * @param sourceId - ID de la source
 * @param objectId - ID de l'item à récupérer
 */
export async function getItem(sourceId: string, objectId: string): Promise<BrowseItem> {
  const params = new URLSearchParams({ object_id: objectId })
  const url = `${API_BASE}/${sourceId}/item?${params.toString()}`

  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`Failed to get item: ${response.status} ${response.statusText}`)
  }
  return response.json()
}
