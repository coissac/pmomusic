<script setup lang="ts">
import { ref, watch, toRef } from "vue";
import { useRenderer, useRenderers } from "@/composables/useRenderers";
import { useUIStore } from "@/stores/ui";
import { Volume2, VolumeX } from "lucide-vue-next";

const props = defineProps<{
    rendererId: string;
}>();

const { state } = useRenderer(toRef(props, "rendererId"));
const { setVolume, toggleMute } = useRenderers();
const uiStore = useUIStore();

const localVolume = ref(state.value?.volume ?? 50);

// Synchroniser localVolume avec le state
watch(
    () => state.value?.volume,
    (newVolume) => {
        if (newVolume !== undefined && newVolume !== null) {
            localVolume.value = newVolume;
        }
    },
    { immediate: true },
);

// Debounce pour le slider
let debounceTimer: number | null = null;
function handleVolumeChange(event: Event) {
    const target = event.target as HTMLInputElement;
    localVolume.value = parseInt(target.value, 10);

    // Debounce: attendre 300ms avant d'envoyer à l'API
    if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
    }

    debounceTimer = window.setTimeout(async () => {
        try {
            await setVolume(props.rendererId, localVolume.value);
        } catch (error) {
            uiStore.notifyError(
                `Impossible de régler le volume: ${error instanceof Error ? error.message : "Erreur inconnue"}`,
            );
        }
        debounceTimer = null;
    }, 300);
}

async function handleToggleMute() {
    try {
        await toggleMute(props.rendererId);
    } catch (error) {
        uiStore.notifyError(
            `Impossible de basculer le mode muet: ${error instanceof Error ? error.message : "Erreur inconnue"}`,
        );
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
    padding: var(--spacing-md) var(--spacing-lg);
    border-radius: 20px;
    background: rgba(255, 255, 255, 0.12);
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    border: 1px solid rgba(255, 255, 255, 0.18);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
}

@media (prefers-color-scheme: dark) {
    .volume-control {
        background: rgba(0, 0, 0, 0.3);
        border-color: rgba(255, 255, 255, 0.12);
    }
}

/* Bouton mute avec effet glass */
.volume-control .btn-icon {
    width: 44px;
    height: 44px;
    min-width: 44px;
    min-height: 44px;
    background: rgba(255, 255, 255, 0.15);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    transition: all 0.3s ease;
}

.volume-control .btn-icon:hover {
    background: rgba(255, 255, 255, 0.25);
    border-color: rgba(255, 255, 255, 0.3);
    transform: scale(1.1);
}

.volume-control .btn-icon:active {
    transform: scale(1);
}

/* Slider avec effet glass */
.volume-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    background: rgba(255, 255, 255, 0.2);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border-radius: 8px;
    height: 8px;
    outline: none;
    transition: all 0.3s ease;
}

.volume-slider:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.25);
}

/* Thumb Webkit (Chrome, Safari) */
.volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.9);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    box-shadow:
        0 4px 12px rgba(0, 0, 0, 0.2),
        inset 0 1px 0 rgba(255, 255, 255, 0.5);
    border: 2px solid rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.2s ease;
}

.volume-slider::-webkit-slider-thumb:hover {
    transform: scale(1.2);
    box-shadow:
        0 6px 16px rgba(0, 0, 0, 0.3),
        inset 0 1px 0 rgba(255, 255, 255, 0.5);
}

.volume-slider::-webkit-slider-thumb:active {
    transform: scale(1.1);
}

/* Thumb Mozilla */
.volume-slider::-moz-range-thumb {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.9);
    backdrop-filter: blur(10px);
    box-shadow:
        0 4px 12px rgba(0, 0, 0, 0.2),
        inset 0 1px 0 rgba(255, 255, 255, 0.5);
    border: 2px solid rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.2s ease;
}

.volume-slider::-moz-range-thumb:hover {
    transform: scale(1.2);
}

.volume-value {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-text);
    min-width: 2.5rem;
    text-align: right;
}

.volume-slider:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    background: rgba(255, 255, 255, 0.1);
}

.volume-slider:disabled::-webkit-slider-thumb {
    cursor: not-allowed;
    background: rgba(255, 255, 255, 0.5);
}

.volume-slider:disabled::-moz-range-thumb {
    cursor: not-allowed;
    background: rgba(255, 255, 255, 0.5);
}

@media (prefers-color-scheme: dark) {
    .volume-control .btn-icon {
        background: rgba(255, 255, 255, 0.1);
        border-color: rgba(255, 255, 255, 0.15);
    }

    .volume-control .btn-icon:hover {
        background: rgba(255, 255, 255, 0.2);
        border-color: rgba(255, 255, 255, 0.25);
    }

    .volume-slider {
        background: rgba(255, 255, 255, 0.15);
    }
}

/* Fallback pour navigateurs sans backdrop-filter */
@supports not (backdrop-filter: blur(20px)) {
    .volume-control {
        background: rgba(255, 255, 255, 0.95);
    }

    .volume-slider {
        background: rgba(200, 200, 200, 0.8);
    }

    @media (prefers-color-scheme: dark) {
        .volume-control {
            background: rgba(0, 0, 0, 0.95);
        }

        .volume-slider {
            background: rgba(100, 100, 100, 0.8);
        }
    }
}

/* Mode kiosque - compactage pour petites hauteurs (800x600) */
@media (max-height: 700px) and (orientation: landscape) {
    .volume-control {
        padding: var(--spacing-sm) var(--spacing-md);
        gap: var(--spacing-sm);
    }

    .volume-control .btn-icon {
        width: 36px;
        height: 36px;
        min-width: 36px;
        min-height: 36px;
    }

    .volume-slider {
        height: 6px;
    }

    .volume-slider::-webkit-slider-thumb {
        width: 18px;
        height: 18px;
    }

    .volume-slider::-moz-range-thumb {
        width: 18px;
        height: 18px;
    }

    .volume-value {
        font-size: 11px;
        min-width: 2rem;
    }
}
</style>
