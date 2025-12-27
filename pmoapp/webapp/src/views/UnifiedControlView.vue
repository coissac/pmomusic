<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useTabs } from '@/composables/useTabs'
import { useRenderers } from '@/composables/useRenderers'
import { useMediaServers } from '@/composables/useMediaServers'
import { useSwipe } from '@vueuse/core'
import { api } from '@/services/pmocontrol/api'
import type { ContainerEntry } from '@/services/pmocontrol/types'

// Import des composants
import BottomTabBar from '@/components/unified/BottomTabBar.vue'
import EmptyState from '@/components/unified/EmptyState.vue'
import ServerDrawer from '@/components/unified/ServerDrawer.vue'
import RendererTabContent from '@/components/unified/RendererTabContent.vue'
import ServerTabContent from '@/components/unified/ServerTabContent.vue'

const route = useRoute()
const router = useRouter()
const { tabs, activeTabId, switchTab, activeTab, syncWithRenderers, isEmpty } = useTabs()
const { allRenderers, fetchRenderers } = useRenderers()
const { allServers, fetchServers } = useMediaServers()

// État du drawer server
const drawerOpen = ref(false)

// Ref pour le swipe edge detection
const viewRef = ref<HTMLElement | null>(null)

// Swipe depuis le bord gauche pour ouvrir le drawer
useSwipe(viewRef, {
  threshold: 50,
  onSwipeEnd(_e: TouchEvent, swipeDirection: string) {
    // Swipe right depuis le bord gauche → ouvrir drawer
    if (swipeDirection === 'right' && !drawerOpen.value) {
      const touch = _e.changedTouches[0]
      // Vérifier que le swipe commence depuis le bord gauche (< 50px)
      if (touch && touch.clientX < 50) {
        drawerOpen.value = true
      }
    }
  },
})

// Gestion de la lecture d'un item depuis le drawer
async function handlePlayItem(item: ContainerEntry, serverId: string) {
  const currentTab = activeTab.value

  if (!currentTab || currentTab.type !== 'renderer' || !currentTab.metadata?.rendererId) {
    console.error('[UnifiedControlView] Pas de renderer actif')
    return
  }

  const rendererId = currentTab.metadata.rendererId

  try {
    console.log('[UnifiedControlView] Play item:', item.title, 'on renderer:', rendererId)

    await api.attachPlaylist(rendererId, serverId, item.id, true) // autoPlay = true

    // Fermer le drawer après succès
    drawerOpen.value = false
  } catch (error) {
    console.error('[UnifiedControlView] Erreur lors de la lecture:', error)
  }
}

// Gestion de l'ajout d'un item à la queue depuis le drawer
async function handleQueueItem(item: ContainerEntry, serverId: string) {
  const currentTab = activeTab.value

  if (!currentTab || currentTab.type !== 'renderer' || !currentTab.metadata?.rendererId) {
    console.error('[UnifiedControlView] Pas de renderer actif')
    return
  }

  const rendererId = currentTab.metadata.rendererId

  try {
    console.log('[UnifiedControlView] Queue item:', item.title, 'on renderer:', rendererId)

    await api.attachPlaylist(rendererId, serverId, item.id, false) // autoPlay = false

    // Fermer le drawer après succès
    drawerOpen.value = false
  } catch (error) {
    console.error('[UnifiedControlView] Erreur lors de l\'ajout à la queue:', error)
  }
}

// Nombre de servers online pour afficher dans le badge
const onlineServersCount = computed(() => allServers.value.filter((s) => s.online).length)

// Gestion de l'ouverture du drawer depuis le bouton
function handleDrawerOpen() {
  drawerOpen.value = true
}

// Sync route query params avec l'état des tabs
onMounted(async () => {
  // Fetch renderers et servers au montage
  await Promise.all([
    fetchRenderers(),
    fetchServers()
  ])

  // Sync initial des tabs avec les renderers
  syncWithRenderers(allRenderers.value)

  // Restaurer l'onglet actif depuis l'URL
  const urlTabId = route.query.tab as string
  if (urlTabId && tabs.value.find((t) => t.id === urlTabId)) {
    switchTab(urlTabId)
  }
})

