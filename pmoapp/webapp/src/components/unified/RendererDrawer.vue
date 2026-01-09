<script setup lang="ts">
import { computed, watch, ref, onUnmounted } from "vue";
import {
    X,
    Music2,
    Circle,
    Settings,
    Play,
    Pause,
    MoreVertical,
    ArrowRightLeft,
} from "lucide-vue-next";
import { useRenderers } from "@/composables/useRenderers";
import { useRouter } from "vue-router";
import StatusBadge from "@/components/pmocontrol/StatusBadge.vue";
import type { RendererSummary } from "@/services/pmocontrol/types";
import { api } from "@/services/pmocontrol/api";

const props = defineProps<{
    modelValue: boolean; // v-model pour contrôler l'ouverture
    selectedRendererId?: string | null; // ID du renderer actuellement sélectionné
}>();

const emit = defineEmits<{
    "update:modelValue": [value: boolean];
    "select-renderer": [rendererId: string];
}>();

const { allRenderers, fetchRenderers, getStateById } = useRenderers();
const router = useRouter();

// Gestion du menu déroulant
const openMenuId = ref<string | null>(null);

// Gestionnaire pour fermer le menu quand on clique en dehors
let clickOutsideHandler: ((event: MouseEvent) => void) | null = null;

watch(openMenuId, (newValue) => {
    if (newValue) {
        // Ajouter l'écouteur après un petit délai pour éviter la fermeture immédiate
        setTimeout(() => {
            clickOutsideHandler = (event: MouseEvent) => {
                const target = event.target as HTMLElement;
                if (!target.closest(".action-menu-container")) {
                    closeMenu();
                }
            };
            document.addEventListener("click", clickOutsideHandler);
        }, 100);
    } else {
        // Retirer l'écouteur
        if (clickOutsideHandler) {
            document.removeEventListener("click", clickOutsideHandler);
            clickOutsideHandler = null;
        }
    }
});

onUnmounted(() => {
    if (clickOutsideHandler) {
        document.removeEventListener("click", clickOutsideHandler);
    }
});

// Fonction pour obtenir l'état d'un renderer
function getRendererState(rendererId: string) {
    return getStateById(rendererId);
}

// Fonction pour obtenir le label du protocole
function getProtocolLabel(protocol: string): string {
    switch (protocol) {
        case "upnp":
            return "UPnP";
        case "openhome":
            return "OpenHome";
        case "hybrid":
            return "Hybrid";
        case "chromecast":
            return "Chromecast";
        default:
            return protocol;
    }
}

