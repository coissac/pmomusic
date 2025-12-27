<script setup lang="ts">
import { ref, computed, toRef, onMounted } from 'vue'
import { useRenderer } from '@/composables/useRenderers'
import CurrentTrack from '@/components/pmocontrol/CurrentTrack.vue'
import TransportControls from '@/components/pmocontrol/TransportControls.vue'
import VolumeControl from '@/components/pmocontrol/VolumeControl.vue'
import QueueViewer from '@/components/pmocontrol/QueueViewer.vue'
import StatusBadge from '@/components/pmocontrol/StatusBadge.vue'
import { ChevronUp, ChevronDown, Link } from 'lucide-vue-next'
import { useRenderers } from '@/composables/useRenderers'
import { useUIStore } from '@/stores/ui'

const props = defineProps<{
  rendererId: string
}>()

const { renderer, state, queue, binding, refresh } = useRenderer(toRef(props, 'rendererId'))
const { detachPlaylist } = useRenderers()
const uiStore = useUIStore()

// État du drawer queue sur mobile
const queueDrawerOpen = ref(false)

function toggleQueueDrawer() {
  queueDrawerOpen.value = !queueDrawerOpen.value
}

// Charger les données au montage
onMounted(async () => {
  await refresh()
})

// État du renderer pour affichage
const isOnline = computed(() => renderer.value?.online ?? false)
const transportState = computed(() => state.value?.transport_state ?? 'STOPPED')
const hasPlaylistBinding = computed(() => !!binding.value)

// Détacher la playlist
async function handleDetachPlaylist() {
  try {
    await detachPlaylist(props.rendererId)
    uiStore.notifySuccess('Playlist détachée')
  } catch (error) {
    uiStore.notifyError(`Erreur: ${error instanceof Error ? error.message : 'Erreur inconnue'}`)
  }
}
</script>

<template>
  <div class="renderer-tab-content">
    <!-- Header avec nom du renderer et état -->
    <header class="renderer-header">
      <div class="header-info">
        <h1 class="renderer-name">{{ renderer?.friendly_name || 'Renderer' }}</h1>
        <p v-if="renderer?.model_name" class="renderer-model">{{ renderer.model_name }}</p>
      </div>
      <div class="header-badges">
        <StatusBadge v-if="state" :status="transportState" />
        <span v-if="renderer?.protocol" class="protocol-badge">
          {{ renderer.protocol.toUpperCase() }}
        </span>

        <!-- Badge playlist binding avec tooltip -->
        <div v-if="hasPlaylistBinding" class="playlist-badge" :title="`Playlist liée\nServeur: ${binding?.server_id}\nContainer: ${binding?.container_id}`">
          <button class="playlist-badge-btn" @click="handleDetachPlaylist" title="Cliquer pour détacher">
            <Link :size="16" />
          </button>
        </div>

        <span v-if="!isOnline" class="offline-badge">OFFLINE</span>
      </div>
    </header>

    <!-- Layout principal -->
    <div class="renderer-layout" :class="{ 'queue-open': queueDrawerOpen }">
      <!-- Colonne gauche: Contrôles -->
      <div class="controls-column">
        <!-- Pochette + infos track -->
        <CurrentTrack v-if="state" :renderer-id="rendererId" class="current-track-section" />

        <!-- Contrôles de transport -->
        <div v-if="state" class="controls-section">
          <TransportControls :renderer-id="rendererId" />
        </div>

        <!-- Contrôle de volume -->
        <div v-if="state" class="volume-section">
          <VolumeControl :renderer-id="rendererId" />
        </div>

        <!-- Message si offline -->
        <div v-if="!isOnline" class="offline-message">
          <p>Ce renderer est actuellement hors ligne</p>
        </div>
      </div>

      <!-- Colonne droite: Queue (desktop) -->
      <div class="queue-column">
        <div class="queue-header">
          <h2 class="queue-title">File d'attente</h2>
          <span v-if="queue" class="queue-count">{{ queue.items.length }} morceaux</span>
        </div>

        <QueueViewer v-if="queue" :renderer-id="rendererId" class="queue-viewer" />

        <div v-else class="queue-empty">
          <p>Aucun morceau en file d'attente</p>
        </div>
      </div>

      <!-- Drawer queue (mobile uniquement) -->
      <div class="queue-drawer" :class="{ open: queueDrawerOpen }">
        <!-- Toggle button -->
        <button class="queue-drawer-toggle" @click="toggleQueueDrawer">
          <ChevronUp v-if="queueDrawerOpen" :size="24" />
          <ChevronDown v-else :size="24" />
          <span>File d'attente ({{ queue?.items.length || 0 }})</span>
        </button>

        <!-- Contenu du drawer -->
        <div class="queue-drawer-content">
          <QueueViewer v-if="queue" :renderer-id="rendererId" />
          <div v-else class="queue-empty">
            <p>Aucun morceau en file d'attente</p>
          </div>
        </div>
      </div>

      <!-- Backdrop pour fermer le drawer -->
      <div
        v-if="queueDrawerOpen"
        class="queue-drawer-backdrop"
        @click="queueDrawerOpen = false"
      ></div>
    </div>
  </div>
</template>

<style scoped>
.renderer-tab-content {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

/* Header */
.renderer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--spacing-md) var(--spacing-lg);
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  flex-shrink: 0;
}

.header-info {
  flex: 1;
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
  margin: 4px 0 0 0;
}

.header-badges {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}

