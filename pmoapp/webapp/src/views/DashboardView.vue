<script setup lang="ts">
import { onMounted } from 'vue'
import { useRenderers } from '@/composables/useRenderers'
import { useMediaServers } from '@/composables/useMediaServers'
import RendererCard from '@/components/pmocontrol/RendererCard.vue'
import MediaServerCard from '@/components/pmocontrol/MediaServerCard.vue'
import { Radio, Server } from 'lucide-vue-next'

const {
  allRenderers: renderers,
  onlineRenderers,
  getStateById,
  fetchRenderers,
  fetchRendererState
} = useRenderers()

const {
  allServers: mediaServers,
  onlineServers,
  fetchServers
} = useMediaServers()

// Charger les données au montage
onMounted(async () => {
  await fetchRenderers()
  await fetchServers()

  // Charger les états de tous les renderers pour afficher les covers
  for (const renderer of renderers.value) {
    fetchRendererState(renderer.id)
  }
})
</script>

<template>
  <div class="dashboard-view">
    <!-- Header -->
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

    <!-- Renderers Section -->
    <section class="dashboard-section">
      <div class="section-header">
        <h2 class="section-title">
          <Radio :size="24" />
          <span>Renderers Audio</span>
        </h2>
      </div>

      <div v-if="renderers.length" class="renderers-grid">
        <RendererCard
          v-for="renderer in renderers"
          :key="renderer.id"
          :renderer="renderer"
          :state="getStateById(renderer.id) ?? null"
        />
      </div>

      <div v-else class="empty-state">
        <p>Aucun renderer découvert</p>
        <button class="btn btn-secondary" @click="fetchRenderers(true)">
          Actualiser
        </button>
      </div>
    </section>

    <!-- Media Servers Section -->
    <section class="dashboard-section">
      <div class="section-header">
        <h2 class="section-title">
          <Server :size="24" />
          <span>Serveurs de Médias</span>
        </h2>
      </div>

      <div v-if="mediaServers.length" class="servers-grid">
        <MediaServerCard
          v-for="server in mediaServers"
          :key="server.id"
          :server="server"
        />
      </div>

      <div v-else class="empty-state">
        <p>Aucun serveur de médias découvert</p>
        <button class="btn btn-secondary" @click="fetchServers(true)">
          Actualiser
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.dashboard-view {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xl);
  padding: var(--spacing-lg);
  max-width: 1400px;
  margin: 0 auto;
  width: 100%;
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
  background-color: var(--color-bg-secondary);
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

/* Empty state */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
  padding: var(--spacing-xl);
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  border: 2px dashed var(--color-border);
}

.empty-state p {
  font-size: var(--text-base);
  color: var(--color-text-tertiary);
  margin: 0;
}

/* Responsive */
@media (max-width: 768px) {
  .dashboard-view {
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
}

@media (min-width: 768px) and (max-width: 1024px) {
  .renderers-grid,
  .servers-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (min-width: 1024px) {
  .renderers-grid,
  .servers-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}
</style>
