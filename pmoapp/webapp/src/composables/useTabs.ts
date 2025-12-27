/**
 * Composable pour gérer les onglets dynamiques de l'interface unifiée.
 * Supporte les onglets home, renderer et server avec persistance localStorage.
 */
import { reactive, computed, watch, onMounted, type Component } from 'vue'
import { Home, Radio, Server } from 'lucide-vue-next'
import type { RendererSummary, MediaServerSummary } from '../services/pmocontrol/types'

export interface Tab {
  id: string // "home", "renderer-{id}", "server-{id}"
  type: 'home' | 'renderer' | 'server'
  title: string // Nom affiché (tronqué sur mobile)
  icon: Component
  metadata?: {
    rendererId?: string
    serverId?: string
  }
  closeable: boolean // home tab non fermable
}

interface TabsState {
  tabs: Tab[]
  activeTabId: string
  tabHistory: string[] // Pour back/forward navigation
}

const MAX_TABS = 8
const STORAGE_KEY = 'pmo-tabs-state'

// État global partagé entre toutes les instances du composable
const state = reactive<TabsState>({
  tabs: [
    {
      id: 'home',
      type: 'home',
      title: 'Home',
      icon: Home,
      closeable: false,
    },
  ],
  activeTabId: 'home',
  tabHistory: ['home'],
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
 */
function saveToLocalStorage() {
  if (isRestoringFromStorage) return

  try {
    const stateToSave = {
      tabs: state.tabs.map((tab) => ({
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
 */
function restoreFromLocalStorage() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (!saved) return

    isRestoringFromStorage = true
    const savedState = JSON.parse(saved)

    // Reconstituer les tabs avec les bonnes icônes
    state.tabs = savedState.tabs.map((tab: Tab) => ({
      ...tab,
      icon: tab.type === 'home' ? Home : tab.type === 'renderer' ? Radio : Server,
    }))

    state.activeTabId = savedState.activeTabId || 'home'
    state.tabHistory = savedState.tabHistory || ['home']

    // Vérifier que l'onglet actif existe toujours
    if (!state.tabs.find((t) => t.id === state.activeTabId)) {
      state.activeTabId = 'home'
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
function openTab(newTab: Omit<Tab, 'id'> & { id?: string }): string {
  // Générer un ID si non fourni
  const tabId =
    newTab.id ||
    (newTab.type === 'home'
      ? 'home'
      : newTab.type === 'renderer'
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
    icon: newTab.icon,
    metadata: newTab.metadata,
    closeable: newTab.closeable !== false, // true par défaut sauf si explicitement false
  }

  state.tabs.push(tab)
  switchTab(tabId)

  return tabId
}

/**
 * Ferme un onglet
 */
function closeTab(tabId: string) {
  const tab = findTab(tabId)
  if (!tab) return

  // Ne pas fermer l'onglet home
  if (!tab.closeable) {
    console.warn('[useTabs] Impossible de fermer un onglet non fermable')
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
      // Fallback sur home
      switchTab('home')
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
 * Ouvre un onglet renderer
 */
function openRenderer(renderer: RendererSummary | undefined) {
  if (!renderer) return 'home'

  return openTab({
    type: 'renderer',
    title: renderer.friendly_name,
    icon: Radio,
    metadata: { rendererId: renderer.id },
    closeable: true,
  })
}

/**
 * Ouvre un onglet server
 */
function openServer(server: MediaServerSummary | undefined) {
  if (!server) return 'home'

  return openTab({
    type: 'server',
    title: server.friendly_name,
    icon: Server,
    metadata: { serverId: server.id },
    closeable: true,
  })
}

/**
 * Ferme tous les onglets sauf home
 */
function closeAllTabs() {
  state.tabs = state.tabs.filter((t) => !t.closeable)
  state.activeTabId = 'home'
  state.tabHistory = ['home']
}

/**
 * Ferme tous les onglets sauf l'actif
 */
function closeOtherTabs() {
  const activeTab = findTab(state.activeTabId)
  if (!activeTab) return

  state.tabs = state.tabs.filter((t) => !t.closeable || t.id === state.activeTabId)
  state.tabHistory = state.tabHistory.filter((id) => state.tabs.some((t) => t.id === id))
}

/**
 * Composable principal
 */
export function useTabs() {
  // Watch pour sauvegarde automatique
  watch(
    () => [state.tabs, state.activeTabId, state.tabHistory],
    () => {
      saveToLocalStorage()
    },
    { deep: true },
  )

  // Restaurer au montage (une seule fois)
  onMounted(() => {
    const firstTab = state.tabs[0]
    if (state.tabs.length === 1 && firstTab && firstTab.id === 'home') {
      restoreFromLocalStorage()
    }
  })

  // Computed properties
  const activeTab = computed(() => findTab(state.activeTabId) || state.tabs[0] || null)
  const hasMultipleTabs = computed(() => state.tabs.length > 1)
  const canAddTab = computed(() => state.tabs.length < MAX_TABS)

  return {
    // State
    tabs: computed(() => state.tabs),
    activeTabId: computed(() => state.activeTabId),
    activeTab,
    tabHistory: computed(() => state.tabHistory),
    hasMultipleTabs,
    canAddTab,

    // Actions
    openTab,
    closeTab,
    switchTab,
    nextTab,
    previousTab,
    openRenderer,
    openServer,
    closeAllTabs,
    closeOtherTabs,
    findTab,
  }
}
