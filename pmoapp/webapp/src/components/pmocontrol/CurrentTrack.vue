<script setup lang="ts">
import { computed, toRef, ref } from "vue";
import { useRenderer } from "@/composables/useRenderers";
import { useCoverImage } from "@/composables/useCoverImage";
import { useUIStore } from "@/stores/ui";
import { api } from "@/services/pmocontrol/api";
import { Music, X } from "lucide-vue-next";

const props = defineProps<{
    rendererId: string;
}>();

const { state } = useRenderer(toRef(props, "rendererId"));
const uiStore = useUIStore();
const metadata = computed(() => state.value?.current_track);
const isSeeking = ref(false);
const isDragging = ref(false);
const dragProgress = ref(0);
const seekTargetMs = ref<number | null>(null);

// État pour l'overlay de la cover
const showCoverOverlay = ref(false);
const showMetadata = ref(false);

// Use the new cover image composable
const albumArtUri = computed(() => metadata.value?.album_art_uri);
const {
    imageLoaded,
    imageError,
    coverImageRef,
    cacheBustedUrl,
    handleImageLoad,
    handleImageError,
} = useCoverImage(albumArtUri);

function openCoverOverlay() {
    if (hasCover.value) {
        showCoverOverlay.value = true;
        showMetadata.value = true; // Afficher les métadonnées par défaut
    }
}

function closeCoverOverlay() {
    showCoverOverlay.value = false;
    showMetadata.value = false;
}

// Détecter le clic sur la partie basse de l'image
function handleOverlayContentClick(event: MouseEvent) {
    const target = event.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const clickY = event.clientY - rect.top;
    const height = rect.height;

    // Si le clic est dans le tiers inférieur, toggle les métadonnées
    if (clickY > height * 0.66) {
        showMetadata.value = !showMetadata.value;
    }
}

// Calcul du pourcentage de progression
const progressPercent = computed(() => {
    // Si on est en train de drag, utiliser la position du drag
    if (isDragging.value) {
        return dragProgress.value;
    }

    // Si on a une cible de seek active
    if (seekTargetMs.value !== null) {
        const currentPosition = state.value?.position_ms || 0;

        // Si la position actuelle est proche de la cible (tolérance de 2 secondes)
        if (Math.abs(currentPosition - seekTargetMs.value) < 2000) {
            // On peut libérer le contrôle
            seekTargetMs.value = null;
            dragProgress.value = 0;
        } else {
            // Sinon, continuer à afficher la position cible
            return dragProgress.value;
        }
    }

    const position = state.value?.position_ms;
    const duration = state.value?.duration_ms;
    if (position && duration && duration > 0) {
        return (position / duration) * 100;
    }
    return 0;
});

