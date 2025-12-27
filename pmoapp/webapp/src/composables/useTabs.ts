/**
 * Composable pour gérer les onglets dynamiques de l'interface unifiée.
 * Onglets renderer auto-générés depuis la liste des renderers online.
 * Onglets server ouverts manuellement via le drawer (fermables).
 */
import { reactive, computed, watch, onMounted, type Component } from 'vue'
import { Radio, Server } from 'lucide-vue-next'
import type { RendererSummary, MediaServerSummary } from '../services/pmocontrol/types'

export interface Tab {
  id: string // "renderer-{id}", "server-{id}"
  type: 'renderer' | 'server'
  title: string // Nom affiché (tronqué sur mobile)
  fullTitle: string // Nom complet (pour tooltip)
  icon: Component
  metadata?: {
    rendererId?: string
    serverId?: string
  }
  closeable: boolean // renderer: false (auto-géré), server: true (manuel)
}

interface TabsState {
  tabs: Tab[]
  activeTabId: string
  tabHistory: string[] // Pour back/forward navigation
}

const MAX_TABS = 12 // Augmenté car onglets auto-générés
const STORAGE_KEY = 'pmo-tabs-state'
const COMPACT_MODE_THRESHOLD = 5 // Passer en mode icônes seulement au-delà de 5 onglets

// État global partagé entre toutes les instances du composable
const state = reactive<TabsState>({
  tabs: [],
  activeTabId: '',
  tabHistory: [],
})

// Flag pour éviter les boucles de sauvegarde
let isRestoringFromStorage = false

/**
 * Tronque un titre pour mobile si nécessaire
 */
function truncateTitle(title: string, maxLength = 15): string {
  return title.length > maxLength ? title.slice(0, maxLength) + '...' : title
}

/**
 * Sauvegarde l'état dans localStorage
 * Note: On ne sauvegarde que les onglets server (les renderer tabs sont auto-générés)
 */
function saveToLocalStorage() {
  if (isRestoringFromStorage) return

  try {
    const stateToSave = {
      // Sauvegarder uniquement les onglets server (fermables manuellement)
      tabs: state.tabs
        .filter((tab) => tab.type === 'server')
        .map((tab) => ({
          ...tab,
          // On ne peut pas sauvegarder les composants Vue, on sauve juste le type
          icon: undefined,
        })),
      activeTabId: state.activeTabId,
      tabHistory: state.tabHistory,
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stateToSave))
  } catch (error) {
    console.error('[useTabs] Erreur sauvegarde localStorage:', error)
  }
}

/**
 * Restaure l'état depuis localStorage
 * Note: Restaure uniquement les onglets server (les renderer tabs seront auto-générés)
 */
function restoreFromLocalStorage() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (!saved) return

    isRestoringFromStorage = true
    const savedState = JSON.parse(saved)

    // Reconstituer uniquement les tabs server avec les bonnes icônes
    const serverTabs = (savedState.tabs || [])
      .filter((tab: Tab) => tab.type === 'server')
      .map((tab: Tab) => ({
        ...tab,
        icon: Server,
        closeable: true,
        fullTitle: tab.fullTitle || tab.title, // Fallback si fullTitle n'existe pas
      }))

    // Ajouter les tabs server restaurés (les renderer tabs seront ajoutés par syncWithRenderers)
    state.tabs.push(...serverTabs)

    state.activeTabId = savedState.activeTabId || ''
    state.tabHistory = savedState.tabHistory || []

    // Vérifier que l'onglet actif existe toujours (sera validé après syncWithRenderers)
    if (!state.tabs.find((t) => t.id === state.activeTabId)) {
      state.activeTabId = ''
    }

    isRestoringFromStorage = false
  } catch (error) {
    console.error('[useTabs] Erreur restauration localStorage:', error)
    isRestoringFromStorage = false
  }
}

/**
 * Trouve un onglet par son ID
 */
function findTab(tabId: string): Tab | undefined {
  return state.tabs.find((t) => t.id === tabId)
}

/**
 * Ouvre un nouvel onglet ou active un onglet existant
 */
