<script setup lang="ts">
import { computed, ref, watch, nextTick, toRef } from "vue";
import { RecycleScroller } from "vue-virtual-scroller";
import "vue-virtual-scroller/dist/vue-virtual-scroller.css";
import { useRenderer } from "@/composables/useRenderers";
import QueueItem from "./QueueItem.vue";
import { Link, Radio, RefreshCw } from "lucide-vue-next";
import type { QueueItem as QueueItemType } from "@/services/pmocontrol/types";

const props = defineProps<{
    rendererId: string;
}>();

const emit = defineEmits<{
    clickItem: [item: QueueItemType];
}>();

const { queue, binding, isStream, queueRefreshing } = useRenderer(toRef(props, "rendererId"));

const isAttached = computed(() => !!binding.value);

const queueContainer = ref<any>(null);

function handleItemClick(item: QueueItemType) {
    emit("clickItem", item);
}

// Auto-scroll vers la piste courante lors de l'ouverture
watch(
    () => queue.value?.current_index,
    async (currentIndex) => {
        if (
            currentIndex !== null &&
            currentIndex !== undefined &&
            queueContainer.value
        ) {
            await nextTick();
            queueContainer.value.scrollToItem(currentIndex);
        }
    },
    { immediate: true },
);
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

            <!-- Indicateurs de status -->
            <div class="status-indicators">
                <!-- Indicateur playlist attachée -->
                <div v-if="isAttached" class="binding-indicator">
                    <Link :size="16" />
                    <span class="binding-text"> Attachée à une playlist </span>
                </div>

                <!-- Indicateur web radio -->
                <div v-if="isStream" class="stream-indicator">
                    <Radio :size="16" />
                    <span class="stream-text"> Web Radio </span>
                </div>

                <!-- Indicateur de mise à jour de queue -->
                <div v-if="queueRefreshing" class="refresh-indicator">
                    <RefreshCw :size="14" class="refresh-icon" />
                    <span class="refresh-text"> Mise à jour... </span>
                </div>
            </div>
        </div>

        <!-- Liste des items virtualisée -->
        <RecycleScroller
            v-if="queue?.items.length"
            class="queue-list"
            :items="queue.items"
            :item-size="64"
            key-field="index"
            v-slot="{ item }"
            ref="queueContainer"
            :min-item-size="64"
        >
            <QueueItem
                :item="item"
                :is-current="item.index === queue.current_index"
                @click="handleItemClick"
            />
        </RecycleScroller>

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
    width: 100%;
    min-width: 0;
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

.status-indicators {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-sm);
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

.stream-indicator {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-xs);
    padding: var(--spacing-xs) var(--spacing-sm);
    background-color: rgba(147, 51, 234, 0.1);
    color: #9333ea;
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    font-weight: 500;
    border: 1px solid #9333ea;
    width: fit-content;
}

.stream-text {
    font-size: var(--text-xs);
}

.refresh-indicator {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-xs);
    padding: var(--spacing-xs) var(--spacing-sm);
    background-color: var(--color-bg-secondary);
    color: var(--color-text-secondary);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    font-weight: 500;
    border: 1px solid var(--color-border);
    width: fit-content;
}

.refresh-icon {
    animation: spin 1s linear infinite;
}

.refresh-text {
    font-size: var(--text-xs);
}

@keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
}

.queue-list {
    width: 100%;
    flex: 1;
    overflow-y: auto;
    padding-right: var(--spacing-xs);
    min-width: 0;
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
