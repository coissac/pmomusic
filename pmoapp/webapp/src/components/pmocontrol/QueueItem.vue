<script setup lang="ts">
import { Music, Play } from 'lucide-vue-next'
import type { QueueItem } from '@/services/pmocontrol/types'

defineProps<{
  item: QueueItem
  isCurrent: boolean
}>()
</script>

<template>
  <div :class="['queue-item', { current: isCurrent }]">
    <!-- Indicateur piste en cours -->
    <div class="current-indicator" v-if="isCurrent">
      <Play :size="16" fill="currentColor" />
    </div>

    <!-- Index (1-based pour l'affichage) -->
    <span class="item-index">{{ item.index + 1 }}</span>

    <!-- Cover miniature (placeholder pour l'instant, on ajoutera la cover plus tard) -->
    <div class="item-cover">
      <Music :size="20" />
    </div>

    <!-- Métadonnées -->
    <div class="item-metadata">
      <div class="item-title">{{ item.title || 'Sans titre' }}</div>
      <div class="item-artist">
        {{ item.artist || 'Artiste inconnu' }}
        <span v-if="item.album"> • {{ item.album }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.queue-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: var(--spacing-sm);
  border-radius: var(--radius-md);
  transition: background-color var(--transition-fast);
  position: relative;
}

.queue-item:hover {
  background-color: var(--color-bg-secondary);
}

.queue-item.current {
  background-color: var(--status-playing-bg);
  border: 1px solid var(--status-playing);
  font-weight: 600;
}

.current-indicator {
  position: absolute;
  left: var(--spacing-xs);
  color: var(--status-playing);
  display: flex;
  align-items: center;
}

.item-index {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  min-width: 2rem;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.queue-item.current .item-index {
  margin-left: var(--spacing-lg);
}

.item-cover {
  width: 48px;
  height: 48px;
  background-color: var(--color-bg-tertiary);
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.item-metadata {
  flex: 1;
  min-width: 0;
}

.item-title {
  font-size: var(--text-base);
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.queue-item.current .item-title {
  color: var(--status-playing);
}

.item-artist {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