// Fonction pour obtenir la classe CSS du protocole
function getProtocolClass(protocol: string): string {
    switch (protocol) {
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
}

// Rafraîchir la liste quand le drawer s'ouvre
watch(
    () => props.modelValue,
    (isOpen) => {
        if (isOpen) {
            fetchRenderers();
        }
    },
);

const onlineRenderers = computed(() =>
    allRenderers.value.filter((r: RendererSummary) => r.online),
);
const offlineRenderers = computed(() =>
    allRenderers.value.filter((r: RendererSummary) => !r.online),
);

function close() {
    emit("update:modelValue", false);
}

function handleRendererClick(renderer: RendererSummary) {
    if (!renderer.online) return;

    // Sélectionner le renderer et fermer le drawer
    emit("select-renderer", renderer.id);
    close();
}

function handleSettingsClick() {
    router.push("/debug/api-dashboard");
    close();
}

// Handlers pour les contrôles de transport
async function handlePlay(event: Event, rendererId: string) {
    event.stopPropagation();
    try {
        await api.play(rendererId);
    } catch (error) {
        console.error("[RendererDrawer] Error playing:", error);
    }
}

async function handlePause(event: Event, rendererId: string) {
    event.stopPropagation();
    try {
        await api.pause(rendererId);
    } catch (error) {
        console.error("[RendererDrawer] Error pausing:", error);
    }
}

// Gestion du menu déroulant
function toggleMenu(event: Event, rendererId: string) {
    event.stopPropagation();
    openMenuId.value = openMenuId.value === rendererId ? null : rendererId;
}

function closeMenu() {
    openMenuId.value = null;
}

// Transfert de la queue
async function handleTransferQueue(event: Event, targetRendererId: string) {
    event.stopPropagation();
    closeMenu();

    if (!props.selectedRendererId) {
        console.warn("[RendererDrawer] No source renderer selected");
        return;
    }

    if (props.selectedRendererId === targetRendererId) {
        console.warn("[RendererDrawer] Cannot transfer to the same renderer");
        return;
    }

    try {
        console.log(
            `[RendererDrawer] Transferring queue from ${props.selectedRendererId} to ${targetRendererId}`,
        );
        await api.transferQueue(props.selectedRendererId, targetRendererId);

        // Sélectionner le nouveau renderer et fermer le drawer
        emit("select-renderer", targetRendererId);
        close();
    } catch (error) {
        console.error("[RendererDrawer] Error transferring queue:", error);
    }
}
</script>

<template>
    <div>
        <!-- Backdrop -->
        <Transition name="backdrop">
            <div v-if="modelValue" class="drawer-backdrop" @click="close"></div>
        </Transition>

        <!-- Drawer -->
        <Transition name="drawer">
            <aside v-if="modelValue" class="renderer-drawer">
                <!-- Header -->
                <header class="drawer-header">
                    <div class="drawer-title-section">
                        <Music2 :size="24" />
                        <h2 class="drawer-title">Renderers</h2>
                    </div>
                    <button
                        class="drawer-close-btn"
                        @click="close"
                        aria-label="Fermer"
                    >
                        <X :size="24" />
                    </button>
                </header>

                <!-- Contenu -->
                <div class="drawer-content">
                    <!-- Renderers online -->
                    <section
                        v-if="onlineRenderers.length > 0"
                        class="renderer-section"
                    >
                        <h3 class="section-title">
                            Disponibles ({{ onlineRenderers.length }})
                        </h3>
                        <ul class="renderer-list">
                            <li
                                v-for="renderer in onlineRenderers"
                                :key="renderer.id"
                                class="renderer-item online"
                                :class="{
                                    selected:
                                        renderer.id === selectedRendererId,
                                }"
                                @click="handleRendererClick(renderer)"
                            >
                                <!-- Bouton transport à gauche -->
                                <button
                                    v-if="getRendererState(renderer.id)"
                                    class="transport-btn"
                                    @click="
                                        getRendererState(renderer.id)
                                            ?.transport_state === 'PLAYING'
                                            ? handlePause($event, renderer.id)
                                            : getRendererState(renderer.id)
                                                    ?.transport_state ===
                                                'PAUSED'
                                              ? handlePlay($event, renderer.id)
                                              : handlePlay($event, renderer.id)
                                    "
                                    :title="
                                        getRendererState(renderer.id)
                                            ?.transport_state === 'PLAYING'
                                            ? 'Pause'
                                            : 'Play'
                                    "
                                >
                                    <Pause
                                        v-if="
                                            getRendererState(renderer.id)
                                                ?.transport_state === 'PLAYING'
                                        "
                                        :size="18"
                                    />
                                    <Play v-else :size="18" />
                                </button>
                                <button
                                    v-else
                                    class="transport-btn disabled"
                                    disabled
                                >
                                    <Play :size="18" />
                                </button>

                                <div class="renderer-icon">
                                    <Music2 :size="20" />
                                </div>
                                <div class="renderer-info">
                                    <div class="renderer-name-row">
                                        <p class="renderer-name">
                                            {{ renderer.friendly_name }}
                                        </p>
                                        <span
                                            :class="[
                                                'protocol-badge',
                                                getProtocolClass(
                                                    renderer.protocol,
                                                ),
                                            ]"
                                        >
                                            {{
                                                getProtocolLabel(
                                                    renderer.protocol,
                                                )
                                            }}
                                        </span>
                                    </div>
                                    <div class="renderer-details">
                                        <p
                                            v-if="renderer.model_name"
                                            class="renderer-model"
                                        >
                                            {{ renderer.model_name }}
                                        </p>
                                        <StatusBadge
                                            v-if="getRendererState(renderer.id)"
                                            :status="
                                                getRendererState(renderer.id)!
                                                    .transport_state
                                            "
                                            class="renderer-state-badge"
                                        />
                                    </div>
                                </div>

                                <!-- Menu actions (uniquement si ce n'est pas le renderer sélectionné) -->
                                <div
                                    v-if="
                                        selectedRendererId &&
                                        renderer.id !== selectedRendererId
                                    "
                                    class="action-menu-container"
                                >
                                    <button
                                        class="menu-btn"
                                        @click="toggleMenu($event, renderer.id)"
                                        :aria-label="`Actions pour ${renderer.friendly_name}`"
                                    >
                                        <MoreVertical :size="18" />
                                    </button>

                                    <Transition name="menu-fade">
                                        <div
                                            v-if="openMenuId === renderer.id"
                                            class="action-dropdown"
                                        >
                                            <button
                                                class="dropdown-item"
                                                @click="
                                                    handleTransferQueue(
                                                        $event,
                                                        renderer.id,
                                                    )
                                                "
                                            >
                                                <ArrowRightLeft :size="16" />
                                                <span
                                                    >Transférer la lecture
                                                    ici</span
                                                >
                                            </button>
                                        </div>
                                    </Transition>
                                </div>

                                <div class="renderer-status">
                                    <Circle :size="8" fill="currentColor" />
                                </div>
                            </li>
                        </ul>
                    </section>

                    <!-- Renderers offline -->
                    <section
                        v-if="offlineRenderers.length > 0"
                        class="renderer-section"
                    >
                        <h3 class="section-title">
                            Hors ligne ({{ offlineRenderers.length }})
                        </h3>
                        <ul class="renderer-list">
                            <li
                                v-for="renderer in offlineRenderers"
                                :key="renderer.id"
                                class="renderer-item offline"
                            >
                                <div class="renderer-icon">
                                    <Music2 :size="20" />
                                </div>
                                <div class="renderer-info">
                                    <p class="renderer-name">
                                        {{ renderer.friendly_name }}
                                    </p>
                                    <p
                                        v-if="renderer.model_name"
                                        class="renderer-model"
                                    >
                                        {{ renderer.model_name }}
                                    </p>
                                </div>
                                <div class="renderer-status">
                                    <Circle :size="8" fill="currentColor" />
                                </div>
                            </li>
                        </ul>
                    </section>

                    <!-- Aucun renderer -->
                    <div
                        v-if="allRenderers.length === 0"
                        class="empty-renderers"
                    >
                        <Music2 :size="48" />
                        <p>Aucun renderer détecté</p>
                    </div>
                </div>

                <!-- Footer avec bouton settings -->
                <footer class="drawer-footer">
                    <button
                        class="settings-btn"
                        @click="handleSettingsClick"
                        title="Ouvrir le menu Debug"
                    >
                        <Settings :size="20" />
                        <span>Debug & Config</span>
                    </button>
                </footer>
            </aside>
        </Transition>
    </div>
</template>

<style scoped>
.drawer-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 50vw; /* Commence avant le drawer (desktop: jusqu'à 50vw) */
    bottom: 0;
    background: rgba(0, 0, 0, 0.35); /* Moins sombre */
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    z-index: 200;
}

