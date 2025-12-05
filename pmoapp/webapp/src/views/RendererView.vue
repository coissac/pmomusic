<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useRenderersStore } from '@/stores/renderers'
import { useUIStore } from '@/stores/ui'
import CurrentTrack from '@/components/pmocontrol/CurrentTrack.vue'
import TransportControls from '@/components/pmocontrol/TransportControls.vue'
import VolumeControl from '@/components/pmocontrol/VolumeControl.vue'
import QueueViewer from '@/components/pmocontrol/QueueViewer.vue'
import PlaylistBindingPanel from '@/components/pmocontrol/PlaylistBindingPanel.vue'
import StatusBadge from '@/components/pmocontrol/StatusBadge.vue'
import { ArrowLeft, Radio } from 'lucide-vue-next'
import {
  addOpenHomeTrack,
  clearOpenHomePlaylist,
  getOpenHomePlaylist,
  playOpenHomeTrack,
} from '@/services/openhomePlaylist'
import type { OpenHomePlaylistSnapshot } from '@/services/pmocontrol/types'

const route = useRoute()
const router = useRouter()
const renderersStore = useRenderersStore()
const uiStore = useUIStore()

const rendererId = computed(() => route.params.id as string)
const renderer = computed(() => renderersStore.getRendererById(rendererId.value))
const state = computed(() => renderersStore.getStateById(rendererId.value))
const openHomeSupported = computed(() => {
  const current = renderer.value
  if (!current) return false
  const caps = current.capabilities
  return (
    current.protocol === 'openhome' ||
    current.protocol === 'hybrid' ||
    caps?.has_oh_playlist === true
  )
})
const ohPlaylist = ref<OpenHomePlaylistSnapshot | null>(null)
const ohLoading = ref(false)
const ohError = ref<string | null>(null)
const newOhUri = ref('')
const newOhMeta = ref('')
const canAddOhTrack = computed(() => newOhUri.value.trim().length > 0)

// Charger les données au montage si nécessaire
onMounted(async () => {
  // Indiquer à uiStore quel renderer est sélectionné
  uiStore.selectRenderer(rendererId.value)

  if (!renderer.value) {
    await renderersStore.fetchRenderers()
  }
  if (!state.value) {
    await renderersStore.fetchRendererState(rendererId.value)
  }
  await renderersStore.fetchQueue(rendererId.value)
})

watch(
  openHomeSupported,
  async supported => {
    if (supported) {
      await refreshOhPlaylist()
    } else {
      ohPlaylist.value = null
    }
  },
  { immediate: true },
)

watch(rendererId, () => {
  ohPlaylist.value = null
  ohError.value = null
  newOhUri.value = ''
  newOhMeta.value = ''
})

// Nettoyer la sélection au démontage
onUnmounted(() => {
  uiStore.selectRenderer(null)
})

function goBack() {
  router.push('/')
}

const protocolLabel = computed(() => {
  if (!renderer.value) return ''
  switch (renderer.value.protocol) {
    case 'upnp':
      return 'UPnP AV'
    case 'openhome':
      return 'OpenHome'
    case 'hybrid':
      return 'Hybrid (UPnP + OpenHome)'
    default:
      return 'Inconnu'
  }
})

async function refreshOhPlaylist() {
  if (!renderer.value || !openHomeSupported.value) return
  ohLoading.value = true
  ohError.value = null
  try {
    ohPlaylist.value = await getOpenHomePlaylist(renderer.value.id)
  } catch (e) {
    ohError.value =
      e instanceof Error ? e.message : 'Failed to load OpenHome playlist'
  } finally {
    ohLoading.value = false
  }
}

async function handleOhClear() {
  if (!renderer.value) return
  try {
    await clearOpenHomePlaylist(renderer.value.id)
    await refreshOhPlaylist()
  } catch (e) {
    ohError.value =
      e instanceof Error ? e.message : 'Failed to clear OpenHome playlist'
  }
}

