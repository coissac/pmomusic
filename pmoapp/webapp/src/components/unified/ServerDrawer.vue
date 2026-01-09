<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useRouter } from "vue-router";
import {
    X,
    Server as ServerIcon,
    Circle,
    ChevronLeft,
    Folder,
    Music,
    ChevronRight,
    Play,
    Plus,
    Settings,
    MoreVertical,
} from "lucide-vue-next";
import { useMediaServers } from "@/composables/useMediaServers";
import { useRenderers } from "@/composables/useRenderers";
import type {
    MediaServerSummary,
    BrowseResponse,
    ContainerEntry,
} from "@/services/pmocontrol/types";

const props = defineProps<{
    modelValue: boolean; // v-model pour contrôler l'ouverture
    selectedRendererId?: string | null; // ID du renderer sélectionné
}>();

const emit = defineEmits<{
    "update:modelValue": [value: boolean];
}>();

const {
    allServers,
    fetchServers,
    browseContainer,
    currentPath,
    setPath,
    clearPath,
} = useMediaServers();

const { playContent, addToQueue, addAfterCurrent, attachAndPlayPlaylist } =
    useRenderers();

const router = useRouter();

// État de navigation
const currentServer = ref<MediaServerSummary | null>(null);
const browseData = ref<BrowseResponse | null>(null);
const isLoading = ref(false);

// État du menu dropdown (pour chaque item, on stocke si son menu est ouvert)
const openMenuId = ref<string | null>(null);

// Rafraîchir la liste quand le drawer s'ouvre
watch(
    () => props.modelValue,
    (isOpen) => {
        if (isOpen) {
            fetchServers();
        } else {
            // Reset navigation quand on ferme
            currentServer.value = null;
            browseData.value = null;
            clearPath();
            closeMenu();
        }
    },
);

// Fermer le menu quand on clique ailleurs (utilise un seul listener global)
let clickOutsideHandler: ((e: MouseEvent) => void) | null = null;

watch(
    () => openMenuId.value,
    (menuId) => {
        // Nettoyer l'ancien listener s'il existe
        if (clickOutsideHandler) {
            document.removeEventListener("click", clickOutsideHandler);
            clickOutsideHandler = null;
        }

        // Ajouter un nouveau listener seulement si un menu est ouvert
        if (menuId) {
            clickOutsideHandler = () => {
                closeMenu();
            };
            // Utiliser setTimeout pour éviter que le clic qui ouvre le menu le ferme immédiatement
            setTimeout(() => {
                if (clickOutsideHandler) {
                    document.addEventListener("click", clickOutsideHandler);
                }
            }, 0);
        }
    },
);

const onlineServers = computed(() =>
    allServers.value.filter((s: MediaServerSummary) => s.online),
);
const offlineServers = computed(() =>
    allServers.value.filter((s: MediaServerSummary) => !s.online),
);

// Mode du drawer
const isNavigating = computed(() => currentServer.value !== null);

function close() {
    emit("update:modelValue", false);
}

async function handleServerClick(server: MediaServerSummary) {
    if (!server.online) return;

    // Commencer la navigation dans ce serveur
    currentServer.value = server;
    isLoading.value = true;

    try {
        // Browse racine (containerId = "0")
        browseData.value = await browseContainer(server.id, "0");
        setPath([{ id: "0", title: server.friendly_name }]);
    } catch (error) {
        console.error("[ServerDrawer] Erreur browse racine:", error);
        browseData.value = null;
    } finally {
        isLoading.value = false;
    }
}

function goBack() {
    currentServer.value = null;
    browseData.value = null;
    clearPath();
}

async function handleContainerClick(item: ContainerEntry) {
    if (!currentServer.value) return;

    isLoading.value = true;
    try {
        browseData.value = await browseContainer(
            currentServer.value.id,
            item.id,
        );
        setPath([...currentPath.value, { id: item.id, title: item.title }]);
    } catch (error) {
        console.error("[ServerDrawer] Erreur browse container:", error);
    } finally {
        isLoading.value = false;
    }
}

