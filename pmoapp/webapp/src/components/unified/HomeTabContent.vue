<script setup lang="ts">
import { onMounted } from 'vue'
import { useRenderers } from '@/composables/useRenderers'
import { useMediaServers } from '@/composables/useMediaServers'
import { useTabs } from '@/composables/useTabs'
import RendererCard from '@/components/pmocontrol/RendererCard.vue'
import MediaServerCard from '@/components/pmocontrol/MediaServerCard.vue'
import { Radio, Server } from 'lucide-vue-next'

const {
  allRenderers: renderers,
  onlineRenderers,
  getStateById,
  fetchRenderers,
  fetchRendererSnapshot,
} = useRenderers()

const { allServers: mediaServers, onlineServers, fetchServers } = useMediaServers()

const { openRenderer, openServer } = useTabs()

// Charger les données au montage
onMounted(async () => {
  await fetchRenderers()
  await fetchServers()

  // Charger les snapshots de tous les renderers
  for (const renderer of renderers.value) {
    fetchRendererSnapshot(renderer.id, { force: true })
  }
})

// Gestion des clics sur les cartes
function handleRendererClick(rendererId: string) {
  const renderer = renderers.value.find((r) => r.id === rendererId)
  if (renderer) {
    openRenderer(renderer)
  }
}

function handleServerClick(serverId: string) {
  const server = mediaServers.value.find((s) => s.id === serverId)
  if (server) {
    openServer(server)
  }
}
</script>

<template>
  <div class="home-tab-content">
    <!-- Header avec statistiques -->
    <header class="dashboard-header">
      <h1 class="dashboard-title">PMOControl Dashboard</h1>
      <div class="dashboard-stats">
        <div class="stat">
          <Radio :size="20" />
          <span>{{ onlineRenderers.length }} / {{ renderers.length }} renderers</span>
        </div>
        <div class="stat">
          <Server :size="20" />
          <span>{{ onlineServers.length }} / {{ mediaServers.length }} serveurs</span>
        </div>
      </div>
    </header>

    <!-- Section Renderers -->
    <section class="dashboard-section">
      <div class="section-header">
        <h2 class="section-title">
          <Radio :size="24" />
          <span>Renderers Audio</span>
        </h2>
      </div>

      <div v-if="renderers.length" class="renderers-grid">
        <div
          v-for="renderer in renderers"
          :key="renderer.id"
          @click="handleRendererClick(renderer.id)"
          class="renderer-card-wrapper"
        >
          <RendererCard :renderer="renderer" :state="getStateById(renderer.id) ?? null" />
        </div>
      </div>

      <div v-else class="empty-state">
        <p>Aucun renderer découvert</p>
        <button class="btn btn-secondary" @click="fetchRenderers(true)">Actualiser</button>
      </div>
    </section>

    <!-- Section Media Servers -->
    <section class="dashboard-section">
      <div class="section-header">
        <h2 class="section-title">
          <Server :size="24" />
          <span>Serveurs de Médias</span>
        </h2>
      </div>

      <div v-if="mediaServers.length" class="servers-grid">
        <div
          v-for="server in mediaServers"
          :key="server.id"
          @click="handleServerClick(server.id)"
          class="server-card-wrapper"
        >
          <MediaServerCard :server="server" />
        </div>
      </div>

      <div v-else class="empty-state">
        <p>Aucun serveur de médias découvert</p>
        <button class="btn btn-secondary" @click="fetchServers(true)">Actualiser</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.home-tab-content {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xl);
  padding: var(--spacing-lg);
  max-width: 1400px;
  margin: 0 auto;
  width: 100%;
  height: 100%;
  overflow-y: auto;
}

/* Header */
.dashboard-header {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}

.dashboard-title {
  font-size: var(--text-3xl);
  font-weight: 700;
  color: var(--color-text);
  margin: 0;
}

.dashboard-stats {
  display: flex;
  gap: var(--spacing-lg);
  flex-wrap: wrap;
}

.stat {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-sm) var(--spacing-md);
  background: rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text-secondary);
}

.stat svg {
  color: var(--color-primary);
}

/* Sections */
.dashboard-section {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
}

.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.section-title {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  font-size: var(--text-2xl);
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.section-title svg {
  color: var(--color-primary);
}

/* Grids */
.renderers-grid,
.servers-grid {
  display: grid;
  gap: var(--spacing-lg);
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
}

/* Wrappers pour les cartes cliquables */
.renderer-card-wrapper,
.server-card-wrapper {
  cursor: pointer;
  transition: transform 0.2s ease;
}

.renderer-card-wrapper:hover,
.server-card-wrapper:hover {
  transform: translateY(-2px);
}

.renderer-card-wrapper:active,
.server-card-wrapper:active {
  transform: translateY(0);
}

/* Empty state */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
  padding: var(--spacing-xl);
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border-radius: var(--radius-lg);
  border: 2px dashed rgba(255, 255, 255, 0.1);
}

.empty-state p {
  font-size: var(--text-base);
  color: var(--color-text-tertiary);
  margin: 0;
}

/* Responsive pour 800x600 landscape */
@media (min-width: 600px) and (max-width: 1024px) and (orientation: landscape) {
  .home-tab-content {
    padding: var(--spacing-md);
    gap: var(--spacing-lg);
  }

  .dashboard-title {
    font-size: var(--text-2xl);
  }

  .section-title {
    font-size: var(--text-xl);
  }

  .renderers-grid,
  .servers-grid {
    grid-template-columns: repeat(2, 1fr);
    gap: var(--spacing-md);
  }
}

/* Responsive mobile portrait */
@media (max-width: 768px) and (orientation: portrait) {
  .home-tab-content {
    padding: var(--spacing-md);
    gap: var(--spacing-lg);
  }

  .dashboard-title {
    font-size: var(--text-2xl);
  }

  .section-title {
    font-size: var(--text-xl);
  }

  .renderers-grid,
  .servers-grid {
    grid-template-columns: 1fr;
  }

  .dashboard-stats {
    gap: var(--spacing-sm);
  }

  .stat {
    font-size: 12px;
    padding: 6px 12px;
  }
}

/* Grid responsive pour très grand écran */
@media (min-width: 1024px) {
  .renderers-grid,
  .servers-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}
</style>