// Formater la durée en MM:SS
function formatTime(ms: number | null | undefined): string {
    if (!ms) return "--:--";
    const totalSeconds = Math.floor(ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

const currentTime = computed(() => formatTime(state.value?.position_ms));
const totalTime = computed(() => formatTime(state.value?.duration_ms));

const hasCover = computed(
    () => !!metadata.value?.album_art_uri && !imageError.value,
);

// Calculer le pourcentage à partir d'une position X dans la barre
function calculateProgressFromX(
    clientX: number,
    progressBar: HTMLElement,
): number {
    const rect = progressBar.getBoundingClientRect();
    const x = Math.max(0, Math.min(clientX - rect.left, rect.width));
    return (x / rect.width) * 100;
}

// Gestion du drag sur la progress bar
function handleProgressBarMouseDown(event: MouseEvent) {
    const duration = state.value?.duration_ms;
    if (!duration || duration === 0 || isSeeking.value) return;

    const progressBar = event.currentTarget as HTMLElement;
    isDragging.value = true;
    dragProgress.value = calculateProgressFromX(event.clientX, progressBar);

    const handleMouseMove = (e: MouseEvent) => {
        if (!isDragging.value) return;
        dragProgress.value = calculateProgressFromX(e.clientX, progressBar);
    };

    const handleMouseUp = async (e: MouseEvent) => {
        if (!isDragging.value) return;

        const finalPercent = calculateProgressFromX(e.clientX, progressBar);
        const newPositionMs = (finalPercent / 100) * duration;
        const newPositionSeconds = Math.floor(newPositionMs / 1000);

        // Garder dragProgress à jour et définir la cible
        dragProgress.value = finalPercent;
        seekTargetMs.value = newPositionMs;

        // D'abord arrêter le drag pour éviter les mouvements pendant le seek
        isDragging.value = false;

        try {
            isSeeking.value = true;
            await api.seekTo(props.rendererId, newPositionSeconds);
        } catch (error) {
            uiStore.notifyError(
                `Impossible de seek: ${error instanceof Error ? error.message : "Erreur inconnue"}`,
            );
            // En cas d'erreur, réinitialiser
            seekTargetMs.value = null;
            dragProgress.value = 0;
        } finally {
            isSeeking.value = false;
        }

        document.removeEventListener("mousemove", handleMouseMove);
        document.removeEventListener("mouseup", handleMouseUp);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
}

// Gestion du seek sur la progress bar (clic simple)
async function handleProgressBarClick(event: MouseEvent) {
    // Ne rien faire si on vient de drag (géré par mouseup)
    if (isDragging.value) return;

    const duration = state.value?.duration_ms;
    if (!duration || duration === 0 || isSeeking.value) return;

    const progressBar = event.currentTarget as HTMLElement;
    const percent = calculateProgressFromX(event.clientX, progressBar);
    const newPositionMs = (percent / 100) * duration;
    const newPositionSeconds = Math.floor(newPositionMs / 1000);

    // Définir la cible et la position visuelle
    dragProgress.value = percent;
    seekTargetMs.value = newPositionMs;

    try {
        isSeeking.value = true;
        await api.seekTo(props.rendererId, newPositionSeconds);
    } catch (error) {
        uiStore.notifyError(
            `Impossible de seek: ${error instanceof Error ? error.message : "Erreur inconnue"}`,
        );
        // En cas d'erreur, réinitialiser
        seekTargetMs.value = null;
        dragProgress.value = 0;
    } finally {
        isSeeking.value = false;
    }
}

// Fonctions pour la progress bar de l'overlay (réutilisent la même logique)
function handleOverlayProgressBarClick(event: MouseEvent) {
    handleProgressBarClick(event);
}

function handleOverlayProgressBarMouseDown(event: MouseEvent) {
    handleProgressBarMouseDown(event);
}

// Gestion tactile pour la progress bar de l'overlay
function handleOverlayProgressBarTouchStart(event: TouchEvent) {
    const duration = state.value?.duration_ms;
    if (!duration || duration === 0 || isSeeking.value) return;

    const progressBar = event.currentTarget as HTMLElement;
    const touch = event.touches[0];
    if (!touch) return;

    isDragging.value = true;
    dragProgress.value = calculateProgressFromX(touch.clientX, progressBar);

    const handleTouchMove = (e: TouchEvent) => {
        if (!isDragging.value) return;
        const moveTouch = e.touches[0];
        if (!moveTouch) return;
        dragProgress.value = calculateProgressFromX(
            moveTouch.clientX,
            progressBar,
        );
    };

    const handleTouchEnd = async (e: TouchEvent) => {
        if (!isDragging.value) return;

        const endTouch = e.changedTouches[0];
        if (!endTouch) return;

        const finalPercent = calculateProgressFromX(
            endTouch.clientX,
            progressBar,
        );
        const newPositionMs = (finalPercent / 100) * duration;
        const newPositionSeconds = Math.floor(newPositionMs / 1000);

        dragProgress.value = finalPercent;
        seekTargetMs.value = newPositionMs;
        isDragging.value = false;

        try {
            isSeeking.value = true;
            await api.seekTo(props.rendererId, newPositionSeconds);
        } catch (error) {
            uiStore.notifyError(
                `Impossible de seek: ${error instanceof Error ? error.message : "Erreur inconnue"}`,
            );
            seekTargetMs.value = null;
            dragProgress.value = 0;
        } finally {
            isSeeking.value = false;
        }

        document.removeEventListener("touchmove", handleTouchMove);
        document.removeEventListener("touchend", handleTouchEnd);
    };

    document.addEventListener("touchmove", handleTouchMove, { passive: true });
    document.addEventListener("touchend", handleTouchEnd);
}

// Gestion du swipe down pour fermer l'overlay
const swipeStartY = ref(0);
const swipeCurrentY = ref(0);
const isSwiping = ref(false);
const swipeThreshold = 100; // Distance minimale pour fermer

function handleOverlayTouchStart(event: TouchEvent) {
    const touch = event.touches[0];
    if (!touch) return;
    swipeStartY.value = touch.clientY;
    swipeCurrentY.value = touch.clientY;
    isSwiping.value = true;

    // Ajouter les listeners sur le document pour capturer tous les mouvements
    document.addEventListener("touchmove", handleSwipeTouchMove, {
        passive: true,
    });
    document.addEventListener("touchend", handleSwipeTouchEnd);
    document.addEventListener("touchcancel", handleSwipeTouchEnd);
}

function handleSwipeTouchMove(event: TouchEvent) {
    if (!isSwiping.value) return;
    const touch = event.touches[0];
    if (!touch) return;
    swipeCurrentY.value = touch.clientY;
}

function handleSwipeTouchEnd() {
    if (!isSwiping.value) return;

    const swipeDistance = swipeCurrentY.value - swipeStartY.value;

    // Si swipe vers le bas suffisant, fermer l'overlay
    if (swipeDistance > swipeThreshold) {
        closeCoverOverlay();
    }

    isSwiping.value = false;
    swipeStartY.value = 0;
    swipeCurrentY.value = 0;

    // Retirer les listeners
    document.removeEventListener("touchmove", handleSwipeTouchMove);
    document.removeEventListener("touchend", handleSwipeTouchEnd);
    document.removeEventListener("touchcancel", handleSwipeTouchEnd);
}

// Ancienne fonction conservée pour compatibilité template (non utilisée)
function handleOverlayTouchMove(event: TouchEvent) {
    handleSwipeTouchMove(event);
}

function handleOverlayTouchEnd() {
    handleSwipeTouchEnd();
}

// Calcul du décalage visuel pendant le swipe
const swipeOffset = computed(() => {
    if (!isSwiping.value) return 0;
    const offset = swipeCurrentY.value - swipeStartY.value;
    // Ne permettre que le swipe vers le bas (positif)
    return Math.max(0, offset);
});

// Opacité qui diminue pendant le swipe
const swipeOpacity = computed(() => {
    if (!isSwiping.value) return 1;
    const offset = swipeOffset.value;
    // Réduire l'opacité progressivement
    return Math.max(0.3, 1 - offset / 300);
});
</script>

<template>
    <div class="current-track">
        <!-- Cover Art -->
        <div
            class="cover-container"
            :class="{ clickable: hasCover }"
            @click="openCoverOverlay"
        >
            <img
                ref="coverImageRef"
                :style="{
                    opacity:
                        cacheBustedUrl && imageLoaded && !imageError ? 1 : 0,
                    visibility:
                        cacheBustedUrl && imageLoaded && !imageError
                            ? 'visible'
                            : 'hidden',
                    position:
                        cacheBustedUrl && imageLoaded && !imageError
                            ? 'relative'
                            : 'absolute',
                }"
                :src="cacheBustedUrl || ''"
                :alt="metadata?.album || 'Album cover'"
                class="cover-image"
                @load="handleImageLoad"
                @error="handleImageError"
            />
            <div
                v-show="!cacheBustedUrl || imageError || !imageLoaded"
                class="cover-placeholder"
            >
                <Music :size="64" />
            </div>
        </div>

        <!-- Metadata -->
        <div class="metadata">
            <h2 class="title">{{ metadata?.title || "Aucun titre" }}</h2>
            <p class="artist">{{ metadata?.artist || "Artiste inconnu" }}</p>
            <p class="album" v-if="metadata?.album">{{ metadata.album }}</p>
        </div>

        <!-- Progress Bar -->
        <div class="progress-section">
            <div
                class="progress-bar"
                @click="handleProgressBarClick"
                @mousedown="handleProgressBarMouseDown"
                @touchstart="handleOverlayProgressBarTouchStart"
                :class="{ seeking: isSeeking, dragging: isDragging }"
            >
                <div
                    class="progress-bar-fill"
                    :style="{ width: `${progressPercent}%` }"
                >
                    <div class="progress-bar-thumb"></div>
                </div>
            </div>
            <div class="time-display">
                <span>{{ currentTime }}</span>
                <span>{{ totalTime }}</span>
            </div>
        </div>

        <!-- Overlay cover en grand avec effet glassmorphism -->
        <Teleport to="body">
            <Transition name="cover-overlay">
                <div
                    v-if="showCoverOverlay"
                    class="cover-overlay"
                    @click="closeCoverOverlay"
                >
                    <div
                        class="cover-overlay-content"
                        :class="{ swiping: isSwiping }"
                        :style="{
                            transform: `translateY(${swipeOffset}px)`,
                            opacity: swipeOpacity,
                        }"
                        @click.stop="handleOverlayContentClick"
                        @touchstart="handleOverlayTouchStart"
                        @touchmove="handleOverlayTouchMove"
                        @touchend="handleOverlayTouchEnd"
                    >
                        <button
                            class="cover-overlay-close"
                            @click="closeCoverOverlay"
                            title="Fermer"
                        >
                            <X :size="24" />
                        </button>
                        <img
                            v-if="hasCover && cacheBustedUrl"
                            :src="cacheBustedUrl"
                            :alt="metadata?.album || 'Album cover'"
                            class="cover-overlay-image"
                        />
                        <Transition name="metadata-fade">
                            <div
                                v-if="metadata && showMetadata"
                                class="cover-overlay-metadata"
                            >
                                <div class="overlay-metadata-text">
                                    <h2>{{ metadata.title }}</h2>
                                    <p class="artist">{{ metadata.artist }}</p>
                                    <p v-if="metadata.album" class="album">
                                        {{ metadata.album }}
                                    </p>
                                </div>
                                <div class="overlay-progress-section">
                                    <div
                                        class="overlay-progress-bar"
                                        @click.stop="
                                            handleOverlayProgressBarClick
                                        "
                                        @mousedown.stop="
                                            handleOverlayProgressBarMouseDown
                                        "
                                        @touchstart.stop="
                                            handleOverlayProgressBarTouchStart
                                        "
                                        :class="{
                                            seeking: isSeeking,
                                            dragging: isDragging,
                                        }"
                                    >
                                        <div
                                            class="overlay-progress-bar-fill"
                                            :style="{
                                                width: `${progressPercent}%`,
                                            }"
                                        >
                                            <div
                                                class="overlay-progress-bar-thumb"
                                            ></div>
                                        </div>
                                    </div>
                                    <div class="overlay-time-display">
                                        <span>{{ currentTime }}</span>
                                        <span>{{ totalTime }}</span>
                                    </div>
                                </div>
                            </div>
                        </Transition>
                    </div>
                </div>
            </Transition>
        </Teleport>
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
    transition:
        transform 0.3s ease,
        box-shadow 0.3s ease;
}