async function handleOhPlay(trackId: number) {
  if (!renderer.value) return
  try {
    await playOpenHomeTrack(renderer.value.id, trackId)
    await refreshOhPlaylist()
  } catch (e) {
    ohError.value =
      e instanceof Error ? e.message : `Failed to play OpenHome track ${trackId}`
  }
}

async function handleOhAdd() {
  if (!renderer.value || !canAddOhTrack.value) return
  try {
    await addOpenHomeTrack(renderer.value.id, {
      uri: newOhUri.value.trim(),
      metadata: newOhMeta.value,
      play: false,
    })
    newOhUri.value = ''
    newOhMeta.value = ''
    await refreshOhPlaylist()
  } catch (e) {
    ohError.value =
      e instanceof Error ? e.message : 'Failed to add track to OpenHome playlist'
  }
}
</script>

<template>
  <div class="renderer-view">
    <!-- Header -->
    <header class="renderer-header">
      <button class="btn-back" @click="goBack" title="Retour au dashboard">
        <ArrowLeft :size="20" />
      </button>
      <div class="header-content">
        <div class="renderer-info">
          <Radio :size="24" class="renderer-icon" />
          <div class="renderer-details">
            <h1 class="renderer-name">{{ renderer?.friendly_name || 'Chargement...' }}</h1>
            <p class="renderer-model">{{ renderer?.model_name }} • {{ protocolLabel }}</p>
          </div>
        </div>
        <StatusBadge v-if="state" :status="state.transport_state" />
      </div>
    </header>

    <!-- Loading state -->
    <div v-if="!renderer || !state" class="loading-state">
      <p>Chargement du renderer...</p>
    </div>

    <!-- Main content -->
    <div v-else class="renderer-content">
      <!-- Left column (Desktop) / Top (Mobile) -->
      <div class="left-column">
        <!-- Current Track -->
        <section class="content-section">
          <CurrentTrack :rendererId="rendererId" />
        </section>

        <!-- Transport Controls -->
        <section class="content-section">
          <TransportControls :rendererId="rendererId" />
        </section>

        <!-- Volume Control -->
        <section class="content-section">
          <h3 class="section-subtitle">Volume</h3>
          <VolumeControl :rendererId="rendererId" />
        </section>

        <!-- Playlist Binding -->
        <section class="content-section">
          <PlaylistBindingPanel :rendererId="rendererId" />
        </section>

        <section v-if="openHomeSupported" class="content-section openhome-playlist">
          <h2>OpenHome Playlist</h2>

          <div v-if="ohLoading">Chargement de la playlist…</div>
          <div v-else-if="ohError" class="error">{{ ohError }}</div>

          <div v-else-if="ohPlaylist && ohPlaylist.tracks.length === 0">
            Playlist vide.
          </div>

          <div v-else-if="ohPlaylist">
            <table class="oh-playlist-table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Titre</th>
                  <th>Artiste</th>
                  <th>Album</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="track in ohPlaylist.tracks"
                  :key="track.id"
                  :class="{ current: ohPlaylist.current_id === track.id }"
                >
                  <td>{{ track.id }}</td>
                  <td>{{ track.title || '—' }}</td>
                  <td>{{ track.artist || '—' }}</td>
                  <td>{{ track.album || '—' }}</td>
                  <td class="actions-cell">
                    <button class="btn btn-secondary btn-icon" @click="handleOhPlay(track.id)" title="Lire ce morceau">
                      ▶
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>

            <div class="oh-controls">
              <button class="btn btn-secondary" @click="refreshOhPlaylist">
                🔁 Rafraîchir
              </button>
              <button class="btn btn-danger" @click="handleOhClear">
                🗑 Effacer la playlist
              </button>
            </div>

            <div class="oh-add-form">
              <input v-model="newOhUri" placeholder="URI à ajouter" />
              <textarea v-model="newOhMeta" placeholder="DIDL-Lite (optionnel)" rows="2"></textarea>
              <button class="btn btn-primary" @click="handleOhAdd" :disabled="!canAddOhTrack">
                ➕ Ajouter
              </button>
            </div>
          </div>
        </section>
      </div>

      <!-- Right column (Desktop) / Bottom (Mobile) -->
      <div class="right-column">
        <section class="content-section queue-section">
          <QueueViewer :rendererId="rendererId" />
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.renderer-view {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
  padding: var(--spacing-lg);
  max-width: 1400px;
  margin: 0 auto;
  width: 100%;
  height: 100%;
}