async function handleBreadcrumbClick(index: number) {
    if (!currentServer.value || index >= currentPath.value.length) return;

    const targetCrumb = currentPath.value[index];
    if (!targetCrumb) return;

    isLoading.value = true;

    try {
        browseData.value = await browseContainer(
            currentServer.value.id,
            targetCrumb.id,
        );
        setPath(currentPath.value.slice(0, index + 1));
    } catch (error) {
        console.error("[ServerDrawer] Erreur breadcrumb navigation:", error);
    } finally {
        isLoading.value = false;
    }
}

// Détermine si un item est jouable (track ou container jouable comme album/playlist)
function isPlayable(item: ContainerEntry): boolean {
    if (!item.is_container) return true; // Les tracks sont toujours jouables

    // Containers jouables basés sur la classe UPnP
    const playableClasses = [
        "object.container.album",
        "object.container.playlist",
        "object.container.person.musicartist",
    ];

    return playableClasses.some((cls) =>
        item.class.toLowerCase().includes(cls),
    );
}

// Détermine si un container est navigable
function isNavigable(item: ContainerEntry): boolean {
    // Tous les containers sont navigables (on laisse le serveur décider si vide)
    return item.is_container;
}

function handleItemClick(item: ContainerEntry) {
    if (!currentServer.value) return;

    // Les containers sont toujours navigables
    if (item.is_container) {
        handleContainerClick(item);
    }
    // Les tracks individuels : on ne fait rien (actions via boutons)
}

function toggleMenu(itemId: string, event: Event) {
    event.stopPropagation();
    openMenuId.value = openMenuId.value === itemId ? null : itemId;
}

function closeMenu() {
    openMenuId.value = null;
}

async function handlePlayItem(event: Event, item: ContainerEntry) {
    event.stopPropagation();
    closeMenu();

    if (!currentServer.value || !props.selectedRendererId) {
        console.warn("[ServerDrawer] No server or renderer selected");
        return;
    }

    try {
        if (item.is_container) {
            // Container : attacher comme playlist avec auto_play
            await attachAndPlayPlaylist(
                props.selectedRendererId,
                currentServer.value.id,
                item.id,
            );
        } else {
            // Item : vider queue + ajouter + jouer
            await playContent(
                props.selectedRendererId,
                currentServer.value.id,
                item.id,
            );
        }
    } catch (error) {
        console.error("[ServerDrawer] Error playing item:", error);
    }
}

async function handleAddToQueue(event: Event, item: ContainerEntry) {
    event.stopPropagation();
    closeMenu();

    if (!currentServer.value || !props.selectedRendererId) {
        console.warn("[ServerDrawer] No server or renderer selected");
        return;
    }

    try {
        // Détacher le binding (fait côté serveur) + ajouter à la fin
        await addToQueue(
            props.selectedRendererId,
            currentServer.value.id,
            item.id,
        );
    } catch (error) {
        console.error("[ServerDrawer] Error adding to queue:", error);
    }
}

async function handleAddAfterCurrent(event: Event, item: ContainerEntry) {
    event.stopPropagation();
    closeMenu();

    if (!currentServer.value || !props.selectedRendererId) {
        console.warn("[ServerDrawer] No server or renderer selected");
        return;
    }

    try {
        // Détacher le binding + insérer après current
        await addAfterCurrent(
            props.selectedRendererId,
            currentServer.value.id,
            item.id,
        );
    } catch (error) {
        console.error("[ServerDrawer] Error adding after current:", error);
    }
}