.cover-container.clickable {
    cursor: pointer;
}

.cover-container.clickable:hover {
    transform: scale(1.02);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
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
    padding: 0 10px;
}

.progress-bar {
    position: relative;
    width: 100%;
    height: 6px;
    background: linear-gradient(
        to bottom,
        rgba(255, 255, 255, 0.15) 0%,
        rgba(255, 255, 255, 0.25) 50%,
        rgba(255, 255, 255, 0.15) 100%
    );
    border-radius: 3px;
    cursor: pointer;
    transition: background 0.2s ease;
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.3);
    overflow: visible;
}

.progress-bar:hover {
    background: linear-gradient(
        to bottom,
        rgba(255, 255, 255, 0.2) 0%,
        rgba(255, 255, 255, 0.35) 50%,
        rgba(255, 255, 255, 0.2) 100%
    );
}

.progress-bar.dragging {
    cursor: grabbing;
    background: linear-gradient(
        to bottom,
        rgba(255, 255, 255, 0.2) 0%,
        rgba(255, 255, 255, 0.35) 50%,
        rgba(255, 255, 255, 0.2) 100%
    );
}

.progress-bar.seeking {
    cursor: wait;
    opacity: 0.7;
}

.progress-bar-fill {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    background: linear-gradient(90deg, #059669 0%, #10b981 50%, #34d399 100%);
    border-radius: 3px;
    transition: width 0.1s linear;
    z-index: 1;
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.5);
    overflow: visible;
}

