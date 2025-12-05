<script setup lang="ts">
import { computed, toRef } from 'vue'
import { useRenderer } from '@/composables/useRenderers'
import { Music } from 'lucide-vue-next'

const props = defineProps<{
  rendererId: string
}>()

const { state } = useRenderer(toRef(props, 'rendererId'))
const metadata = computed(() => state.value?.current_track)

// Calcul du pourcentage de progression
const progressPercent = computed(() => {
  const position = state.value?.position_ms
  const duration = state.value?.duration_ms
  if (position && duration && duration > 0) {
    return (position / duration) * 100
  }
  return 0
})

// Formater la durée en MM:SS
function formatTime(ms: number | null | undefined): string {
  if (!ms) return '--:--'
  const totalSeconds = Math.floor(ms / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

const currentTime = computed(() => formatTime(state.value?.position_ms))
const totalTime = computed(() => formatTime(state.value?.duration_ms))

const hasCover = computed(() => !!metadata.value?.album_art_uri)

function handleImageError(event: Event) {
  const img = event.target as HTMLImageElement
  img.style.display = 'none'
}
</script>

<template>
  <div class="current-track">
    <!-- Cover Art -->
    <div class="cover-container">
      <img
        v-if="hasCover"
        :src="metadata?.album_art_uri!"
        :alt="metadata?.album || 'Album cover'"
        class="cover-image"
        loading="lazy"
        @error="handleImageError"
      />
      <div v-else class="cover-placeholder">
        <Music :size="64" />
      </div>
    </div>

    <!-- Metadata -->
    <div class="metadata">
      <h2 class="title">{{ metadata?.title || 'Aucun titre' }}</h2>
      <p class="artist">{{ metadata?.artist || 'Artiste inconnu' }}</p>
      <p class="album" v-if="metadata?.album">{{ metadata.album }}</p>
    </div>

    <!-- Progress Bar -->
    <div class="progress-section">
      <div class="progress-bar">
        <div
          class="progress-bar-fill"
          :style="{ width: `${progressPercent}%` }"
        ></div>
      </div>
      <div class="time-display">
        <span>{{ currentTime }}</span>
        <span>{{ totalTime }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.current-track {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
}

.cover-container {
  width: 100%;
  aspect-ratio: 1;
  max-width: 300px;
  margin: 0 auto;
  border-radius: var(--radius-lg);
  overflow: hidden;
  background-color: var(--color-bg-secondary);
  box-shadow: var(--shadow-lg);
}

.cover-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.cover-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
}

.metadata {
  text-align: center;
}

.title {
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
  margin: 0 0 var(--spacing-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.artist {
  font-size: var(--text-lg);
  color: var(--color-text-secondary);
  margin: 0 0 var(--spacing-xs);
}

.album {
  font-size: var(--text-base);
  color: var(--color-text-tertiary);
  margin: 0;
}

.progress-section {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
}

.time-display {
  display: flex;
  justify-content: space-between;
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  font-variant-numeric: tabular-nums;
}

/* Responsive */
@media (min-width: 768px) {
  .cover-container {
    max-width: 250px;
  }
}

@media (min-width: 1024px) {
  .cover-container {
    max-width: 300px;
  }
}
</style>
