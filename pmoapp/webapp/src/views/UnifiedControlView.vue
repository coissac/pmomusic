<script setup lang="ts">
import { watch, onMounted, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useTabs } from '@/composables/useTabs'
import BottomTabBar from '@/components/unified/BottomTabBar.vue'

// Import des composants de contenu
import HomeTabContent from '@/components/unified/HomeTabContent.vue'
import RendererTabContent from '@/components/unified/RendererTabContent.vue'
import ServerTabContent from '@/components/unified/ServerTabContent.vue'

const route = useRoute()
const router = useRouter()
const { tabs, activeTabId, switchTab, activeTab } = useTabs()

// Sync route query params avec l'état des tabs
onMounted(() => {
  // Restaurer l'onglet actif depuis l'URL au montage
  const urlTabId = route.query.tab as string
  if (urlTabId && tabs.value.find((t) => t.id === urlTabId)) {
    switchTab(urlTabId)
  }
})

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
    case 'home':
      return HomeTabContent
    case 'renderer':
      return RendererTabContent
    case 'server':
      return ServerTabContent
    default:
      return null
  }
})
</script>

<template>
  <div class="unified-control-view">
    <!-- Zone de contenu avec keep-alive pour la performance -->
    <main class="content-area">
      <keep-alive :max="8">
        <component
          v-if="currentTabComponent"
          :is="currentTabComponent"
          :key="activeTab?.id"
          v-bind="activeTab?.metadata || {}"
        />
      </keep-alive>
    </main>

    <!-- Barre d'onglets en bas -->
    <BottomTabBar />
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