.progress-bar.dragging .progress-bar-fill {
    transition: width 0s;
    box-shadow: 0 0 12px rgba(16, 185, 129, 0.8);
}

.progress-bar-thumb {
    position: absolute;
    right: -10px;
    top: 50%;
    transform: translateY(-50%);
    width: 20px;
    height: 20px;
    background: white;
    border: 3px solid rgba(0, 0, 0, 0.8);
    border-radius: 50%;
    box-shadow:
        0 2px 8px rgba(0, 0, 0, 0.6),
        inset 0 1px 2px rgba(0, 0, 0, 0.1);
    transition: all 0.2s ease;
    z-index: 2;
    cursor: grab;
}

.progress-bar:hover .progress-bar-thumb {
    transform: translateY(-50%) scale(1.2);
    box-shadow:
        0 3px 12px rgba(0, 0, 0, 0.7),
        inset 0 1px 2px rgba(0, 0, 0, 0.1);
}

.progress-bar.dragging .progress-bar-thumb {
    transform: translateY(-50%) scale(1.3);
    cursor: grabbing;
    box-shadow:
        0 4px 16px rgba(0, 0, 0, 0.8),
        inset 0 1px 2px rgba(0, 0, 0, 0.1);
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

/* Mode kiosque - compactage pour petites hauteurs (800x600) */
@media (max-height: 700px) and (orientation: landscape) {
    .current-track {
        gap: var(--spacing-sm);
    }

    .cover-container {
        max-width: 160px !important;
        max-height: 160px;
    }

    .cover-placeholder {
        font-size: 40px;
    }

    .metadata {
        margin-top: -8px;
    }

    .title {
        font-size: var(--text-lg);
        margin: 0 0 4px;
    }

    .artist {
        font-size: var(--text-sm);
        margin: 0 0 2px;
    }

    .album {
        font-size: var(--text-xs);
    }

    .progress-section {
        gap: 4px;
    }

    .time-display {
        font-size: 11px;
    }
}

/* Overlay cover en grand - Liquid metal style */
.cover-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(20px) saturate(150%);
    -webkit-backdrop-filter: blur(20px) saturate(150%);
    padding: 16px;
    /* Empêcher le pull-to-refresh Android de se déclencher */
    overscroll-behavior: contain;
    touch-action: none;
}