// Watch renderers pour sync automatique des tabs
watch(
  () => allRenderers.value,
  (newRenderers) => {
    syncWithRenderers(newRenderers)
  },
  { deep: true },
)

// Watch les changements d'URL pour changer d'onglet
watch(
  () => route.query.tab,
  (newTabId) => {
    if (newTabId && typeof newTabId === 'string') {
      const tab = tabs.value.find((t) => t.id === newTabId)
      if (tab && activeTabId.value !== newTabId) {
        switchTab(newTabId)
      }
    }
  },
)

// Watch les changements d'onglet actif pour mettre à jour l'URL
watch(
  () => activeTabId.value,
  (newActiveTabId) => {
    const currentTabId = route.query.tab as string
    if (currentTabId !== newActiveTabId) {
      router.replace({
        query: {
          ...route.query,
          tab: newActiveTabId,
          tabs: tabs.value.map((t) => t.id).join(','),
        },
      })
    }
  },
)

// Composant dynamique selon le type d'onglet
const currentTabComponent = computed(() => {
  const tab = activeTab.value
  if (!tab) return null

  switch (tab.type) {
    case 'renderer':
      return RendererTabContent
    case 'server':
      return ServerTabContent
    default:
      return null
  }
})

// Props pour le composant actif
const currentTabProps = computed(() => {
  const tab = activeTab.value
  if (!tab || !tab.metadata) return null

  if (tab.type === 'renderer' && tab.metadata.rendererId) {
    return { rendererId: tab.metadata.rendererId }
  }
  if (tab.type === 'server' && tab.metadata.serverId) {
    return { serverId: tab.metadata.serverId }
  }
  return null
})
</script>

<template>
  <div ref="viewRef" class="unified-control-view">
    <!-- Zone de contenu -->
    <main class="content-area">
      <!-- État vide: aucun renderer détecté -->
      <EmptyState v-if="isEmpty" />

      <!-- Onglets renderers/servers avec keep-alive -->
      <keep-alive v-else :max="12">
        <component
          v-if="currentTabComponent && currentTabProps"
          :is="currentTabComponent"
          :key="activeTab?.id"
          v-bind="currentTabProps"
        />
      </keep-alive>
    </main>

    <!-- Barre d'onglets en bas -->
    <BottomTabBar :online-servers-count="onlineServersCount" @open-drawer="handleDrawerOpen" />

    <!-- Drawer servers (swipe depuis bord gauche) -->
    <ServerDrawer
      v-model="drawerOpen"
      @play-item="handlePlayItem"
      @queue-item="handleQueueItem"
    />
  </div>
</template>

<style scoped>
.unified-control-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100vh;
  overflow: hidden;
  background: var(--color-bg);
}

.content-area {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 0;
  padding-bottom: 80px; /* Espace pour la barre fixe en bas (64px + marge) */
  position: relative;
}

/* Placeholder temporaire */
.tab-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: var(--spacing-xl);
}

.placeholder-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-lg);
  max-width: 500px;
  padding: var(--spacing-xl);
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: var(--radius-lg);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.1);
  text-align: center;
}

.placeholder-icon {
  color: var(--color-primary);
  opacity: 0.5;
}

.placeholder-title {
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
  margin: 0;
}

.placeholder-text {
  font-size: var(--text-base);
  color: var(--color-text-secondary);
  margin: 0;
  line-height: 1.6;
}

.placeholder-subtitle {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
  font-style: italic;
  margin: 0;
}

/* Responsive pour 800x600 landscape */
@media (min-width: 600px) and (orientation: landscape) {
  .content-area {
    padding: var(--spacing-md);
  }
}

/* Responsive mobile portrait */
@media (max-width: 768px) and (orientation: portrait) {
  .content-area {
    padding: var(--spacing-sm);
  }

  .placeholder-card {
    padding: var(--spacing-lg);
  }

  .placeholder-icon {
    width: 48px;
    height: 48px;
  }

  .placeholder-title {
    font-size: var(--text-xl);
  }

  .placeholder-text {
    font-size: var(--text-sm);
  }
}

/* Animation de transition des onglets */
.v-enter-active,
.v-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.v-enter-from {
  opacity: 0;
  transform: translateX(20px);
}

.v-leave-to {
  opacity: 0;
  transform: translateX(-20px);
}
</style>
