<template>
    <div class="timer-control">
        <button
            class="timer-button"
            :class="{ active: timerState?.active }"
            @click.stop="toggleTimerDialog"
            :title="buttonTitle"
            ref="buttonRef"
        >
            <Clock :size="24" />
            <span
                v-if="timerState?.active && remainingMinutes !== null"
                class="timer-badge"
            >
                {{ remainingMinutes }}
            </span>
        </button>

        <!-- Backdrop et Dialog (téléportés au body) -->
        <Teleport to="body">
            <div
                v-if="showDialog"
                class="timer-backdrop"
                @click="closeDialog"
            ></div>

            <div v-if="showDialog" class="timer-dialog" @click.stop>
                <div class="timer-dialog-content">
                    <div class="timer-header">
                        <h3>Sleep Timer</h3>
                        <button class="close-button" @click="closeDialog">
                            <X :size="18" />
                        </button>
                    </div>

                    <div class="timer-body">
                        <!-- Affichage compact du temps restant -->
                        <div v-if="timerState?.active" class="time-display">
                            {{ formatTime(remainingSeconds) }}
                        </div>

                        <!-- Slider compact -->
                        <div class="slider-section">
                            <div class="slider-value">
                                {{ sliderValue }} min
                            </div>
                            <input
                                type="range"
                                min="0"
                                max="120"
                                step="5"
                                v-model.number="sliderValue"
                                class="timer-slider"
                                @change="handleSliderChange"
                            />
                            <div class="slider-marks">
                                <span>0</span>
                                <span>60</span>
                                <span>120</span>
                            </div>
                        </div>

                        <!-- Bouton annuler (seulement si actif) -->
                        <button
                            v-if="timerState?.active"
                            class="btn-cancel"
                            @click="handleCancel"
                            :disabled="isLoading"
                        >
                            Annuler le timer
                        </button>
                    </div>
                </div>
            </div>
        </Teleport>
    </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { Clock, X } from "lucide-vue-next";
import { api } from "@/services/pmocontrol/api";
import { sse } from "@/services/pmocontrol/sse";
import type {
    SleepTimerState,
    RendererEventPayload,
} from "@/services/pmocontrol/types";

const props = defineProps<{
    rendererId: string;
}>();

const showDialog = ref(false);
const sliderValue = ref(0);
const timerState = ref<SleepTimerState | null>(null);
const isLoading = ref(false);
const buttonRef = ref<HTMLElement | null>(null);
const localRemainingSeconds = ref<number | null>(null);
let countdownInterval: number | null = null;

const remainingSeconds = computed(
    () =>
        localRemainingSeconds.value ??
        timerState.value?.remaining_seconds ??
        null,
);

const remainingMinutes = computed(() => {
    if (remainingSeconds.value === null) return null;
    return Math.ceil(remainingSeconds.value / 60);
});

const buttonTitle = computed(() => {
    if (timerState.value?.active && remainingMinutes.value !== null) {
        return `Sleep timer: ${remainingMinutes.value} min restantes`;
    }
    return "Sleep timer";
});

function formatTime(seconds: number | null): string {
    if (seconds === null) return "--:--";
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
}

function startCountdown() {
    if (countdownInterval !== null) {
        clearInterval(countdownInterval);
    }

    if (timerState.value?.active && timerState.value.remaining_seconds) {
        localRemainingSeconds.value = timerState.value.remaining_seconds;

        countdownInterval = window.setInterval(() => {
            if (
                localRemainingSeconds.value !== null &&
                localRemainingSeconds.value > 0
            ) {
                localRemainingSeconds.value--;
            } else {
                stopCountdown();
            }
        }, 1000);
    }
}

function stopCountdown() {
    if (countdownInterval !== null) {
        clearInterval(countdownInterval);
        countdownInterval = null;
    }
    localRemainingSeconds.value = null;
}

async function fetchTimerState() {
    try {
        const state = await api.getSleepTimer(props.rendererId);
        timerState.value = state;
        if (state.active && state.duration_seconds) {
            sliderValue.value = Math.round(state.duration_seconds / 60);
            startCountdown();
        } else {
            stopCountdown();
        }
    } catch (error) {
        console.error("Erreur lors de la récupération du timer:", error);
        timerState.value = null;
        stopCountdown();
    }
}

// Le slider modifie le timer en temps réel
async function handleSliderChange() {
    if (sliderValue.value === 0) {
        // Si on met à 0, on annule
        await handleCancel();
        return;
    }

    isLoading.value = true;
    try {
        const durationSeconds = sliderValue.value * 60;

        if (timerState.value?.active) {
            await api.updateSleepTimer(props.rendererId, durationSeconds);
        } else {
            await api.startSleepTimer(props.rendererId, durationSeconds);
        }

        await fetchTimerState();
    } catch (error) {
        console.error("Erreur lors de la configuration du timer:", error);
    } finally {
        isLoading.value = false;
    }
}

async function handleCancel() {
    isLoading.value = true;
    try {
        await api.cancelSleepTimer(props.rendererId);
        timerState.value = {
            active: false,
            duration_seconds: 0,
            remaining_seconds: null,
        };
        sliderValue.value = 0;
        closeDialog();
    } catch (error) {
        console.error("Erreur lors de l'annulation du timer:", error);
    } finally {
        isLoading.value = false;
    }
}

function toggleTimerDialog() {
    showDialog.value = !showDialog.value;
}

