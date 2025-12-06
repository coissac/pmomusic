<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import type { MediaServerSummary } from '@/services/pmocontrol/types'
import { Server, Circle } from 'lucide-vue-next'

const props = defineProps<{
  server: MediaServerSummary
}>()

const router = useRouter()

const statusClass = computed(() => props.server.online ? 'online' : 'offline')
const statusLabel = computed(() => props.server.online ? 'En ligne' : 'Hors ligne')

function goToServer() {
  if (props.server.online) {
    router.push(`/server/${props.server.id}`)
  }
}
</script>

<template>
  <div :class="['media-server-card', { offline: !server.online }]">
    <!-- Header -->
    <div class="card-header">
      <div class="server-icon">
        <Server :size="40" />
      </div>
      <div class="header-content">
        <h3 class="server-name">{{ server.friendly_name }}</h3>
        <p class="server-model">{{ server.model_name }}</p>
      </div>
    </div>

    <!-- Status -->
    <div class="card-status">
      <Circle :size="12" :class="['status-indicator', statusClass]" fill="currentColor" />
      <span :class="['status-label', statusClass]">{{ statusLabel }}</span>
    </div>

    <!-- Browse Button -->
    <button
      class="btn btn-primary card-browse-btn"
      @click="goToServer"
      :disabled="!server.online"
    >
      Parcourir
    </button>
  </div>
</template>

<style scoped>
.media-server-card {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
  padding: var(--spacing-lg);
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border);
  transition: all var(--transition-normal);
}

.media-server-card:hover {
  border-color: var(--color-primary);
  box-shadow: var(--shadow-md);
}

.media-server-card.offline {
  opacity: 0.6;
  filter: grayscale(0.5);
}

/* Header */
.card-header {
  display: flex;
  gap: var(--spacing-md);
  align-items: center;
}

.server-icon {
  flex-shrink: 0;
  width: 64px;
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--color-bg-tertiary);
  border-radius: var(--radius-md);
  color: var(--color-primary);
}

.header-content {
  flex: 1;
  min-width: 0;
}

.server-name {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--color-text);
  margin: 0 0 var(--spacing-xs);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.server-model {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Status */
.card-status {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-sm);
  background-color: var(--color-bg);
  border-radius: var(--radius-md);
}

.status-indicator {
  flex-shrink: 0;
}

.status-indicator.online {
  color: var(--status-playing);
}

.status-indicator.offline {
  color: var(--status-offline);
}

.status-label {
  font-size: var(--text-sm);
  font-weight: 600;
}

.status-label.online {
  color: var(--status-playing);
}

.status-label.offline {
  color: var(--status-offline);
}

/* Browse Button */
.card-browse-btn {
  width: 100%;
}

.card-browse-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