function handleSettingsClick() {
    router.push("/debug/api-dashboard");
    close();
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
            <aside v-if="modelValue" class="server-drawer">
                <!-- Header - Liste des serveurs -->
                <header v-if="!isNavigating" class="drawer-header">
                    <div class="drawer-title-section">
                        <ServerIcon :size="24" />
                        <h2 class="drawer-title">Media Servers</h2>
                    </div>
                    <button
                        class="drawer-close-btn"
                        @click="close"
                        aria-label="Fermer"
                    >
                        <X :size="24" />
                    </button>
                </header>

                <!-- Header - Navigation dans un serveur -->
                <header v-else class="drawer-header">
                    <button
                        class="back-btn"
                        @click="goBack"
                        aria-label="Retour aux serveurs"
                    >
                        <ChevronLeft :size="20" />
                    </button>
                    <div class="drawer-title-section">
                        <ServerIcon :size="20" />
                        <h2 class="drawer-title small">
                            {{ currentServer?.friendly_name }}
                        </h2>
                    </div>
                    <button
                        class="drawer-close-btn"
                        @click="close"
                        aria-label="Fermer"
                    >
                        <X :size="24" />
                    </button>
                </header>

                <!-- Breadcrumb -->
                <nav
                    v-if="isNavigating && currentPath.length > 1"
                    class="breadcrumb"
                >
                    <button
                        v-for="(crumb, index) in currentPath"
                        :key="crumb.id"
                        class="breadcrumb-item"
                        :class="{ active: index === currentPath.length - 1 }"
                        @click="handleBreadcrumbClick(index)"
                    >
                        {{ crumb.title }}
                        <ChevronRight
                            v-if="index < currentPath.length - 1"
                            :size="14"
                        />
                    </button>
                </nav>

                <!-- Contenu -->
                <div class="drawer-content">
                    <!-- Liste des serveurs -->
                    <div v-if="!isNavigating">
                        <!-- Servers online -->
                        <section
                            v-if="onlineServers.length > 0"
                            class="server-section"
                        >
                            <h3 class="section-title">
                                Disponibles ({{ onlineServers.length }})
                            </h3>
                            <ul class="server-list">
                                <li
                                    v-for="server in onlineServers"
                                    :key="server.id"
                                    class="server-item online"
                                    @click="handleServerClick(server)"
                                >
                                    <div class="server-icon">
                                        <ServerIcon :size="20" />
                                    </div>
                                    <div class="server-info">
                                        <p class="server-name">
                                            {{ server.friendly_name }}
                                        </p>
                                        <p
                                            v-if="server.model_name"
                                            class="server-model"
                                        >
                                            {{ server.model_name }}
                                        </p>
                                    </div>
                                    <div class="server-status">
                                        <Circle :size="8" fill="currentColor" />
                                    </div>
                                </li>
                            </ul>
                        </section>

                        <!-- Servers offline -->
                        <section
                            v-if="offlineServers.length > 0"
                            class="server-section"
                        >
                            <h3 class="section-title">
                                Hors ligne ({{ offlineServers.length }})
                            </h3>
                            <ul class="server-list">
                                <li
                                    v-for="server in offlineServers"
                                    :key="server.id"
                                    class="server-item offline"
                                >
                                    <div class="server-icon">
                                        <ServerIcon :size="20" />
                                    </div>
                                    <div class="server-info">
                                        <p class="server-name">
                                            {{ server.friendly_name }}
                                        </p>
                                        <p
                                            v-if="server.model_name"
                                            class="server-model"
                                        >
                                            {{ server.model_name }}
                                        </p>
                                    </div>
                                    <div class="server-status">
                                        <Circle :size="8" fill="currentColor" />
                                    </div>
                                </li>
                            </ul>
                        </section>

                        <!-- Aucun server -->
                        <div
                            v-if="allServers.length === 0"
                            class="empty-servers"
                        >
                            <ServerIcon :size="48" />
                            <p>Aucun serveur multimédia détecté</p>
                        </div>
                    </div>

                    <!-- Navigation dans un serveur -->
                    <div v-else>
                        <!-- Loading -->
                        <div v-if="isLoading" class="loading-state">
                            <div class="spinner"></div>
                            <p>Chargement...</p>
                        </div>

                        <!-- Contenu du serveur -->
                        <ul v-else-if="browseData" class="content-list">
                            <li
                                v-for="item in browseData.entries"
                                :key="item.id"
                                class="content-item"
                                :class="{
                                    navigable:
                                        item.is_container && isNavigable(item),
                                    'menu-open': openMenuId === item.id,
                                }"
                                @click="handleItemClick(item)"
                            >
                                <!-- Cover avec image ou icône -->
                                <div class="content-cover">
                                    <img
                                        v-if="item.album_art_uri"
                                        :src="item.album_art_uri"
                                        :alt="item.title"
                                        class="cover-image"
                                        loading="lazy"
                                        @error="
                                            (e) => {
                                                (
                                                    e.target as HTMLImageElement
                                                ).style.display = 'none';
                                                const placeholder = (
                                                    e.target as HTMLElement
                                                )
                                                    .nextElementSibling as HTMLElement;
                                                if (placeholder)
                                                    placeholder.style.display =
                                                        'flex';
                                            }
                                        "
                                    />
                                    <div
                                        class="cover-placeholder"
                                        :style="{
                                            display: item.album_art_uri
                                                ? 'none'
                                                : 'flex',
                                        }"
                                    >
                                        <Folder
                                            v-if="item.is_container"
                                            :size="24"
                                        />
                                        <Music v-else :size="24" />
                                    </div>
                                    <!-- Petite icône de type pour containers jouables -->
                                    <div
                                        v-if="
                                            item.is_container &&
                                            isPlayable(item)
                                        "
                                        class="type-badge"
                                    >
                                        <Folder :size="12" />
                                    </div>
                                </div>

                                <div class="content-info">
                                    <p class="content-title">
                                        {{ item.title }}
                                    </p>
                                    <p
                                        v-if="item.artist"
                                        class="content-subtitle artist"
                                    >
                                        {{ item.artist }}
                                    </p>
                                    <p
                                        v-else-if="item.album"
                                        class="content-subtitle"
                                    >
                                        {{ item.album }}
                                    </p>
                                </div>

                                <!-- Actions pour items jouables -->
                                <div
                                    v-if="isPlayable(item)"
                                    class="content-actions"
                                >
                                    <!-- Bouton Play -->
                                    <button
                                        class="action-btn play-btn"
                                        :title="
                                            item.is_container
                                                ? 'Jouer tout'
                                                : 'Jouer'
                                        "
                                        @click="handlePlayItem($event, item)"
                                    >
                                        <Play :size="16" />
                                    </button>

                                    <!-- Bouton Menu avec dropdown -->
                                    <div class="action-menu-container">
                                        <button
                                            class="action-btn menu-btn"
                                            :title="'Plus d\'actions'"
                                            @click="toggleMenu(item.id, $event)"
                                        >
                                            <MoreVertical :size="16" />
                                        </button>

                                        <!-- Dropdown menu -->
                                        <Transition name="menu-fade">
                                            <div
                                                v-if="openMenuId === item.id"
                                                class="action-dropdown"
                                                @click.stop
                                            >
                                                <button
                                                    class="dropdown-item"
                                                    @click="
                                                        handleAddToQueue(
                                                            $event,
                                                            item,
                                                        )
                                                    "
                                                >
                                                    <Plus :size="14" />
                                                    <span
                                                        >Ajouter à la
                                                        queue</span
                                                    >
                                                </button>
                                                <button
                                                    class="dropdown-item"
                                                    @click="
                                                        handleAddAfterCurrent(
                                                            $event,
                                                            item,
                                                        )
                                                    "
                                                >
                                                    <Plus :size="14" />
                                                    <span>Ajouter après</span>
                                                </button>
                                            </div>
                                        </Transition>
                                    </div>
                                </div>

                                <!-- Chevron pour containers navigables (même si jouables) -->
                                <ChevronRight
                                    v-if="
                                        item.is_container && isNavigable(item)
                                    "
                                    :size="16"
                                    class="content-chevron"
                                />
                            </li>
                        </ul>

                        <!-- Vide -->
                        <div v-else class="empty-servers">
                            <Folder :size="48" />
                            <p>Dossier vide</p>
                        </div>
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
    left: 50vw; /* Commence après le drawer (desktop: 50vw) */
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.35); /* Moins sombre */
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    z-index: 200;
}

