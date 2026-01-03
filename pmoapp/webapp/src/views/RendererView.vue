<script setup lang="ts">
import { computed, onMounted, onUnmounted, toRef } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useRenderer, useRenderers } from "@/composables/useRenderers";
import { useUIStore } from "@/stores/ui";
import CurrentTrack from "@/components/pmocontrol/CurrentTrack.vue";
import TransportControls from "@/components/pmocontrol/TransportControls.vue";
import VolumeControl from "@/components/pmocontrol/VolumeControl.vue";
import QueueViewer from "@/components/pmocontrol/QueueViewer.vue";
import PlaylistBindingPanel from "@/components/pmocontrol/PlaylistBindingPanel.vue";
import StatusBadge from "@/components/pmocontrol/StatusBadge.vue";
import { ArrowLeft, Radio } from "lucide-vue-next";

const route = useRoute();
const router = useRouter();
const uiStore = useUIStore();

const rendererId = computed(() => route.params.id as string);
const { renderer, state, refresh } = useRenderer(toRef(() => rendererId.value));
const { fetchRenderers, selectRenderer: selectRendererSnapshot } =
    useRenderers();

// Charger les données au montage si nécessaire
onMounted(async () => {
    uiStore.selectRenderer(rendererId.value);
    selectRendererSnapshot(rendererId.value);

    // Charger toutes les données du renderer
    if (!renderer.value) {
        await fetchRenderers();
    }
    await refresh();
});

// Nettoyer la sélection au démontage
onUnmounted(() => {
    uiStore.selectRenderer(null);
    selectRendererSnapshot(null);
});

function goBack() {
    router.push("/");
}

const protocolLabel = computed(() => {
    if (!renderer.value) return "";
    switch (renderer.value.protocol) {
        case "upnp":
            return "UPnP AV";
        case "openhome":
            return "OpenHome";
        case "hybrid":
            return "Hybrid (UPnP + OpenHome)";
        case "chromecast":
            return "Chromecast";
        default:
            return "Inconnu";
    }
});
</script>

<template>
    <div class="renderer-view">
        <!-- Header -->
        <header class="renderer-header">
            <button
                class="btn-back"
                @click="goBack"
                title="Retour au dashboard"
            >
                <ArrowLeft :size="20" />
            </button>
            <div class="header-content">
                <div class="renderer-info">
                    <Radio :size="24" class="renderer-icon" />
                    <div class="renderer-details">
                        <h1 class="renderer-name">
                            {{ renderer?.friendly_name || "Chargement..." }}
                        </h1>
                        <p class="renderer-model">
                            {{ renderer?.model_name }} • {{ protocolLabel }}
                        </p>
                    </div>
                </div>
                <StatusBadge v-if="state" :status="state.transport_state" />
            </div>
        </header>

        <!-- Loading state -->
        <div v-if="!renderer || !state" class="loading-state">
            <p>Chargement du renderer...</p>
        </div>

        <!-- Main content -->
        <div v-else class="renderer-content">
            <!-- Left column (Desktop) / Top (Mobile) -->
            <div class="left-column">
                <!-- Current Track -->
                <section class="content-section">
                    <CurrentTrack :rendererId="rendererId" />
                </section>

                <!-- Transport Controls -->
                <section class="content-section">
                    <TransportControls :rendererId="rendererId" />
                </section>

                <!-- Volume Control -->
                <section class="content-section">
                    <h3 class="section-subtitle">Volume</h3>
                    <VolumeControl :rendererId="rendererId" />
                </section>

                <!-- Playlist Binding -->
                <section class="content-section">
                    <PlaylistBindingPanel :rendererId="rendererId" />
                </section>
            </div>

            <!-- Right column (Desktop) / Bottom (Mobile) -->
            <div class="right-column">
                <section class="content-section queue-section">
                    <QueueViewer :rendererId="rendererId" />
                </section>
            </div>
        </div>
    </div>
</template>

<style scoped>
.renderer-view {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-lg);
    padding: var(--spacing-lg);
    max-width: 1400px;
    margin: 0 auto;
    width: 100%;
    height: 100%;
}

/* Header */
.renderer-header {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-md);
}

.btn-back {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    background: none;
    border: none;
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all var(--transition-fast);
    flex-shrink: 0;
}

.btn-back:hover {
    background-color: var(--color-bg-secondary);
    color: var(--color-text);
}

.header-content {
    flex: 1;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--spacing-md);
    flex-wrap: wrap;
}

.renderer-info {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
}

.renderer-icon {
    color: var(--color-primary);
    flex-shrink: 0;
}

.renderer-details {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
}

.renderer-name {
    font-size: var(--text-2xl);
    font-weight: 700;
    color: var(--color-text);
    margin: 0;
}

.renderer-model {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 0;
}

/* Loading */
.loading-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--text-base);
    color: var(--color-text-secondary);
}

/* Content */
.renderer-content {
    flex: 1;
    display: grid;
    gap: var(--spacing-xl);
    grid-template-columns: 1fr;
    min-height: 0;
}

.left-column,
.right-column {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-lg);
    min-height: 0;
}

.content-section {
    background-color: var(--color-bg-secondary);
    border-radius: var(--radius-lg);
    padding: var(--spacing-lg);
    border: 1px solid var(--color-border);
}

.queue-section {
    flex: 1;
    min-height: 400px;
    display: flex;
    flex-direction: column;
}

.section-subtitle {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 var(--spacing-md);
}

/* Responsive - Desktop */
@media (min-width: 1024px) {
    .renderer-content {
        grid-template-columns: 400px 1fr;
    }

    .queue-section {
        min-height: 0;
    }
}

/* Responsive - Tablet */
@media (min-width: 768px) and (max-width: 1023px) {
    .renderer-content {
        grid-template-columns: 1fr;
    }

    .left-column {
        display: grid;
        grid-template-columns: repeat(2, 1fr);
        gap: var(--spacing-lg);
    }

    .queue-section {
        grid-column: 1 / -1;
    }
}

/* Responsive - Mobile */
@media (max-width: 767px) {
    .renderer-view {
        padding: var(--spacing-md);
    }

    .renderer-name {
        font-size: var(--text-xl);
    }

    .renderer-info {
        flex-wrap: wrap;
    }

    .queue-section {
        min-height: 300px;
    }
}
</style>