/* Header */
.renderer-header {
  display: flex;
  align-items: flex-start;
  gap: var(--spacing-md);
}

.btn-back {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  background: none;
  border: none;
  border-radius: var(--radius-md);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
  flex-shrink: 0;
}

.btn-back:hover {
  background-color: var(--color-bg-secondary);
  color: var(--color-text);
}

.header-content {
  flex: 1;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--spacing-md);
  flex-wrap: wrap;
}

.renderer-info {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
}

.renderer-icon {
  color: var(--color-primary);
  flex-shrink: 0;
}

.renderer-details {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
}

.renderer-name {
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
  margin: 0;
}

.renderer-model {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: 0;
}

/* Loading */
.loading-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--text-base);
  color: var(--color-text-secondary);
}

/* Content */
.renderer-content {
  flex: 1;
  display: grid;
  gap: var(--spacing-xl);
  grid-template-columns: 1fr;
  min-height: 0;
}

.left-column,
.right-column {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
  min-height: 0;
}

.content-section {
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  padding: var(--spacing-lg);
  border: 1px solid var(--color-border);
}

.queue-section {
  flex: 1;
  min-height: 400px;
  display: flex;
  flex-direction: column;
}

.section-subtitle {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--color-text);
  margin: 0 0 var(--spacing-md);
}

.openhome-playlist h2 {
  margin: 0 0 var(--spacing-md);
  font-size: var(--text-lg);
}

.oh-playlist-table {
  width: 100%;
  border-collapse: collapse;
  margin-bottom: var(--spacing-md);
}

.oh-playlist-table th,
.oh-playlist-table td {
  padding: var(--spacing-xs);
  border-bottom: 1px solid var(--color-border);
  font-size: var(--text-sm);
}

.oh-playlist-table tbody tr:hover {
  background-color: var(--color-bg-tertiary);
}

.oh-playlist-table tr.current {
  background-color: rgba(16, 185, 129, 0.15);
}

.actions-cell {
  text-align: center;
}

.btn-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
}

.oh-controls {
  display: flex;
  flex-wrap: wrap;
  gap: var(--spacing-sm);
  margin-bottom: var(--spacing-md);
}

.oh-add-form {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}

.oh-add-form input,
.oh-add-form textarea {
  width: 100%;
  padding: var(--spacing-sm);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background-color: var(--color-bg-tertiary);
  color: var(--color-text);
}

.oh-add-form button {
  align-self: flex-start;
}

.error {
  color: var(--status-error, #dc2626);
  font-weight: 600;
}

/* Responsive - Desktop */
@media (min-width: 1024px) {
  .renderer-content {
    grid-template-columns: 400px 1fr;
  }

  .queue-section {
    min-height: 0;
  }
}

/* Responsive - Tablet */
@media (min-width: 768px) and (max-width: 1023px) {
  .renderer-content {
    grid-template-columns: 1fr;
  }

  .left-column {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--spacing-lg);
  }

  .queue-section {
    grid-column: 1 / -1;
  }
}

/* Responsive - Mobile */
@media (max-width: 767px) {
  .renderer-view {
    padding: var(--spacing-md);
  }

  .renderer-name {
    font-size: var(--text-xl);
  }

  .renderer-info {
    flex-wrap: wrap;
  }

  .queue-section {
    min-height: 300px;
  }
}
</style>
