<script setup lang="ts">
import { computed } from "vue";
import { Server, Music2 } from "lucide-vue-next";
import StatusBadge from "@/components/pmocontrol/StatusBadge.vue";
import TimerControl from "@/components/pmocontrol/TimerControl.vue";
import ShuffleControl from "@/components/pmocontrol/ShuffleControl.vue";
import type {
    RendererSummary,
    RendererState,
} from "@/services/pmocontrol/types";

const props = defineProps<{
    onlineServersCount?: number;
    onlineRenderersCount?: number;
    activeRenderer?: RendererSummary | null;
    activeRendererState?: RendererState | null;
}>();

const emit = defineEmits<{
    "open-drawer": [];
    "open-renderer-drawer": [];
}>();

// Label du protocole
const protocolLabel = computed(() => {
    if (!props.activeRenderer) return "";
    switch (props.activeRenderer.protocol) {
        case "upnp":
            return "UPnP";
        case "openhome":
            return "OpenHome";
        case "hybrid":
            return "Hybrid";
        case "chromecast":
            return "Chromecast";
        default:
            return props.activeRenderer.protocol;
    }
});

// Classe CSS du protocole
const protocolClass = computed(() => {
    if (!props.activeRenderer) return "";
    switch (props.activeRenderer.protocol) {
        case "upnp":
            return "protocol-upnp";
        case "openhome":
            return "protocol-openhome";
        case "hybrid":
            return "protocol-hybrid";
        case "chromecast":
            return "protocol-chromecast";
        default:
            return "protocol-unknown";
    }
});

function handleServerDrawerClick() {
    emit("open-drawer");
}

function handleRendererDrawerClick() {
    emit("open-renderer-drawer");
}
</script>

<template>
    <div class="bottom-bar">
        <!-- Bouton pour ouvrir le drawer des servers (gauche) -->
        <button
            class="drawer-button server-drawer-button"
            @click="handleServerDrawerClick"
            :aria-label="`Open media servers (${onlineServersCount || 0} online)`"
            :title="`Media Servers (${onlineServersCount || 0} online)`"
        >
            <Server :size="24" />
            <span
                v-if="onlineServersCount && onlineServersCount > 0"
                class="badge server-badge"
                >{{ onlineServersCount }}</span
            >
        </button>

        <!-- Zone centrale avec les infos du renderer -->
        <div class="renderer-info-section">
            <div v-if="activeRenderer" class="renderer-info-content">
                <div class="renderer-name-row">
                    <h2 class="renderer-name">
                        {{ activeRenderer.friendly_name }}
                    </h2>
                    <span
                        v-if="activeRenderer.protocol"
                        :class="['protocol-badge', protocolClass]"
                    >
                        {{ protocolLabel }}
                    </span>
                </div>
                <div class="renderer-details-row">
                    <p v-if="activeRenderer.model_name" class="renderer-model">
                        {{ activeRenderer.model_name }}
                    </p>
                    <StatusBadge
                        v-if="activeRendererState"
                        :status="activeRendererState.transport_state"
                        class="status-badge"
                    />
                    <span v-if="!activeRenderer.online" class="offline-badge">
                        OFFLINE
                    </span>
                </div>
            </div>
            <div v-else class="renderer-info-content empty">
                <p class="no-renderer">Aucun renderer sélectionné</p>
            </div>
        </div>

        <!-- Shuffle et Sleep Timer (si un renderer est actif) -->
        <div v-if="activeRenderer" class="controls-section">
            <ShuffleControl :renderer-id="activeRenderer.id" />
            <TimerControl :renderer-id="activeRenderer.id" />
        </div>

        <!-- Bouton pour ouvrir le drawer des renderers (droite) -->
        <button
            class="drawer-button renderer-drawer-button"
            @click="handleRendererDrawerClick"
            :aria-label="`Open renderers (${onlineRenderersCount || 0} online)`"
            :title="`Renderers (${onlineRenderersCount || 0} online)`"
        >
            <Music2 :size="24" />
            <span
                v-if="onlineRenderersCount && onlineRenderersCount > 0"
                class="badge renderer-badge"
                >{{ onlineRenderersCount }}</span
            >
        </button>
    </div>