.cover-overlay-content {
    position: relative;
    width: calc(100vw - 32px);
    height: calc(100dvh - 32px);
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.1);
    backdrop-filter: blur(40px) saturate(180%);
    -webkit-backdrop-filter: blur(40px) saturate(180%);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 24px;
    box-shadow:
        0 20px 60px rgba(0, 0, 0, 0.5),
        inset 0 1px 0 rgba(255, 255, 255, 0.2),
        inset 0 -1px 0 rgba(0, 0, 0, 0.3);
}

@media (prefers-color-scheme: dark) {
    .cover-overlay-content {
        background: rgba(0, 0, 0, 0.3);
        border-color: rgba(255, 255, 255, 0.15);
    }

    .cover-overlay-metadata {
        background: rgba(0, 0, 0, 0.4);
    }
}

.cover-overlay-close {
    position: absolute;
    top: var(--spacing-md);
    right: var(--spacing-md);
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.2);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.3);
    border-radius: 50%;
    cursor: pointer;
    color: var(--color-text);
    transition: all 0.3s ease;
    z-index: 10;
}

.cover-overlay-close:hover {
    background: rgba(255, 255, 255, 0.3);
    transform: scale(1.1) rotate(90deg);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.cover-overlay-close:active {
    transform: scale(1) rotate(90deg);
}

@media (prefers-color-scheme: dark) {
    .cover-overlay-close {
        background: rgba(0, 0, 0, 0.3);
        border-color: rgba(255, 255, 255, 0.2);
    }

    .cover-overlay-close:hover {
        background: rgba(0, 0, 0, 0.5);
    }
}

.cover-overlay-image {
    width: calc(100vw - 64px);
    height: calc(100dvh - 64px);
    border-radius: 16px;
    box-shadow:
        0 30px 80px rgba(0, 0, 0, 0.6),
        0 0 2px rgba(255, 255, 255, 0.2);
    object-fit: contain;
}

.cover-overlay-metadata {
    position: absolute;
    bottom: 24px;
    left: 15%;
    right: 15%;
    text-align: center;
    padding: var(--spacing-lg);
    background: rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(30px) saturate(180%);
    -webkit-backdrop-filter: blur(30px) saturate(180%);
    border-radius: 20px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.4);
}