function closeDialog() {
    showDialog.value = false;
}

let sseUnsubscribe: (() => void) | null = null;

function handleTimerEvent(event: RendererEventPayload) {
    if (event.renderer_id !== props.rendererId) return;

    switch (event.type) {
        case "timer_started":
        case "timer_updated":
            timerState.value = {
                active: true,
                duration_seconds: event.duration_seconds,
                remaining_seconds: event.remaining_seconds,
            };
            startCountdown();
            break;

        case "timer_tick":
            if (timerState.value?.active) {
                timerState.value = {
                    ...timerState.value,
                    remaining_seconds: event.remaining_seconds,
                };
                // Resynchroniser le countdown local
                localRemainingSeconds.value = event.remaining_seconds;
            }
            break;

        case "timer_expired":
        case "timer_cancelled":
            timerState.value = {
                active: false,
                duration_seconds: 0,
                remaining_seconds: null,
            };
            sliderValue.value = 0;
            stopCountdown();
            break;
    }
}

// Surveiller les changements de renderer
watch(
    () => props.rendererId,
    () => {
        // Réinitialiser l'état quand on change de renderer
        stopCountdown();
        showDialog.value = false;
        sliderValue.value = 0;
        timerState.value = null;
        // Charger l'état du nouveau renderer
        fetchTimerState();
    },
);

onMounted(() => {
    fetchTimerState();
    sseUnsubscribe = sse.onRendererEvent(handleTimerEvent);
});

onUnmounted(() => {
    if (sseUnsubscribe) {
        sseUnsubscribe();
    }
    stopCountdown();
});
</script>

<style scoped>
.timer-control {
    position: relative;
}

.timer-button {
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

.timer-button:hover {
    background: rgba(255, 255, 255, 0.3);
    transform: scale(1.1);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.timer-button:active {
    transform: scale(0.95);
}

.timer-button.active {
    background: rgba(34, 197, 94, 0.2);
    color: #22c55e;
    border-color: rgba(34, 197, 94, 0.4);
}

@media (prefers-color-scheme: dark) {
    .timer-button {
        background: rgba(255, 255, 255, 0.15);
    }

    .timer-button:hover {
        background: rgba(255, 255, 255, 0.25);
    }
}

.timer-badge {
    position: absolute;
    top: 2px;
    right: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    font-size: 11px;
    font-weight: 700;
    color: white;
    background: rgba(34, 197, 94, 0.9);
    border: 2px solid var(--color-bg);
    border-radius: 10px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}

.timer-backdrop {
    position: fixed;
    top: 0 !important;
    left: 0 !important;
    right: 0 !important;
    bottom: 0 !important;
    width: 100vw;
    height: 100vh;
    background: rgba(0, 0, 0, 0.5);
    z-index: 999;
    margin: 0;
    padding: 0;
}

.timer-dialog {
    position: fixed;
    bottom: 60px;
    right: 20px;
    z-index: 1000;
}

.timer-dialog-content {
    background: var(--background-secondary, #1f2937);
    border-radius: 12px;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
    width: 280px;
}

.timer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--border-color, rgba(255, 255, 255, 0.1));
}

.timer-header h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary, #ffffff);
}

.close-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: none;
    background: transparent;
    color: var(--text-secondary, #9ca3af);
    cursor: pointer;
    transition: all 0.2s ease;
}

.close-button:hover {
    background: var(--glass-background, rgba(255, 255, 255, 0.1));
    color: var(--text-primary, #ffffff);
}

.timer-body {
    padding: 1rem;
}

.time-display {
    text-align: center;
    font-size: 28px;
    font-weight: 600;
    color: var(--status-playing, #22c55e);
    margin-bottom: 0.75rem;
    font-variant-numeric: tabular-nums;
}

.slider-section {
    margin-bottom: 0.75rem;
}

.slider-value {
    text-align: center;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary, #ffffff);
    margin-bottom: 0.5rem;
}

.timer-slider {
    width: 100%;
    height: 6px;
    border-radius: 3px;
    background: var(--glass-background, rgba(255, 255, 255, 0.1));
    outline: none;
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
}

.timer-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--status-playing, #22c55e);
    cursor: pointer;
    transition: all 0.2s ease;
}

.timer-slider::-webkit-slider-thumb:hover {
    transform: scale(1.2);
}

.timer-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--status-playing, #22c55e);
    border: none;
    cursor: pointer;
    transition: all 0.2s ease;
}

.timer-slider::-moz-range-thumb:hover {
    transform: scale(1.2);
}

.slider-marks {
    display: flex;
    justify-content: space-between;
    margin-top: 0.25rem;
    font-size: 10px;
    color: var(--text-secondary, #9ca3af);
}

.btn-cancel {
    width: 100%;
    padding: 0.5rem;
    border-radius: 6px;
    border: none;
    font-size: 13px;
    font-weight: 500;
    background: var(--glass-background, rgba(255, 255, 255, 0.1));
    color: var(--text-primary, #ffffff);
    cursor: pointer;
    transition: all 0.2s ease;
}

.btn-cancel:hover:not(:disabled) {
    background: var(--glass-background-hover, rgba(255, 255, 255, 0.15));
}

.btn-cancel:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

@media (max-width: 768px) {
    .timer-dialog-content {
        width: 260px;
    }

    .timer-button {
        width: 36px;
        height: 36px;
    }
}
</style>