@media (max-width: 768px) and (orientation: portrait) {
    .drawer-backdrop {
        left: 0; /* Mobile portrait: backdrop commence à gauche car drawer prend 100vw */
        background: rgba(0, 0, 0, 0.4); /* Plus sombre sur mobile */
    }
}

.server-drawer {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: 50vw; /* Desktop/landscape: 50% de l'écran */
    background: rgba(255, 255, 255, 0.08); /* Plus transparent */
    backdrop-filter: blur(40px) saturate(180%);
    -webkit-backdrop-filter: blur(40px) saturate(180%);
    border-right: 1px solid rgba(255, 255, 255, 0.15);
    box-shadow: 4px 0 32px rgba(0, 0, 0, 0.25);
    z-index: 201;
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

@media (prefers-color-scheme: dark) {
    .server-drawer {
        background: rgba(0, 0, 0, 0.4);
        border-right-color: rgba(255, 255, 255, 0.1);
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

.drawer-title.small {
    font-size: var(--text-base);
}

.back-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    padding: 0;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    cursor: pointer;
    transition: all 0.2s ease;
    color: var(--color-text);
}

.back-btn:hover {
    background: rgba(255, 255, 255, 0.2);
    transform: scale(1.05);
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

/* Breadcrumb */
.breadcrumb {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: var(--spacing-sm) var(--spacing-md);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    overflow-x: auto;
    flex-shrink: 0;
}

.breadcrumb-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
}

.breadcrumb-item:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text);
}