.cover-overlay-metadata h2 {
    font-size: clamp(1.5rem, 4vw, 2.5rem);
    font-weight: 700;
    color: white;
    margin: 0;
    text-shadow: 0 2px 12px rgba(0, 0, 0, 0.8);
    line-height: 1.2;
}

.cover-overlay-metadata .artist {
    font-size: clamp(1rem, 2.5vw, 1.5rem);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    margin: 8px 0 0 0;
    text-shadow: 0 1px 8px rgba(0, 0, 0, 0.6);
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.2);
}

.cover-overlay-metadata .album {
    font-size: clamp(0.875rem, 2vw, 1.125rem);
    font-weight: 400;
    font-style: italic;
    color: rgba(255, 255, 255, 0.75);
    margin: 6px 0 0 0;
    text-shadow: 0 1px 6px rgba(0, 0, 0, 0.6);
}

/* Animations de transition */
.cover-overlay-enter-active,
.cover-overlay-leave-active {
    transition: all 0.3s ease;
}

.cover-overlay-enter-active .cover-overlay-content,
.cover-overlay-leave-active .cover-overlay-content {
    transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.cover-overlay-enter-from,
.cover-overlay-leave-to {
    opacity: 0;
    backdrop-filter: blur(0px);
    -webkit-backdrop-filter: blur(0px);
}

.cover-overlay-enter-from .cover-overlay-content,
.cover-overlay-leave-to .cover-overlay-content {
    opacity: 0;
    transform: scale(0.9);
}

.cover-overlay-enter-to,
.cover-overlay-leave-from {
    opacity: 1;
}

.cover-overlay-enter-to .cover-overlay-content,
.cover-overlay-leave-from .cover-overlay-content {
    opacity: 1;
    transform: scale(1);
}

/* Responsive overlay */
@media (max-width: 768px) {
    .cover-overlay {
        padding: 12px;
    }

    .cover-overlay-content {
        width: calc(100vw - 24px);
        height: calc(100dvh - 24px);
        border-radius: 16px;
    }

    .cover-overlay-image {
        width: calc(100vw - 48px);
        height: calc(100dvh - 48px);
    }

    .cover-overlay-metadata {
        bottom: 16px;
        left: 16px;
        right: 16px;
        padding: var(--spacing-md);
    }

    .cover-overlay-close {
        width: 40px;
        height: 40px;
        top: 12px;
        right: 12px;
    }
}

/* Mode kiosque - overlay adapté */
@media (max-height: 700px) and (orientation: landscape) {
    .cover-overlay {
        padding: 12px;
    }

    .cover-overlay-content {
        width: calc(100vw - 24px);
        height: calc(100dvh - 24px);
    }

    .cover-overlay-image {
        width: calc(100vw - 48px);
        height: calc(100dvh - 48px);
    }

    .cover-overlay-metadata {
        bottom: 12px;
        left: 12px;
        right: 12px;
        padding: var(--spacing-sm);
    }

    .cover-overlay-metadata .artist {
        padding-top: 4px;
    }

    .cover-overlay-metadata .album {
        display: none; /* Masquer l'album en mode kiosque pour plus d'espace */
    }
}

/* Transition pour les métadonnées */
.metadata-fade-enter-active,
.metadata-fade-leave-active {
    transition: all 0.3s ease;
}

.metadata-fade-enter-from,
.metadata-fade-leave-to {
    opacity: 0;
    transform: translateY(20px);
}

.metadata-fade-enter-to,
.metadata-fade-leave-from {
    opacity: 1;
    transform: translateY(0);
}

/* Swipe down pour fermer */
.cover-overlay-content {
    transition:
        transform 0.1s ease-out,
        opacity 0.1s ease-out;
}

.cover-overlay-content:not(.swiping) {
    transition:
        transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1),
        opacity 0.3s ease;
}