.protocol-badge,
.offline-badge {
  padding: 4px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.protocol-badge {
  background: rgba(255, 255, 255, 0.1);
  color: var(--color-text-secondary);
  border: 1px solid rgba(255, 255, 255, 0.2);
}

.offline-badge {
  background: var(--status-offline);
  color: white;
}

/* Badge playlist binding compact */
.playlist-badge {
  position: relative;
  display: inline-flex;
}

.playlist-badge-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
  background: rgba(102, 126, 234, 0.2);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(102, 126, 234, 0.4);
  border-radius: 50%;
  cursor: pointer;
  transition: all 0.3s ease;
  color: rgba(102, 126, 234, 1);
}

.playlist-badge-btn:hover {
  background: rgba(102, 126, 234, 0.3);
  border-color: rgba(102, 126, 234, 0.6);
  transform: scale(1.1);
}

.playlist-badge-btn:active {
  transform: scale(1.0);
}

@media (prefers-color-scheme: dark) {
  .playlist-badge-btn {
    background: rgba(102, 126, 234, 0.15);
    border-color: rgba(102, 126, 234, 0.3);
  }

  .playlist-badge-btn:hover {
    background: rgba(102, 126, 234, 0.25);
    border-color: rgba(102, 126, 234, 0.5);
  }
}

/* Layout principal - 800x600 landscape (2 colonnes) */
.renderer-layout {
  display: grid;
  grid-template-columns: 300px 1fr;
  gap: var(--spacing-lg);
  padding: var(--spacing-lg);
  flex: 1;
  overflow: hidden;
}

/* Colonne gauche - Contrôles */
.controls-column {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
  overflow-y: auto;
  padding-right: var(--spacing-sm);
}

.current-track-section,
.controls-section,
.volume-section,
.playlist-section {
  flex-shrink: 0;
}

.offline-message {
  padding: var(--spacing-lg);
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: var(--radius-md);
  text-align: center;
  color: var(--status-offline);
}

/* Colonne droite - Queue */
.queue-column {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
  overflow: hidden;
}

.queue-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: var(--spacing-sm);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.queue-title {
  font-size: var(--text-xl);
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.queue-count {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  padding: 4px 12px;
  background: rgba(255, 255, 255, 0.05);
  border-radius: var(--radius-sm);
}

.queue-viewer {
  flex: 1;
  overflow-y: auto;
}

.queue-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--spacing-xl);
  background: rgba(255, 255, 255, 0.03);
  border-radius: var(--radius-md);
  border: 1px dashed rgba(255, 255, 255, 0.1);
}

.queue-empty p {
  color: var(--color-text-tertiary);
  font-size: var(--text-base);
  margin: 0;
}

/* Queue drawer - masqué sur desktop, visible sur mobile */
.queue-drawer {
  display: none;
}

/* Responsive - Mobile portrait */
@media (max-width: 768px) and (orientation: portrait) {
  .renderer-header {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--spacing-sm);
    padding: var(--spacing-md);
  }

  .header-badges {
    width: 100%;
    justify-content: flex-start;
  }

  .renderer-layout {
    grid-template-columns: 1fr;
    gap: var(--spacing-md);
    padding: var(--spacing-md);
    padding-bottom: 0;
  }

  /* Queue column cachée sur mobile */
  .queue-column {
    display: none;
  }

  /* Queue drawer visible sur mobile */
  .queue-drawer {
    display: block;
    position: fixed;
    bottom: 64px; /* Hauteur de la tab bar */
    left: 0;
    right: 0;
    background: rgba(255, 255, 255, 0.1);
    backdrop-filter: blur(30px) saturate(180%);
    -webkit-backdrop-filter: blur(30px) saturate(180%);
    border-top: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: 0 -4px 24px rgba(0, 0, 0, 0.2);
    transform: translateY(calc(100% - 56px));
    transition: transform 0.3s ease;
    z-index: 90;
    max-height: 70vh;
  }

  .queue-drawer.open {
    transform: translateY(0);
  }

  .queue-drawer-toggle {
    width: 100%;
    height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--color-text);
    font-size: var(--text-base);
    font-weight: 600;
    font-family: inherit;
  }

  .queue-drawer-content {
    max-height: calc(70vh - 56px);
    overflow-y: auto;
    padding: var(--spacing-md);
  }

  .queue-drawer-backdrop {
    display: block;
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 64px;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    z-index: 89;
  }

  .controls-column {
    padding-right: 0;
  }
}

/* Responsive - 800x600 landscape */
@media (min-width: 600px) and (max-width: 1024px) and (orientation: landscape) {
  .renderer-layout {
    grid-template-columns: 280px 1fr;
    gap: var(--spacing-md);
    padding: var(--spacing-md);
  }

  .renderer-header {
    padding: var(--spacing-sm) var(--spacing-md);
  }

  .renderer-name {
    font-size: var(--text-xl);
  }

  .queue-title {
    font-size: var(--text-lg);
  }
}

/* Large desktop */
@media (min-width: 1200px) {
  .renderer-layout {
    grid-template-columns: 350px 1fr;
    max-width: 1400px;
    margin: 0 auto;
  }
}

/* Scrollbar styling */
.controls-column::-webkit-scrollbar,
.queue-viewer::-webkit-scrollbar {
  width: 6px;
}

.controls-column::-webkit-scrollbar-track,
.queue-viewer::-webkit-scrollbar-track {
  background: rgba(255, 255, 255, 0.05);
  border-radius: 3px;
}

.controls-column::-webkit-scrollbar-thumb,
.queue-viewer::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border-radius: 3px;
}

.controls-column::-webkit-scrollbar-thumb:hover,
.queue-viewer::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}
</style>