@media (max-width: 768px) and (orientation: portrait) {
    .drawer-backdrop {
        right: 0; /* Mobile portrait: backdrop prend tout l'écran */
        background: rgba(0, 0, 0, 0.4); /* Plus sombre sur mobile */
    }
}

.renderer-drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: 50vw; /* Desktop/landscape: 50% de l'écran */
    background: rgba(255, 255, 255, 0.08); /* Plus transparent */
    backdrop-filter: blur(40px) saturate(180%);
    -webkit-backdrop-filter: blur(40px) saturate(180%);
    border-left: 1px solid rgba(255, 255, 255, 0.15);
    box-shadow: -4px 0 32px rgba(0, 0, 0, 0.25);
    z-index: 201;
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

@media (prefers-color-scheme: dark) {
    .renderer-drawer {
        background: rgba(0, 0, 0, 0.4);
        border-left-color: rgba(255, 255, 255, 0.1);
    }
}

/* Header */
.drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-sm);
    padding: var(--spacing-lg);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    flex-shrink: 0;
}

.drawer-title-section {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    color: var(--color-text);
    flex: 1;
    min-width: 0;
}

.drawer-title {
    font-size: var(--text-xl);
    font-weight: 700;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.drawer-close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    padding: 0;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    cursor: pointer;
    transition: all 0.2s ease;
    color: var(--color-text);
}

