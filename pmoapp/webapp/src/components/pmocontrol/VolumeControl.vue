<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useRenderersStore } from '@/stores/renderers'
import { Volume2, VolumeX } from 'lucide-vue-next'

const props = defineProps<{
  rendererId: string
}>()

const renderersStore = useRenderersStore()

const state = computed(() => renderersStore.getStateById(props.rendererId))
const localVolume = ref(state.value?.volume ?? 50)

// Synchroniser localVolume avec le state
watch(() => state.value?.volume, (newVolume) => {
  if (newVolume !== undefined && newVolume !== null) {
    localVolume.value = newVolume
  }
}, { immediate: true })

// Debounce pour le slider
let debounceTimer: number | null = null
function handleVolumeChange(event: Event) {
  const target = event.target as HTMLInputElement
  localVolume.value = parseInt(target.value, 10)

  // Debounce: attendre 300ms avant d'envoyer à l'API
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer)
  }

  debounceTimer = window.setTimeout(async () => {
    try {
      await renderersStore.setVolume(props.rendererId, localVolume.value)
    } catch (error) {
      console.error('Erreur set volume:', error)
    }
    debounceTimer = null
  }, 300)
}

async function handleToggleMute() {
  try {
    await renderersStore.toggleMute(props.rendererId)
  } catch (error) {
    console.error('Erreur toggle mute:', error)
  }
}
</script>

<template>
  <div class="volume-control">
    <button
      class="btn btn-icon"
      @click="handleToggleMute"
      :title="state?.mute ? 'Réactiver le son' : 'Couper le son'"
    >
      <VolumeX v-if="state?.mute" :size="20" />
      <Volume2 v-else :size="20" />
    </button>

    <input
      type="range"
      min="0"
      max="100"
      :value="localVolume"
      @input="handleVolumeChange"
      class="volume-slider"
      :disabled="state?.mute ?? false"
    />

    <span class="volume-value">{{ localVolume }}</span>
  </div>
</template>

<style scoped>
.volume-control {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  width: 100%;
  max-width: 300px;
}

.volume-slider {
  flex: 1;
}

.volume-value {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text-secondary);
  min-width: 2.5rem;
  text-align: right;
}

.volume-slider:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
