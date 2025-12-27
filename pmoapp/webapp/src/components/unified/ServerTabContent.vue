<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useMediaServers } from '@/composables/useMediaServers'
import MediaBrowser from '@/components/pmocontrol/MediaBrowser.vue'

const props = defineProps<{
  serverId: string
}>()

const { getServerById, fetchServers } = useMediaServers()

// Container ID actuel (commence à la racine '0')
const containerId = ref('0')

const server = computed(() => getServerById(props.serverId))
const isOnline = computed(() => server.value?.online ?? false)

// Gestion de la navigation dans les containers
function handleNavigate(newContainerId: string) {
  containerId.value = newContainerId
}

onMounted(async () => {
  await fetchServers()
})
</script>

<template>
  <div class="server-tab-content">
    <!-- Header avec nom du serveur et état -->
    <header class="server-header">
      <div class="header-info">
        <h1 class="server-name">{{ server?.friendly_name || 'Media Server' }}</h1>
        <p v-if="server?.model_name" class="server-model">{{ server.model_name }}</p>
      </div>
      <div class="header-badges">
        <span v-if="isOnline" class="online-badge">ONLINE</span>
        <span v-else class="offline-badge">OFFLINE</span>
      </div>
    </header>

    <!-- Browser de médias -->
    <div class="browser-section">
      <MediaBrowser
        v-if="isOnline"
        :server-id="serverId"
        :container-id="containerId"
        @navigate="handleNavigate"
      />

      <div v-else class="offline-message">
        <p>Ce serveur est actuellement hors ligne</p>
        <button class="btn btn-secondary" @click="fetchServers(true)">Actualiser</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.server-tab-content {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

/* Header */
.server-header {
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

.server-name {
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
  margin: 0;
}

.server-model {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: 4px 0 0 0;
}

.header-badges {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}

.online-badge,
.offline-badge {
  padding: 4px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.online-badge {
  background: var(--status-playing);
  color: white;
}

.offline-badge {
  background: var(--status-offline);
  color: white;
}

/* Section browser */
.browser-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: var(--spacing-lg);
}

.offline-message {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
  padding: var(--spacing-xl);
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: var(--radius-lg);
  text-align: center;
}

.offline-message p {
  color: var(--status-offline);
  font-size: var(--text-lg);
  margin: 0;
}

/* Responsive - Mobile portrait */
@media (max-width: 768px) and (orientation: portrait) {
  .server-header {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--spacing-sm);
    padding: var(--spacing-md);
  }

  .header-badges {
    width: 100%;
    justify-content: flex-start;
  }

  .browser-section {
    padding: var(--spacing-md);
  }
}

/* Responsive - 800x600 landscape */
@media (min-width: 600px) and (max-width: 1024px) and (orientation: landscape) {
  .server-header {
    padding: var(--spacing-sm) var(--spacing-md);
  }

  .server-name {
    font-size: var(--text-xl);
  }

  .browser-section {
    padding: var(--spacing-md);
  }
}

/* Large desktop */
@media (min-width: 1200px) {
  .browser-section {
    max-width: 1400px;
    margin: 0 auto;
    width: 100%;
  }
}
</style>