.drawer-close-btn:hover {
    background: rgba(255, 255, 255, 0.2);
    transform: scale(1.1);
}

.drawer-close-btn:active {
    transform: scale(0.95);
}

/* Content */
.drawer-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-md);
}

.renderer-section {
    margin-bottom: var(--spacing-lg);
}

.section-title {
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-text-secondary);
    margin: 0 0 var(--spacing-sm) 0;
    padding: 0 var(--spacing-sm);
}

.renderer-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.renderer-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-md);
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    transition: all 0.2s ease;
}

.renderer-item.online {
    cursor: pointer;
}

.renderer-item.online:hover {
    background: rgba(255, 255, 255, 0.15);
    border-color: rgba(255, 255, 255, 0.2);
    transform: translateX(-4px);
}

.renderer-item.online:active {
    transform: translateX(-2px);
}

.renderer-item.selected {
    background: rgba(102, 126, 234, 0.2);
    border-color: var(--color-primary);
    box-shadow: 0 0 16px rgba(102, 126, 234, 0.3);
}

.renderer-item.selected:hover {
    background: rgba(102, 126, 234, 0.25);
}

.renderer-item.offline {
    opacity: 0.5;
    cursor: not-allowed;
}

.renderer-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-secondary);
}

.renderer-item.selected .renderer-icon {
    background: rgba(102, 126, 234, 0.3);
    color: var(--color-primary);
}

.renderer-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.renderer-name-row {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    flex-wrap: wrap;
}

.renderer-name {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.renderer-details {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    flex-wrap: wrap;
}

.renderer-model {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.renderer-state-badge {
    flex-shrink: 0;
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

/* Transport button */
.transport-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    padding: 0;
    background: rgba(102, 126, 234, 0.2);
    border: 1px solid var(--color-primary);
    border-radius: 50%;
    cursor: pointer;
    transition: all 0.2s ease;
    color: var(--color-primary);
}

.transport-btn:hover:not(.disabled) {
    background: var(--color-primary);
    color: white;
    transform: scale(1.1);
}

.transport-btn:active:not(.disabled) {
    transform: scale(0.95);
}

.transport-btn.disabled {
    opacity: 0.3;
    cursor: not-allowed;
    background: rgba(255, 255, 255, 0.05);
    border-color: rgba(255, 255, 255, 0.1);
    color: var(--color-text-tertiary);
}

/* Menu actions */
.action-menu-container {
    position: relative;
    flex-shrink: 0;
}

.menu-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    color: var(--color-text-secondary);
    transition: all 0.2s ease;
}

.menu-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text);
}

.action-dropdown {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    min-width: 200px;
    background: var(--color-surface-elevated);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    overflow: hidden;
    z-index: 1000;
}

@media (prefers-color-scheme: light) {
    .action-dropdown {
        background: white;
        border-color: rgba(0, 0, 0, 0.1);
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    }
}