function openTab(newTab: Omit<Tab, 'id' | 'fullTitle'> & { id?: string; fullTitle?: string }): string {
  // Générer un ID si non fourni
  const tabId =
    newTab.id ||
    (newTab.type === 'renderer'
      ? `renderer-${newTab.metadata?.rendererId}`
      : `server-${newTab.metadata?.serverId}`)

  // Si l'onglet existe déjà, on le sélectionne
  const existingTab = findTab(tabId)
  if (existingTab) {
    switchTab(tabId)
    return tabId
  }

  // Vérifier la limite max
  if (state.tabs.length >= MAX_TABS) {
    console.warn(`[useTabs] Limite max de ${MAX_TABS} onglets atteinte`)
    return state.activeTabId
  }

  // Créer le nouvel onglet
  const tab: Tab = {
    id: tabId,
    type: newTab.type,
    title: truncateTitle(newTab.title),
    fullTitle: newTab.fullTitle || newTab.title, // Utiliser fullTitle si fourni, sinon title
    icon: newTab.icon,
    metadata: newTab.metadata,
    closeable: newTab.closeable !== false, // true par défaut sauf si explicitement false
  }

  state.tabs.push(tab)
  switchTab(tabId)

  return tabId
}

/**
 * Ferme un onglet (uniquement les onglets server manuels)
 * Les onglets renderer sont auto-gérés et ne peuvent pas être fermés manuellement
 */
function closeTab(tabId: string) {
  const tab = findTab(tabId)
  if (!tab) return

  // Ne fermer que les onglets server (closeable = true)
  // Les renderer tabs sont auto-gérés par syncWithRenderers
  if (!tab.closeable || tab.type === 'renderer') {
    console.warn('[useTabs] Impossible de fermer un onglet renderer (auto-géré)')
    return
  }

  const tabIndex = state.tabs.findIndex((t) => t.id === tabId)
  if (tabIndex === -1) return

  // Supprimer l'onglet
  state.tabs.splice(tabIndex, 1)

  // Supprimer de l'historique
  state.tabHistory = state.tabHistory.filter((id) => id !== tabId)

  // Si c'était l'onglet actif, basculer vers le précédent dans l'historique
  if (state.activeTabId === tabId) {
    // Chercher le précédent dans l'historique qui existe encore
    const previousTab = state.tabHistory
      .slice()
      .reverse()
      .find((id) => state.tabs.some((t) => t.id === id))

    if (previousTab) {
      switchTab(previousTab)
    } else {
      // Fallback sur le premier onglet disponible (ou vide)
      const firstTab = state.tabs[0]
      if (firstTab) {
        switchTab(firstTab.id)
      } else {
        state.activeTabId = ''
      }
    }
  }
}

/**
 * Change l'onglet actif
 */
function switchTab(tabId: string) {
  const tab = findTab(tabId)
  if (!tab) {
    console.warn(`[useTabs] Onglet ${tabId} introuvable`)
    return
  }

  state.activeTabId = tabId

  // Ajouter à l'historique (en évitant les doublons consécutifs)
  if (state.tabHistory[state.tabHistory.length - 1] !== tabId) {
    state.tabHistory.push(tabId)

    // Limiter la taille de l'historique
    if (state.tabHistory.length > 20) {
      state.tabHistory.shift()
    }
  }
}

/**
 * Onglet suivant (pour swipe gesture)
 */
function nextTab() {
  const currentIndex = state.tabs.findIndex((t) => t.id === state.activeTabId)
  const nextIndex = (currentIndex + 1) % state.tabs.length
  const nextTabObj = state.tabs[nextIndex]
  if (nextTabObj) switchTab(nextTabObj.id)
}

/**
 * Onglet précédent (pour swipe gesture)
 */
function previousTab() {
  const currentIndex = state.tabs.findIndex((t) => t.id === state.activeTabId)
  const previousIndex = currentIndex === 0 ? state.tabs.length - 1 : currentIndex - 1
  const prevTabObj = state.tabs[previousIndex]
  if (prevTabObj) switchTab(prevTabObj.id)
}

