<script setup lang="ts">
import { computed, toRef } from 'vue'
import { useRenderer, useRenderers } from '@/composables/useRenderers'
import { useUIStore } from '@/stores/ui'
import { Play, Pause, Square, SkipForward } from 'lucide-vue-next'

const props = defineProps<{
  rendererId: string
}>()

const { state } = useRenderer(toRef(props, 'rendererId'))
const { resumeOrPlayFromQueue, pause, stop, next } = useRenderers()
const uiStore = useUIStore()

const isPlaying = computed(() => state.value?.transport_state === 'PLAYING')
const isPaused = computed(() => state.value?.transport_state === 'PAUSED')
const isStopped = computed(() => state.value?.transport_state === 'STOPPED' || state.value?.transport_state === 'NO_MEDIA')

async function handlePlay() {
  try {
    await resumeOrPlayFromQueue(props.rendererId)
  } catch (error) {
    uiStore.notifyError(`Impossible de démarrer la lecture: ${error instanceof Error ? error.message : 'Erreur inconnue'}`)
  }
}

async function handlePause() {
  try {
    await pause(props.rendererId)
  } catch (error) {
    uiStore.notifyError(`Impossible de mettre en pause: ${error instanceof Error ? error.message : 'Erreur inconnue'}`)
  }
}

async function handleStop() {
  try {
    await stop(props.rendererId)
  } catch (error) {
    uiStore.notifyError(`Impossible d'arrêter la lecture: ${error instanceof Error ? error.message : 'Erreur inconnue'}`)
  }
}

async function handleNext() {
  try {
    await next(props.rendererId)
  } catch (error) {
    uiStore.notifyError(`Impossible de passer au morceau suivant: ${error instanceof Error ? error.message : 'Erreur inconnue'}`)
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
  gap: var(--spacing-md);
  align-items: center;
  justify-content: center;
  padding: var(--spacing-lg);
  border-radius: 24px;
  background: rgba(255, 255, 255, 0.12);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.18);
  box-shadow:
    0 8px 32px 0 rgba(31, 38, 135, 0.15),
    inset 0 1px 0 0 rgba(255, 255, 255, 0.3);
}

@media (prefers-color-scheme: dark) {
  .transport-controls {
    background: rgba(0, 0, 0, 0.3);
    border-color: rgba(255, 255, 255, 0.12);
  }
}

/* Boutons avec effet glass */
.transport-controls .btn-icon {
  width: 56px;
  height: 56px;
  min-width: 56px;
  min-height: 56px;
  background: rgba(255, 255, 255, 0.15);
  backdrop-filter: blur(10px) saturate(150%);
  -webkit-backdrop-filter: blur(10px) saturate(150%);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 50%;
  transition: all 0.3s ease;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  transform: translateZ(0);
}

.transport-controls .btn-icon:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.25);
  border-color: rgba(255, 255, 255, 0.3);
  transform: translateY(-2px);
  box-shadow: 0 6px 16px rgba(0, 0, 0, 0.15);
}

.transport-controls .btn-icon:active:not(:disabled) {
  transform: translateY(0);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.transport-controls .btn-icon:disabled {
  background: rgba(255, 255, 255, 0.05);
  border-color: rgba(255, 255, 255, 0.1);
  opacity: 0.5;
  cursor: not-allowed;
}

/* Bouton primary (play) avec accent vert */
.transport-controls .btn-primary {
  background: rgba(34, 197, 94, 0.3);
  border-color: rgba(34, 197, 94, 0.5);
}

.transport-controls .btn-primary:hover:not(:disabled) {
  background: rgba(34, 197, 94, 0.4);
  border-color: rgba(34, 197, 94, 0.6);
}

@media (prefers-color-scheme: dark) {
  .transport-controls .btn-icon {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.15);
  }

  .transport-controls .btn-icon:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.2);
    border-color: rgba(255, 255, 255, 0.25);
  }
}

/* Fallback pour navigateurs sans backdrop-filter */
@supports not (backdrop-filter: blur(20px)) {
  .transport-controls {
    background: rgba(255, 255, 255, 0.95);
  }

  .transport-controls .btn-icon {
    background: rgba(255, 255, 255, 0.9);
  }

  @media (prefers-color-scheme: dark) {
    .transport-controls {
      background: rgba(0, 0, 0, 0.95);
    }

    .transport-controls .btn-icon {
      background: rgba(255, 255, 255, 0.15);
    }
  }
}

/* Mode kiosque - compactage pour petites hauteurs (800x600) */
@media (max-height: 700px) and (orientation: landscape) {
  .transport-controls {
    gap: var(--spacing-sm);
    padding: var(--spacing-sm) var(--spacing-md);
  }

  .transport-controls .btn-icon {
    width: 44px;
    height: 44px;
    min-width: 44px;
    min-height: 44px;
  }
}
</style>