.dropdown-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    width: 100%;
    padding: var(--spacing-sm) var(--spacing-md);
    background: none;
    border: none;
    text-align: left;
    cursor: pointer;
    transition: background-color 0.2s ease;
    color: var(--color-text);
    font-size: var(--text-sm);
}

.dropdown-item:hover {
    background: rgba(102, 126, 234, 0.1);
}

.dropdown-item:active {
    background: rgba(102, 126, 234, 0.2);
}

.dropdown-item span {
    flex: 1;
}

/* Transitions pour le menu */
.menu-fade-enter-active {
    transition: all 0.15s ease-out;
}

.menu-fade-leave-active {
    transition: all 0.1s ease-in;
}

.menu-fade-enter-from {
    opacity: 0;
    transform: translateY(-4px);
}

.menu-fade-leave-to {
    opacity: 0;
    transform: translateY(-4px);
}

.renderer-status {
    flex-shrink: 0;
    color: var(--status-playing);
}

.renderer-item.offline .renderer-status {
    color: var(--status-offline);
}

.empty-renderers {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-md);
    padding: var(--spacing-2xl);
    text-align: center;
    color: var(--color-text-secondary);
}

.empty-renderers p {
    margin: 0;
    font-size: var(--text-base);
}

/* Footer */
.drawer-footer {
    flex-shrink: 0;
    padding: var(--spacing-md);
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(0, 0, 0, 0.1);
}

.settings-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    width: 100%;
    padding: var(--spacing-md);
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
}

.settings-btn:hover {
    background: rgba(255, 255, 255, 0.2);
    color: var(--color-text);
    transform: translateY(-1px);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}

.settings-btn:active {
    transform: translateY(0);
}

/* Animations */
.backdrop-enter-active {
    transition: opacity 0.3s ease-out;
    transition-delay: 0.1s; /* Attend que le drawer soit un peu visible */
}

.backdrop-leave-active {
    transition: opacity 0.25s ease-in;
}

.backdrop-enter-from,
.backdrop-leave-to {
    opacity: 0;
}

.drawer-enter-active {
    transition: all 0.4s cubic-bezier(0.16, 1, 0.3, 1); /* Courbe d'animation fluide (easeOutExpo) */
}

.drawer-leave-active {
    transition: all 0.3s cubic-bezier(0.7, 0, 0.84, 0); /* Courbe d'animation de sortie (easeInExpo) */
}

.drawer-enter-from {
    transform: translateX(100%);
    opacity: 0;
}

.drawer-leave-to {
    transform: translateX(100%);
    opacity: 0;
}

/* Scrollbar styling */
.drawer-content::-webkit-scrollbar {
    width: 6px;
}

.drawer-content::-webkit-scrollbar-track {
    background: rgba(255, 255, 255, 0.05);
    border-radius: 3px;
}

.drawer-content::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.2);
    border-radius: 3px;
}

.drawer-content::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.3);
}

/* Mobile responsive - portrait */
@media (max-width: 768px) and (orientation: portrait) {
    .renderer-drawer {
        width: 100vw; /* Mobile portrait: 100% de l'écran */
        background: rgba(
            255,
            255,
            255,
            0.06
        ); /* Encore plus transparent sur mobile */
        box-shadow: none; /* Pas d'ombre sur les côtés */
    }

    .drawer-header {
        padding: var(--spacing-md);
    }

    .drawer-title {
        font-size: var(--text-lg);
    }

    .renderer-item {
        padding: var(--spacing-md);
    }

    .renderer-icon {
        width: 48px;
        height: 48px;
    }
}

/* Fallback pour navigateurs sans backdrop-filter */
@supports not (backdrop-filter: blur(30px)) {
    .renderer-drawer {
        background: rgba(255, 255, 255, 0.98);
    }

    @media (prefers-color-scheme: dark) {
        .renderer-drawer {
            background: rgba(20, 20, 30, 0.98);
        }
    }
}
</style>
