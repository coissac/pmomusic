<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import type {
    RendererCapabilitiesSummary,
    RendererSummary,
    RendererState,
} from "@/services/pmocontrol/types";
import StatusBadge from "./StatusBadge.vue";
import { Music, Volume2, VolumeX } from "lucide-vue-next";
import { useCoverImage } from "@/composables/useCoverImage";

const props = defineProps<{
    renderer: RendererSummary;
    state: RendererState | null;
}>();

const router = useRouter();

// Métadonnées proviennent directement de l'état du renderer (API + SSE)
const metadata = computed(() => props.state?.current_track);

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

const protocolLabel = computed(() => {
    switch (props.renderer.protocol) {
        case "upnp":
            return "UPnP AV";
        case "openhome":
            return "OpenHome";
        case "hybrid":
            return "Hybrid (UPnP + OpenHome)";
        default:
            return "Inconnu";
    }
});

const protocolClass = computed(() => {
    switch (props.renderer.protocol) {
        case "upnp":
            return "protocol-upnp";
        case "openhome":
            return "protocol-openhome";
        case "hybrid":
            return "protocol-hybrid";
        default:
            return "protocol-unknown";
    }
});

const capabilityBadges = computed(() => {
    const caps = props.renderer.capabilities;
    if (!caps) return [];
    const mapping: Array<{
        key: keyof RendererCapabilitiesSummary;
        label: string;
    }> = [
        { key: "has_avtransport", label: "AVTransport" },
        { key: "has_oh_playlist", label: "OpenHome" },
        { key: "has_linkplay_http", label: "Hybrid" },
        { key: "has_oh_volume", label: "Vol" },
        { key: "has_oh_time", label: "Time" },
        { key: "has_oh_info", label: "Info" },
    ];
    return mapping
        .filter(({ key }) => caps[key])
        .map(({ key, label }) => ({ key, label }));
});

const hasCover = computed(
    () => !!metadata.value?.album_art_uri && !imageError.value,
);

function goToRenderer() {
    router.push(`/renderer/${props.renderer.id}`);
}
</script>

<template>
    <div :class="['renderer-card', { offline: !renderer.online }]">
        <!-- Header -->
        <div class="card-header">
            <div class="header-content">
                <h3 class="renderer-name">{{ renderer.friendly_name }}</h3>
                <p class="renderer-model">{{ renderer.model_name }}</p>
            </div>
            <div class="badges">
                <span :class="['protocol-badge', protocolClass]">
                    {{ protocolLabel }}
                </span>
                <StatusBadge v-if="state" :status="state.transport_state" />
            </div>
        </div>

        <!-- Cover Art -->
        <div v-if="capabilityBadges.length" class="capabilities">
            <span
                v-for="badge in capabilityBadges"
                :key="badge.key"
                class="capability-badge"
            >
                {{ badge.label }}
            </span>
        </div>

        <div class="card-cover">
            <img
                ref="coverImageRef"
                :style="{
                    opacity: hasCover && cacheBustedUrl && imageLoaded ? 1 : 0,
                    visibility:
                        hasCover && cacheBustedUrl && imageLoaded
                            ? 'visible'
                            : 'hidden',
                    position:
                        hasCover && cacheBustedUrl && imageLoaded
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
                v-show="!cacheBustedUrl || !imageLoaded"
                class="cover-placeholder"
            >
                <Music :size="48" />
            </div>
        </div>

        <!-- Metadata (current track) -->
        <div v-if="metadata" class="card-metadata">
            <p class="track-title">{{ metadata.title || "Sans titre" }}</p>
            <p class="track-artist">
                {{ metadata.artist || "Artiste inconnu" }}
            </p>
        </div>
        <div v-else class="card-metadata empty">
            <p class="track-title">Aucun média</p>
        </div>

        <!-- Volume -->
        <div v-if="state && state.volume !== null" class="card-volume">
            <VolumeX v-if="state.mute" :size="16" class="volume-icon muted" />
            <Volume2 v-else :size="16" class="volume-icon" />
            <div class="volume-bar">
                <div
                    class="volume-bar-fill"
                    :style="{ width: `${state.mute ? 0 : state.volume}%` }"
                ></div>
            </div>
            <span class="volume-value">{{ state.volume }}</span>
        </div>

        <!-- Control Button -->
        <button class="btn btn-primary card-control-btn" @click="goToRenderer">
            Contrôler
        </button>
    </div>
</template>

<style scoped>
.renderer-card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
    padding: var(--spacing-lg);
    background-color: var(--color-bg-secondary);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-border);
    transition: all var(--transition-normal);
}

.renderer-card:hover {
    border-color: var(--color-primary);
    box-shadow: var(--shadow-md);
}

.renderer-card.offline {
    opacity: 0.6;
    filter: grayscale(0.5);
}

/* Header */
.card-header {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
}

.header-content {
    flex: 1;
}

.renderer-name {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 var(--spacing-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.renderer-model {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.badges {
    display: flex;
    gap: var(--spacing-xs);
    flex-wrap: wrap;
}

.capabilities {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-xs);
}

.capability-badge {
    padding: 2px 8px;
    border-radius: var(--radius-full);
    font-size: var(--text-xs);
    background-color: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
}

.protocol-badge {
    padding: var(--spacing-xs) var(--spacing-sm);
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.protocol-upnp {
    background-color: rgba(59, 130, 246, 0.1);
    color: #3b82f6;
    border: 1px solid #3b82f6;
}

.protocol-openhome {
    background-color: rgba(139, 92, 246, 0.1);
    color: #8b5cf6;
    border: 1px solid #8b5cf6;
}

.protocol-hybrid {
    background-color: rgba(16, 185, 129, 0.1);
    color: #10b981;
    border: 1px solid #10b981;
}

/* Cover */
.card-cover {
    width: 100%;
    aspect-ratio: 1;
    border-radius: var(--radius-md);
    overflow: hidden;
    background-color: var(--color-bg-tertiary);
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

/* Metadata */
.card-metadata {
    min-height: 3rem;
}

.track-title {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 var(--spacing-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.track-artist {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.card-metadata.empty .track-title {
    color: var(--color-text-tertiary);
    font-weight: 400;
}

/* Volume */
.card-volume {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
}

.volume-icon {
    color: var(--color-text-secondary);
    flex-shrink: 0;
}

.volume-icon.muted {
    color: var(--status-offline);
}

.volume-bar {
    flex: 1;
    height: 4px;
    background-color: var(--color-bg-tertiary);
    border-radius: var(--radius-full);
    overflow: hidden;
}

.volume-bar-fill {
    height: 100%;
    background-color: var(--color-primary);
    transition: width var(--transition-fast);
}

.volume-value {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-text-secondary);
    min-width: 2rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
}

/* Control Button */
.card-control-btn {
    width: 100%;
}

/* Responsive grid handled by parent (DashboardView) */
</style>