/**
 * Synchronise les onglets avec la liste des renderers online
 * Les renderer tabs sont automatiquement créés/supprimés selon l'état online
 */
function syncWithRenderers(renderers: RendererSummary[]) {
  const onlineRenderers = renderers.filter((r) => r.online)

  // Extraire les tabs renderer actuels
  const currentRendererTabs = state.tabs.filter((t) => t.type === 'renderer')

  // IDs des renderers online
  const onlineRendererIds = new Set(onlineRenderers.map((r) => `renderer-${r.id}`))

  // Supprimer les tabs des renderers qui ne sont plus online
  const renderersToRemove = currentRendererTabs.filter((t) => !onlineRendererIds.has(t.id))
  renderersToRemove.forEach((tab) => {
    const index = state.tabs.findIndex((t) => t.id === tab.id)
    if (index !== -1) state.tabs.splice(index, 1)
    state.tabHistory = state.tabHistory.filter((id) => id !== tab.id)
  })

  // Ajouter les nouveaux renderers online
  const currentRendererIds = new Set(currentRendererTabs.map((t) => t.id))
  onlineRenderers.forEach((renderer) => {
    const tabId = `renderer-${renderer.id}`
    if (!currentRendererIds.has(tabId)) {
      const newTab: Tab = {
        id: tabId,
        type: 'renderer',
        title: truncateTitle(renderer.friendly_name),
        fullTitle: renderer.friendly_name,
        icon: Radio,
        metadata: { rendererId: renderer.id },
        closeable: false, // renderer tabs ne sont pas fermables manuellement
      }
      state.tabs.unshift(newTab) // Ajouter au début
    }
  })

  // Vérifier que l'onglet actif existe toujours
  if (state.activeTabId && !state.tabs.find((t) => t.id === state.activeTabId)) {
    // Basculer vers le premier onglet disponible
    const firstTab = state.tabs[0]
    if (firstTab) {
      state.activeTabId = firstTab.id
    } else {
      state.activeTabId = ''
    }
  }

  // Si aucun onglet actif et qu'il y a des onglets, sélectionner le premier
  if (!state.activeTabId && state.tabs.length > 0) {
    const firstTab = state.tabs[0]
    if (firstTab) {
      state.activeTabId = firstTab.id
    }
  }
}

/**
 * Ouvre un onglet server (manuel)
 */
function openServer(server: MediaServerSummary | undefined) {
  if (!server) return ''

  return openTab({
    type: 'server',
    title: server.friendly_name,
    icon: Server,
    metadata: { serverId: server.id },
    closeable: true,
  })
}

/**
 * Composable principal
 */
export function useTabs() {
  // Watch pour sauvegarde automatique (uniquement les server tabs)
  watch(
    () => [state.tabs, state.activeTabId, state.tabHistory],
    () => {
      saveToLocalStorage()
    },
    { deep: true },
  )

  // Restaurer au montage (uniquement les server tabs)
  onMounted(() => {
    // Restaurer seulement si pas déjà fait
    if (state.tabs.filter((t) => t.type === 'server').length === 0) {
      restoreFromLocalStorage()
    }
  })

  // Computed properties
  const activeTab = computed(() => findTab(state.activeTabId) || state.tabs[0] || null)
  const hasMultipleTabs = computed(() => state.tabs.length > 1)
  const canAddTab = computed(() => state.tabs.length < MAX_TABS)
  const isEmpty = computed(() => state.tabs.length === 0)
  const compactMode = computed(() => state.tabs.length > COMPACT_MODE_THRESHOLD)

  return {
    // State
    tabs: computed(() => state.tabs),
    activeTabId: computed(() => state.activeTabId),
    activeTab,
    tabHistory: computed(() => state.tabHistory),
    hasMultipleTabs,
    canAddTab,
    isEmpty,
    compactMode,

    // Actions
    syncWithRenderers, // Nouvelle fonction clé pour auto-sync
    openTab,
    closeTab,
    switchTab,
    nextTab,
    previousTab,
    openServer,
    findTab,
  }
}
