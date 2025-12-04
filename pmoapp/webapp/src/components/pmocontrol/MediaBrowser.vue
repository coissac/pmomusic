<script setup lang="ts">
import { computed, watch } from 'vue'
import { useMediaServersStore } from '@/stores/mediaServers'
import { useRenderersStore } from '@/stores/renderers'
import { useUIStore } from '@/stores/ui'
import type { BreadcrumbItem } from './Breadcrumb.vue'
import Breadcrumb from './Breadcrumb.vue'
import ContainerItem from './ContainerItem.vue'
import MediaItem from './MediaItem.vue'
import { Loader2 } from 'lucide-vue-next'

const props = defineProps<{
  serverId: string
  containerId: string
}>()

const mediaServersStore = useMediaServersStore()
const renderersStore = useRenderersStore()
const uiStore = useUIStore()

const browseData = computed(() =>
  mediaServersStore.getBrowseCached(props.serverId, props.containerId)
)

const containers = computed(() =>
  browseData.value?.entries.filter((e) => e.is_container) || []
)

const items = computed(() =>
  browseData.value?.entries.filter((e) => !e.is_container) || []
)

const loading = computed(() => mediaServersStore.loading)
const error = computed(() => mediaServersStore.error)

const breadcrumbPath = computed<BreadcrumbItem[]>(() => mediaServersStore.currentPath)

// Charger le container au montage et quand containerId change
watch(
  () => props.containerId,
  async (newContainerId) => {
    if (newContainerId) {
      await mediaServersStore.browseContainer(props.serverId, newContainerId)
    }
  },
  { immediate: true }
)

const emit = defineEmits<{
  navigate: [containerId: string]
}>()

function handleNavigate(containerId: string) {
  emit('navigate', containerId)
}

function handleBrowseContainer(containerId: string) {
  emit('navigate', containerId)
}

// Actions handlers
async function handlePlayNow(entryId: string, rendererId: string) {
  try {
    // TODO: Implémenter l'action "play now" quand l'API sera disponible
    console.log('Play now:', entryId, 'on', rendererId)
    uiStore.addNotification('info', 'Fonctionnalité "Lire maintenant" pas encore implémentée dans l\'API')
  } catch (err) {
    console.error('Erreur play now:', err)
    uiStore.addNotification('error', 'Erreur lors de la lecture')
  }
}

async function handleAddToQueue(entryId: string, rendererId: string) {
  try {
    // TODO: Implémenter l'action "add to queue" quand l'API sera disponible
    console.log('Add to queue:', entryId, 'on', rendererId)
    uiStore.addNotification('info', 'Fonctionnalité "Ajouter à la queue" pas encore implémentée dans l\'API')
  } catch (err) {
    console.error('Erreur add to queue:', err)
    uiStore.addNotification('error', 'Erreur lors de l\'ajout à la queue')
  }
}

async function handleAttachPlaylist(containerId: string, rendererId: string) {
  try {
    await renderersStore.attachPlaylist(rendererId, props.serverId, containerId)
    uiStore.addNotification('success', `Queue attachée à la playlist !`)
  } catch (err) {
    console.error('Erreur attach playlist:', err)
    uiStore.addNotification('error', 'Erreur lors de l\'attachement')
  }
}
</script>

<template>
  <div class="media-browser">
    <!-- Breadcrumb -->
    <Breadcrumb
      :items="breadcrumbPath"
      :serverId="serverId"
      @navigate="handleNavigate"
    />

    <!-- Loading state -->
    <div v-if="loading" class="browser-loading">
      <Loader2 :size="32" class="spinner" />
      <p>Chargement...</p>
    </div>

    <!-- Error state -->
    <div v-else-if="error" class="browser-error">
      <p class="error-message">{{ error }}</p>
      <button class="btn btn-secondary" @click="mediaServersStore.browseContainer(serverId, containerId)">
        Réessayer
      </button>
    </div>

    <!-- Content -->
    <div v-else class="browser-content">
      <!-- Containers section -->
      <div v-if="containers.length" class="browser-section">
        <h3 class="section-title">Dossiers et playlists</h3>
        <div class="entries-list">
          <ContainerItem
            v-for="container in containers"
            :key="container.id"
            :entry="container"
            :server-id="serverId"
            @browse="handleBrowseContainer"
            @play-now="handlePlayNow"
            @add-to-queue="handleAddToQueue"
            @attach-playlist="handleAttachPlaylist"
          />
        </div>
      </div>

      <!-- Items section -->
      <div v-if="items.length" class="browser-section">
        <h3 class="section-title">Pistes</h3>
        <div class="entries-list">
          <MediaItem
            v-for="item in items"
            :key="item.id"
            :entry="item"
            :server-id="serverId"
            @play-now="handlePlayNow"
            @add-to-queue="handleAddToQueue"
          />
        </div>
      </div>

      <!-- Empty state -->
      <div v-if="!containers.length && !items.length" class="browser-empty">
        <p>Ce dossier est vide</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.media-browser {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
  height: 100%;
}

/* Loading */
.browser-loading {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
  color: var(--color-text-secondary);
}

.spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* Error */
.browser-error {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
}

.error-message {
  font-size: var(--text-base);
  color: var(--status-offline);
  margin: 0;
}

/* Content */
.browser-content {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xl);
  padding-right: var(--spacing-xs);
}

.browser-section {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}

.section-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
  padding-bottom: var(--spacing-sm);
  border-bottom: 1px solid var(--color-border);
}

.entries-list {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
}

/* Empty state */
.browser-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: var(--text-base);
  padding: var(--spacing-xl);
}

/* Scrollbar styling */
.browser-content::-webkit-scrollbar {
  width: 6px;
}

.browser-content::-webkit-scrollbar-track {
  background: var(--color-bg-secondary);
  border-radius: var(--radius-full);
}

.browser-content::-webkit-scrollbar-thumb {
  background: var(--color-border);
  border-radius: var(--radius-full);
}

.browser-content::-webkit-scrollbar-thumb:hover {
  background: var(--color-text-tertiary);
}
</style>
