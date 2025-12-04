<script setup lang="ts">
import type { ContainerEntry } from '@/services/pmocontrol/types'
import { Music } from 'lucide-vue-next'
import ActionMenu from './ActionMenu.vue'

const props = defineProps<{
  entry: ContainerEntry
  serverId: string
  showActions?: boolean
}>()

const emit = defineEmits<{
  playNow: [itemId: string, rendererId: string]
  addToQueue: [itemId: string, rendererId: string]
}>()

function handleImageError(event: Event) {
  const img = event.target as HTMLImageElement
  img.style.display = 'none'
  const placeholder = img.nextElementSibling
  if (placeholder && placeholder instanceof HTMLElement) {
    placeholder.style.display = 'flex'
  }
}

function handlePlayNow(rendererId: string) {
  emit('playNow', props.entry.id, rendererId)
}

function handleAddToQueue(rendererId: string) {
  emit('addToQueue', props.entry.id, rendererId)
}
</script>

<template>
  <div class="media-item">
    <!-- Cover miniature -->
    <div class="media-cover">
      <img
        v-if="entry.album_art_uri"
        :src="entry.album_art_uri"
        :alt="entry.album || entry.title"
        class="cover-image"
        loading="lazy"
        @error="handleImageError"
      />
      <div class="cover-placeholder" :style="{ display: entry.album_art_uri ? 'none' : 'flex' }">
        <Music :size="20" />
      </div>
    </div>

    <!-- Metadata -->
    <div class="media-metadata">
      <div class="media-title">{{ entry.title }}</div>
      <div class="media-details">
        <span v-if="entry.artist" class="media-artist">{{ entry.artist }}</span>
        <span v-if="entry.album" class="media-album">{{ entry.album }}</span>
      </div>
    </div>

    <!-- Actions menu -->
    <div class="media-actions">
      <ActionMenu
        type="item"
        :entry-id="entry.id"
        :server-id="serverId"
        @play-now="handlePlayNow"
        @add-to-queue="handleAddToQueue"
      />
    </div>
  </div>
</template>

<style scoped>
.media-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: var(--spacing-sm);
  border-radius: var(--radius-md);
  transition: background-color var(--transition-fast);
  border: 1px solid transparent;
}

.media-item:hover {
  background-color: var(--color-bg-secondary);
  border-color: var(--color-border);
}

/* Cover */
.media-cover {
  position: relative;
  flex-shrink: 0;
  width: 48px;
  height: 48px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  background-color: var(--color-bg-tertiary);
}

.cover-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.cover-placeholder {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
}

/* Metadata */
.media-metadata {
  flex: 1;
  min-width: 0;
}

.media-title {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: var(--spacing-xs);
}

.media-details {
  display: flex;
  gap: var(--spacing-sm);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.media-artist {
  flex-shrink: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.media-album {
  flex-shrink: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.media-album::before {
  content: '•';
  margin-right: var(--spacing-sm);
}

/* Actions */
.media-actions {
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
