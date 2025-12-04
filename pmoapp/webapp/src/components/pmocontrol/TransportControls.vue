<script setup lang="ts">
import { computed } from 'vue'
import { useRenderersStore } from '@/stores/renderers'
import { Play, Pause, Square, SkipForward } from 'lucide-vue-next'

const props = defineProps<{
  rendererId: string
}>()

const renderersStore = useRenderersStore()

const state = computed(() => renderersStore.getStateById(props.rendererId))
const isPlaying = computed(() => state.value?.transport_state === 'PLAYING')
const isPaused = computed(() => state.value?.transport_state === 'PAUSED')
const isStopped = computed(() => state.value?.transport_state === 'STOPPED' || state.value?.transport_state === 'NO_MEDIA')

async function handlePlay() {
  try {
    await renderersStore.play(props.rendererId)
  } catch (error) {
    console.error('Erreur play:', error)
  }
}

async function handlePause() {
  try {
    await renderersStore.pause(props.rendererId)
  } catch (error) {
    console.error('Erreur pause:', error)
  }
}

async function handleStop() {
  try {
    await renderersStore.stop(props.rendererId)
  } catch (error) {
    console.error('Erreur stop:', error)
  }
}

async function handleNext() {
  try {
    await renderersStore.next(props.rendererId)
  } catch (error) {
    console.error('Erreur next:', error)
  }
}
</script>

<template>
  <div class="transport-controls">
    <button
      class="btn btn-icon btn-primary"
      :disabled="isPlaying"
      @click="handlePlay"
      title="Lecture"
    >
      <Play :size="20" />
    </button>

    <button
      class="btn btn-icon"
      :disabled="isPaused || isStopped"
      @click="handlePause"
      title="Pause"
    >
      <Pause :size="20" />
    </button>

    <button
      class="btn btn-icon"
      :disabled="isStopped"
      @click="handleStop"
      title="Stop"
    >
      <Square :size="20" />
    </button>

    <button
      class="btn btn-icon"
      :disabled="!state?.queue_len"
      @click="handleNext"
      title="Suivant"
    >
      <SkipForward :size="20" />
    </button>
  </div>
</template>

<style scoped>
.transport-controls {
  display: flex;
  gap: var(--spacing-sm);
  align-items: center;
  justify-content: center;
}
</style>
