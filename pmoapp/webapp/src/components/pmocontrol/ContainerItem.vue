<script setup lang="ts">
import { computed } from 'vue'
import type { ContainerEntry } from '@/services/pmocontrol/types'
import { Folder, Music } from 'lucide-vue-next'
import ActionMenu from './ActionMenu.vue'

const props = defineProps<{
  entry: ContainerEntry
  serverId: string
  showActions?: boolean
}>()

const emit = defineEmits<{
  browse: [containerId: string]
  playNow: [containerId: string, rendererId: string]
  addToQueue: [containerId: string, rendererId: string]
  attachPlaylist: [containerId: string, rendererId: string]
}>()

const iconComponent = computed(() => {
  const cls = props.entry.class.toLowerCase()
  if (cls.includes('playlist')) return Music
  if (cls.includes('album')) return Music
  return Folder
})

const containerType = computed(() => {
  const cls = props.entry.class.toLowerCase()
  if (cls.includes('playlist')) return 'Playlist'
  if (cls.includes('album')) return 'Album'
  if (cls.includes('artist')) return 'Artiste'
  if (cls.includes('genre')) return 'Genre'
  return 'Dossier'
})

function handleBrowse() {
  emit('browse', props.entry.id)
}

function handlePlayNow(rendererId: string) {
  emit('playNow', props.entry.id, rendererId)
}

function handleAddToQueue(rendererId: string) {
  emit('addToQueue', props.entry.id, rendererId)
}

function handleAttachPlaylist(rendererId: string) {
  emit('attachPlaylist', props.entry.id, rendererId)
}
</script>

<template>
  <div class="container-item">
    <!-- Main content (clickable) -->
    <button class="container-content" @click="handleBrowse">
      <div class="container-icon">
        <component :is="iconComponent" :size="24" />
      </div>
      <div class="container-metadata">
        <div class="container-title">{{ entry.title }}</div>
        <div class="container-details">
          <span class="container-type">{{ containerType }}</span>
          <span v-if="entry.child_count !== null" class="container-count">
            {{ entry.child_count }} élément{{ entry.child_count > 1 ? 's' : '' }}
          </span>
        </div>
      </div>
    </button>

    <!-- Actions menu -->
    <div class="container-actions">
      <ActionMenu
        type="container"
        :entry-id="entry.id"
        :server-id="serverId"
        @play-now="handlePlayNow"
        @add-to-queue="handleAddToQueue"
        @attach-playlist="handleAttachPlaylist"
      />
    </div>
  </div>
</template>

<style scoped>
.container-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-sm);
  border-radius: var(--radius-md);
  transition: background-color var(--transition-fast);
  border: 1px solid transparent;
}

.container-item:hover {
  background-color: var(--color-bg-secondary);
  border-color: var(--color-border);
}

.container-content {
  flex: 1;
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  text-align: left;
  min-width: 0;
}

.container-icon {
  flex-shrink: 0;
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--color-bg-tertiary);
  border-radius: var(--radius-sm);
  color: var(--color-primary);
}

.container-metadata {
  flex: 1;
  min-width: 0;
}

.container-title {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: var(--spacing-xs);
}

.container-details {
  display: flex;
  gap: var(--spacing-sm);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.container-type {
  font-weight: 500;
}

.container-count::before {
  content: '•';
  margin-right: var(--spacing-sm);
}

.container-actions {
  flex-shrink: 0;
}

.btn-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.btn-icon:hover {
  background-color: var(--color-bg-tertiary);
  color: var(--color-text);
}
</style>
