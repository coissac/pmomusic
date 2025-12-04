<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue'
import { useRenderersStore } from '@/stores/renderers'
import QueueItem from './QueueItem.vue'
import { Link } from 'lucide-vue-next'

const props = defineProps<{
  rendererId: string
}>()

const renderersStore = useRenderersStore()

const queue = computed(() => renderersStore.getQueueById(props.rendererId))
const binding = computed(() => renderersStore.getBindingById(props.rendererId))

const isAttached = computed(() => !!binding.value)

const queueContainer = ref<HTMLElement | null>(null)

// Auto-scroll vers la piste courante lors de l'ouverture
watch(() => queue.value?.current_index, async (currentIndex) => {
  if (currentIndex !== null && currentIndex !== undefined && queueContainer.value) {
    await nextTick()
    const currentItem = queueContainer.value.querySelector('.queue-item.current')
    if (currentItem) {
      currentItem.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }
}, { immediate: true })
</script>

<template>
  <div class="queue-viewer">
    <!-- Header avec indication de binding -->
    <div class="queue-header">
      <h3 class="queue-title">
        File d'attente
        <span class="queue-count" v-if="queue?.items.length">
          ({{ queue.items.length }})
        </span>
      </h3>

      <!-- Indicateur playlist attachée -->
      <div v-if="isAttached" class="binding-indicator">
        <Link :size="16" />
        <span class="binding-text">
          Attachée à une playlist
        </span>
      </div>
    </div>

    <!-- Liste des items -->
    <div v-if="queue?.items.length" class="queue-list" ref="queueContainer">
      <QueueItem
        v-for="item in queue.items"
        :key="item.index"
        :item="item"
        :is-current="item.index === queue.current_index"
      />
    </div>

    <!-- État vide -->
    <div v-else class="queue-empty">
      <p>Aucun élément dans la file d'attente</p>
    </div>
  </div>
</template>

<style scoped>
.queue-viewer {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
  height: 100%;
}

.queue-header {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}

.queue-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.queue-count {
  font-size: var(--text-sm);
  font-weight: 400;
  color: var(--color-text-secondary);
}

.binding-indicator {
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-xs);
  padding: var(--spacing-xs) var(--spacing-sm);
  background-color: var(--status-playing-bg);
  color: var(--status-playing);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: 500;
  border: 1px solid var(--status-playing);
  width: fit-content;
}

.binding-text {
  font-size: var(--text-xs);
}

.queue-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
  padding-right: var(--spacing-xs);
}

/* Scrollbar styling */
.queue-list::-webkit-scrollbar {
  width: 6px;
}

.queue-list::-webkit-scrollbar-track {
  background: var(--color-bg-secondary);
  border-radius: var(--radius-full);
}

.queue-list::-webkit-scrollbar-thumb {
  background: var(--color-border);
  border-radius: var(--radius-full);
}

.queue-list::-webkit-scrollbar-thumb:hover {
  background: var(--color-text-tertiary);
}

.queue-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: var(--text-base);
  text-align: center;
  padding: var(--spacing-xl);
}
</style>