.breadcrumb-item.active {
    color: var(--color-text);
    font-weight: 600;
    cursor: default;
}

.breadcrumb-item.active:hover {
    background: transparent;
}

/* Content */
.drawer-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-md);
}

.server-section {
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

.server-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.server-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-md);
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    transition: all 0.2s ease;
}

.server-item.online {
    cursor: pointer;
}

.server-item.online:hover {
    background: rgba(255, 255, 255, 0.15);
    border-color: rgba(255, 255, 255, 0.2);
    transform: translateX(4px);
}

.server-item.online:active {
    transform: translateX(2px);
}

.server-item.offline {
    opacity: 0.5;
    cursor: not-allowed;
}

.server-icon {
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

.server-info {
    flex: 1;
    min-width: 0;
}

.server-name {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.server-model {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 2px 0 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.server-status {
    flex-shrink: 0;
    color: var(--status-playing);
}

.server-item.offline .server-status {
    color: var(--status-offline);
}

/* Content list */
.content-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
}

.content-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-sm) var(--spacing-md);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    transition: all 0.2s ease;
    position: relative;
}

.content-item.navigable {
    cursor: pointer;
}

.content-item.navigable:hover {
    background: rgba(255, 255, 255, 0.12);
    transform: translateX(2px);
}

.content-item.navigable:active {
    transform: translateX(1px);
}

.content-item.menu-open {
    z-index: 200; /* Passe au-dessus des autres items quand son menu est ouvert */
}

/* Cover avec image */
.content-cover {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    flex-shrink: 0;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-secondary);
    overflow: hidden;
}

.cover-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.cover-placeholder {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
}

.type-badge {
    position: absolute;
    bottom: 3px;
    right: 3px;
    width: 18px;
    height: 18px;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(4px);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.9);
}

.content-info {
    flex: 1;
    min-width: 0;
}

.content-title {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--color-text);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.content-subtitle {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    margin: 2px 0 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.content-subtitle.artist {
    font-weight: 500;
    color: var(--color-text);
}

.content-chevron {
    flex-shrink: 0;
    color: var(--color-text-tertiary);
}

/* Actions pour items jouables */
.content-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    opacity: 1; /* Toujours visible pour le tactile */
    transition: all 0.2s ease;
    z-index: 10; /* Au-dessus pour capturer les clicks */
    position: relative; /* Crée un contexte de stacking */
}

.action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.2s ease;
    color: var(--color-text);
    pointer-events: auto; /* S'assurer que les boutons capturent les clicks */
}

.action-btn:hover {
    background: rgba(255, 255, 255, 0.2);
    transform: scale(1.05);
}

