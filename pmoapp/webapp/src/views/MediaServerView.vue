<script setup lang="ts">
import { computed, watch, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useMediaServersStore } from '@/stores/mediaServers'
import MediaBrowser from '@/components/pmocontrol/MediaBrowser.vue'
import { ArrowLeft, Server } from 'lucide-vue-next'

const route = useRoute()
const router = useRouter()
const mediaServersStore = useMediaServersStore()

const serverId = computed(() => route.params.serverId as string)
const containerId = ref(route.query.container as string || '0')

const server = computed(() => mediaServersStore.getServerById(serverId.value))

// Watcher sur query.container pour mettre à jour containerId
watch(
  () => route.query.container,
  (newContainer) => {
    containerId.value = (newContainer as string) || '0'
  }
)

function goBack() {
  router.push('/')
}

function handleNavigate(newContainerId: string) {
  router.push({
    name: 'MediaServer',
    params: { serverId: serverId.value },
    query: { container: newContainerId }
  })
}
</script>

<template>
  <div class="media-server-view">
    <!-- Header -->
    <header class="server-header">
      <button class="btn-back" @click="goBack" title="Retour au dashboard">
        <ArrowLeft :size="20" />
      </button>
      <div class="header-content">
        <div class="server-info">
          <Server :size="24" class="server-icon" />
          <div class="server-details">
            <h1 class="server-name">{{ server?.friendly_name || 'Chargement...' }}</h1>
            <p class="server-model">{{ server?.model_name }}</p>
          </div>
        </div>
        <div v-if="server" class="server-status" :class="{ online: server.online, offline: !server.online }">
          <span class="status-dot"></span>
          <span class="status-label">{{ server.online ? 'En ligne' : 'Hors ligne' }}</span>
        </div>
      </div>
    </header>

    <!-- Loading state -->
    <div v-if="!server" class="loading-state">
      <p>Chargement du serveur...</p>
    </div>

    <!-- Offline state -->
    <div v-else-if="!server.online" class="offline-state">
      <p>Ce serveur est actuellement hors ligne</p>
      <button class="btn btn-secondary" @click="goBack">
        Retour au dashboard
      </button>
    </div>

    <!-- Main content -->
    <div v-else class="server-content">
      <MediaBrowser
        :serverId="serverId"
        :containerId="containerId"
        @navigate="handleNavigate"
      />
    </div>
  </div>
</template>

<style scoped>
.media-server-view {
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
.server-header {
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

.server-info {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
}

.server-icon {
  color: var(--color-primary);
  flex-shrink: 0;
}

.server-details {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
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
  margin: 0;
}

.server-status {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-sm) var(--spacing-md);
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: 600;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.server-status.online .status-dot {
  background-color: var(--status-playing);
}

.server-status.online .status-label {
  color: var(--status-playing);
}

.server-status.offline .status-dot {
  background-color: var(--status-offline);
}

.server-status.offline .status-label {
  color: var(--status-offline);
}

/* Loading & Offline states */
.loading-state,
.offline-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
  font-size: var(--text-base);
  color: var(--color-text-secondary);
}

.offline-state p {
  color: var(--status-offline);
}

/* Content */
.server-content {
  flex: 1;
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  padding: var(--spacing-lg);
  border: 1px solid var(--color-border);
  min-height: 0;
  overflow: hidden;
}

/* Responsive - Mobile */
@media (max-width: 767px) {
  .media-server-view {
    padding: var(--spacing-md);
  }

  .server-name {
    font-size: var(--text-xl);
  }

  .server-info {
    flex-wrap: wrap;
  }
}
</style>
