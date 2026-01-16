<template>
    <div class="shuffle-control">
        <button
            class="shuffle-button"
            :class="{ loading: isLoading }"
            @click.stop="handleShuffle"
            :disabled="isLoading"
            title="Mélanger la queue"
        >
            <Shuffle :size="24" />
        </button>
    </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { Shuffle } from "lucide-vue-next";
import { api } from "@/services/pmocontrol/api";

const props = defineProps<{
    rendererId: string;
}>();

const isLoading = ref(false);

async function handleShuffle() {
    if (isLoading.value) return;

    isLoading.value = true;
    try {
        await api.shuffleQueue(props.rendererId);
    } catch (error) {
        console.error("Erreur lors du shuffle de la queue:", error);
    } finally {
        isLoading.value = false;
    }
}
</script>

<style scoped>
.shuffle-control {
    position: relative;
}

.shuffle-button {
    position: relative;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    background: rgba(255, 255, 255, 0.2);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.3);
    border-radius: 50%;
    cursor: pointer;
    transition: all 0.3s ease;
    color: var(--color-text);
}

.shuffle-button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.3);
    transform: scale(1.1);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.shuffle-button:active:not(:disabled) {
    transform: scale(0.95);
}

.shuffle-button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
}

.shuffle-button.loading {
    animation: pulse 1s infinite;
}

@keyframes pulse {
    0%,
    100% {
        opacity: 1;
    }
    50% {
        opacity: 0.5;
    }
}

@media (prefers-color-scheme: dark) {
    .shuffle-button {
        background: rgba(255, 255, 255, 0.15);
    }

    .shuffle-button:hover:not(:disabled) {
        background: rgba(255, 255, 255, 0.25);
    }
}

@media (max-width: 768px) {
    .shuffle-button {
        width: 36px;
        height: 36px;
    }
}
</style>