/* Progress bar dans l'overlay - style glassmorphism macOS */
.overlay-metadata-text {
    margin-bottom: var(--spacing-md);
}

.overlay-progress-section {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
    padding-top: var(--spacing-md);
    border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.overlay-progress-bar {
    position: relative;
    width: 100%;
    height: 8px;
    background: rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.2s ease;
    box-shadow:
        inset 0 1px 3px rgba(0, 0, 0, 0.4),
        0 1px 0 rgba(255, 255, 255, 0.1);
    overflow: visible;
}

.overlay-progress-bar:hover {
    background: rgba(255, 255, 255, 0.2);
}

.overlay-progress-bar.dragging {
    cursor: grabbing;
    background: rgba(255, 255, 255, 0.25);
}

.overlay-progress-bar.seeking {
    cursor: wait;
    opacity: 0.7;
}

.overlay-progress-bar-fill {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    background: linear-gradient(90deg, #059669 0%, #10b981 50%, #34d399 100%);
    border-radius: 4px;
    transition: width 0.1s linear;
    z-index: 1;
    box-shadow: 0 0 12px rgba(16, 185, 129, 0.6);
    overflow: visible;
}

.overlay-progress-bar.dragging .overlay-progress-bar-fill {
    transition: width 0s;
    box-shadow: 0 0 16px rgba(16, 185, 129, 0.9);
}

.overlay-progress-bar-thumb {
    position: absolute;
    right: -12px;
    top: 50%;
    transform: translateY(-50%);
    width: 24px;
    height: 24px;
    background: rgba(255, 255, 255, 0.95);
    border: 2px solid rgba(16, 185, 129, 0.8);
    border-radius: 50%;
    box-shadow:
        0 2px 8px rgba(0, 0, 0, 0.5),
        0 0 12px rgba(16, 185, 129, 0.4),
        inset 0 1px 2px rgba(255, 255, 255, 0.5);
    transition: all 0.2s ease;
    z-index: 2;
    cursor: grab;
}

.overlay-progress-bar:hover .overlay-progress-bar-thumb {
    transform: translateY(-50%) scale(1.15);
    box-shadow:
        0 3px 12px rgba(0, 0, 0, 0.6),
        0 0 16px rgba(16, 185, 129, 0.5),
        inset 0 1px 2px rgba(255, 255, 255, 0.5);
}

.overlay-progress-bar.dragging .overlay-progress-bar-thumb {
    transform: translateY(-50%) scale(1.25);
    cursor: grabbing;
    box-shadow:
        0 4px 16px rgba(0, 0, 0, 0.7),
        0 0 20px rgba(16, 185, 129, 0.6),
        inset 0 1px 2px rgba(255, 255, 255, 0.5);
}

.overlay-time-display {
    display: flex;
    justify-content: space-between;
    font-size: var(--text-sm);
    color: rgba(255, 255, 255, 0.8);
    font-variant-numeric: tabular-nums;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
}

/* Responsive progress bar overlay */
@media (max-width: 768px) {
    .overlay-progress-bar {
        height: 10px;
    }

    .overlay-progress-bar-thumb {
        width: 28px;
        height: 28px;
        right: -14px;
    }

    .overlay-time-display {
        font-size: var(--text-base);
    }
}

/* Mode kiosque - progress bar overlay */
@media (max-height: 700px) and (orientation: landscape) {
    .overlay-progress-section {
        padding-top: var(--spacing-xs);
    }

    .overlay-progress-bar {
        height: 6px;
    }

    .overlay-progress-bar-thumb {
        width: 20px;
        height: 20px;
        right: -10px;
    }

    .overlay-time-display {
        font-size: 11px;
    }
}
</style>