.action-btn:active {
    transform: scale(0.95);
}

.play-btn {
    color: var(--color-primary);
    border-color: var(--color-primary);
}

.play-btn:hover {
    background: var(--color-primary);
    color: white;
    box-shadow: 0 2px 8px rgba(102, 126, 234, 0.4);
}

.menu-btn {
    color: var(--color-text-secondary);
}

.menu-btn:hover {
    background: rgba(255, 255, 255, 0.25);
    color: var(--color-text);
}

/* Menu dropdown container */
.action-menu-container {
    position: relative;
    z-index: 100; /* Plus élevé que les content-item pour que le dropdown passe au-dessus */
}

/* Dropdown menu */
.action-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 180px;
    background: rgba(20, 20, 30, 0.98);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    padding: 4px;
    z-index: 10000; /* Très haut pour passer au-dessus de tout */
}

@media (prefers-color-scheme: light) {
    .action-dropdown {
        background: rgba(255, 255, 255, 0.98);
        border-color: rgba(0, 0, 0, 0.1);
    }
}

/* Dropdown items */
.dropdown-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    width: 100%;
    padding: var(--spacing-sm) var(--spacing-md);
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--color-text);
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
    transition: all 0.2s ease;
}

.dropdown-item:hover {
    background: rgba(255, 255, 255, 0.1);
}

.dropdown-item:active {
    transform: scale(0.98);
}

.dropdown-item span {
    flex: 1;
}

/* Menu fade animation */
.menu-fade-enter-active {
    transition: all 0.15s ease-out;
}

.menu-fade-leave-active {
    transition: all 0.1s ease-in;
}

.menu-fade-enter-from {
    opacity: 0;
    transform: translateY(-8px) scale(0.95);
}

.menu-fade-leave-to {
    opacity: 0;
    transform: translateY(-4px) scale(0.98);
}

/* Loading state */
.loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-md);
    padding: var(--spacing-2xl);
    color: var(--color-text-secondary);
}

.spinner {
    width: 32px;
    height: 32px;
    border: 3px solid rgba(255, 255, 255, 0.2);
    border-top-color: var(--color-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
}

@keyframes spin {
    to {
        transform: rotate(360deg);
    }
}

.loading-state p {
    margin: 0;
    font-size: var(--text-sm);
}

.empty-servers {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-md);
    padding: var(--spacing-2xl);
    text-align: center;
    color: var(--color-text-secondary);
}

.empty-servers p {
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
    /* Pas de delay au leave - disparaît en même temps que le drawer */
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
    transform: translateX(-100%);
    opacity: 0;
}

.drawer-leave-to {
    transform: translateX(-100%);
    opacity: 0;
}

/* Animation des contenus - désactivée pour éviter les problèmes de z-index en escalier */

/* Scrollbar styling */
.drawer-content::-webkit-scrollbar,
.breadcrumb::-webkit-scrollbar {
    width: 6px;
    height: 6px;
}

.drawer-content::-webkit-scrollbar-track,
.breadcrumb::-webkit-scrollbar-track {
    background: rgba(255, 255, 255, 0.05);
    border-radius: 3px;
}

.drawer-content::-webkit-scrollbar-thumb,
.breadcrumb::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.2);
    border-radius: 3px;
}

.drawer-content::-webkit-scrollbar-thumb:hover,
.breadcrumb::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.3);
}

/* Mobile responsive - portrait */
@media (max-width: 768px) and (orientation: portrait) {
    .server-drawer {
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

    /* Ajuster les items pour mobile */
    .content-item {
        padding: var(--spacing-md);
    }

    .content-cover {
        width: 64px;
        height: 64px;
    }
}

/* Fallback pour navigateurs sans backdrop-filter */
@supports not (backdrop-filter: blur(30px)) {
    .server-drawer {
        background: rgba(255, 255, 255, 0.98);
    }

    @media (prefers-color-scheme: dark) {
        .server-drawer {
            background: rgba(20, 20, 30, 0.98);
        }
    }
}
</style>
