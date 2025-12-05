<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useMediaServers } from '@/composables/useMediaServers'
import { useRenderers } from '@/composables/useRenderers'
import { useUIStore } from '@/stores/ui'
import Breadcrumb from './Breadcrumb.vue'
import ContainerItem from './ContainerItem.vue'
import MediaItem from './MediaItem.vue'
import { Loader2 } from 'lucide-vue-next'

const props = defineProps<{
  serverId: string
  containerId: string
}>()

const {
  getBrowseCached,
  browseContainer,
  currentPath: breadcrumbPath,
  loading,
  error
} = useMediaServers()

const {
  playContent,
  addToQueue,
  attachAndPlayPlaylist,
  attachPlaylist,
} = useRenderers()
const uiStore = useUIStore()

// Flags pour gérer le rechargement automatique avec debounce et cooldown
const isRefreshing = ref(false)
const refreshTimeoutId = ref<number | null>(null)
const lastRefreshTime = ref<number>(0)
const REFRESH_COOLDOWN_MS = 5000  // Ne pas recharger plus d'une fois toutes les 5 secondes

const browseData = computed(() =>
  getBrowseCached(props.serverId, props.containerId)
)

const containers = computed(() =>
  browseData.value?.entries.filter((e) => e.is_container) || []
)

const items = computed(() =>
  browseData.value?.entries.filter((e) => !e.is_container) || []
)

// Charger le container au montage et quand containerId change
watch(
  () => props.containerId,
  async (newContainerId) => {
    if (newContainerId) {
      await browseContainer(props.serverId, newContainerId)
    }
  },
  { immediate: true }
)

// Recharger automatiquement si le cache est invalidé (ex: après un ContainersUpdated SSE)
// Cela se produit notamment quand on clique sur "Lire maintenant" sur une playlist,
// ce qui déclenche un événement ContainersUpdated qui invalide le cache
// Utilise un debounce de 3 secondes pour regrouper les multiples invalidations
// et un cooldown de 5 secondes pour éviter les rechargements successifs
watch(
  () => browseData.value,
  (data) => {
    // Si browseData devient undefined alors que containerId est présent,
    // et qu'on n'est pas déjà en train de charger, planifier un rechargement
    if (!data && props.containerId && !loading.value) {
      // Vérifier le cooldown: ignorer si on a rechargé il y a moins de 5 secondes
      const timeSinceLastRefresh = Date.now() - lastRefreshTime.value
      if (timeSinceLastRefresh < REFRESH_COOLDOWN_MS) {
        console.log(
          `[MediaBrowser] Cache invalidé mais cooldown actif (${Math.round((REFRESH_COOLDOWN_MS - timeSinceLastRefresh) / 1000)}s restantes), rechargement ignoré`
        )
        return
      }

      // Annuler tout timeout en cours
      if (refreshTimeoutId.value !== null) {
        clearTimeout(refreshTimeoutId.value)
      }

      // Planifier le rechargement après 3 secondes
      // Cela permet de regrouper plusieurs événements SSE successifs
      refreshTimeoutId.value = window.setTimeout(async () => {
        if (!isRefreshing.value) {
          console.log(
            `[MediaBrowser] Cache invalidé pour ${props.serverId}/${props.containerId}, rechargement après debounce...`
          )
          isRefreshing.value = true
          await browseContainer(props.serverId, props.containerId, false)
          lastRefreshTime.value = Date.now()  // Enregistrer le moment du rechargement
          isRefreshing.value = false
          refreshTimeoutId.value = null
        }
      }, 3000)
    }
  }
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

// Actions handlers pour les containers (playlists/albums)
async function handlePlayContainer(containerId: string, rendererId: string) {
  try {
    await attachAndPlayPlaylist(rendererId, props.serverId, containerId)
    uiStore.notifySuccess('Lecture de la playlist démarrée !')
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Erreur inconnue'
    uiStore.notifyError(`Erreur lors de la lecture de la playlist: ${message}`)
  }
}

async function handleQueueContainer(containerId: string, rendererId: string) {
  try {
    await attachPlaylist(rendererId, props.serverId, containerId)
    uiStore.notifySuccess('Playlist attachée à la queue !')
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Erreur inconnue'
    uiStore.notifyError(`Erreur lors de l'ajout de la playlist: ${message}`)
  }
}

// Actions handlers pour les items (tracks)
async function handlePlayItem(itemId: string, rendererId: string) {
  try {
    await playContent(rendererId, props.serverId, itemId)
    uiStore.notifySuccess('Lecture démarrée !')
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Erreur inconnue'
    uiStore.notifyError(`Erreur lors de la lecture: ${message}`)
  }
}

async function handleQueueItem(itemId: string, rendererId: string) {
  try {
    await addToQueue(rendererId, props.serverId, itemId)
    uiStore.notifySuccess('Ajouté à la queue !')
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Erreur inconnue'
    uiStore.notifyError(`Erreur lors de l'ajout à la queue: ${message}`)
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
      <button class="btn btn-secondary" @click="browseContainer(serverId, containerId, false)">
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
            @play-now="handlePlayContainer"
            @add-to-queue="handleQueueContainer"
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
            @play-now="handlePlayItem"
            @add-to-queue="handleQueueItem"
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