</template>

<style scoped>
.bottom-bar {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    height: 72px;
    padding: 0 var(--spacing-md);
    background: rgba(255, 255, 255, 0.1);
    backdrop-filter: blur(30px) saturate(180%);
    -webkit-backdrop-filter: blur(30px) saturate(180%);
    border-top: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: 0 -4px 24px rgba(0, 0, 0, 0.1);
    z-index: 100;
}

@media (prefers-color-scheme: dark) {
    .bottom-bar {
        background: rgba(0, 0, 0, 0.25);
        border-top: 1px solid rgba(255, 255, 255, 0.1);
    }
}

/* Boutons drawer */
.drawer-button {
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

.drawer-button:hover {
    background: rgba(255, 255, 255, 0.3);
    transform: scale(1.1);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.drawer-button:active {
    transform: scale(0.95);
}

@media (prefers-color-scheme: dark) {
    .drawer-button {
        background: rgba(255, 255, 255, 0.15);
    }

    .drawer-button:hover {
        background: rgba(255, 255, 255, 0.25);
    }
}

/* Badges */
.badge {
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
    border: 2px solid var(--color-bg);
    border-radius: 10px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}

.server-badge {
    background: rgba(102, 126, 234, 0.9);
}

.renderer-badge {
    background: rgba(234, 102, 126, 0.9);
}

/* Zone centrale */
.renderer-info-section {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: center;
}

.renderer-info-content {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: center;
    text-align: center;
    max-width: 100%;
}

.renderer-info-content.empty {
    color: var(--color-text-tertiary);
}

.renderer-name-row {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    flex-wrap: wrap;
    justify-content: center;
}

.renderer-name {
    font-size: var(--text-lg);
    font-weight: 700;
    color: var(--color-text);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 300px;
}

.renderer-details-row {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    flex-wrap: wrap;
    justify-content: center;
}

.renderer-model {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.no-renderer {
    font-size: var(--text-base);
    color: var(--color-text-tertiary);
    margin: 0;
}

/* Protocol badge */
.protocol-badge {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
}

.protocol-upnp {
    background-color: rgba(59, 130, 246, 0.15);
    color: #3b82f6;
    border: 1px solid rgba(59, 130, 246, 0.3);
}

.protocol-openhome {
    background-color: rgba(139, 92, 246, 0.15);
    color: #8b5cf6;
    border: 1px solid rgba(139, 92, 246, 0.3);
}

.protocol-hybrid {
    background-color: rgba(16, 185, 129, 0.15);
    color: #10b981;
    border: 1px solid rgba(16, 185, 129, 0.3);
}

.protocol-chromecast {
    background-color: rgba(244, 114, 182, 0.15);
    color: #f472b6;
    border: 1px solid rgba(244, 114, 182, 0.3);
}

.status-badge {
    flex-shrink: 0;
}

.offline-badge {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    background-color: rgba(239, 68, 68, 0.15);
    color: #ef4444;
    border: 1px solid rgba(239, 68, 68, 0.3);
}

/* Controls section (shuffle + timer) */
.controls-section {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    flex-shrink: 0;
}

/* Mobile responsive */
@media (max-width: 768px) {
    .bottom-bar {
        height: 64px;
        padding: 0 var(--spacing-sm);
        gap: var(--spacing-sm);
    }

    .drawer-button {
        width: 48px;
        height: 48px;
    }

    .renderer-name {
        font-size: var(--text-base);
        max-width: 200px;
    }

    .renderer-model {
        font-size: var(--text-xs);
    }
}

/* Animation d'entrée */
@keyframes slideInUp {
    from {
        transform: translateY(100%);
        opacity: 0;
    }
    to {
        transform: translateY(0);
        opacity: 1;
    }
}

.bottom-bar {
    animation: slideInUp 0.3s ease-out;
}
</style>
