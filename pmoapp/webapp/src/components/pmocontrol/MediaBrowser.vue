<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useMediaServers } from "@/composables/useMediaServers";
import { useRenderers } from "@/composables/useRenderers";
import { useUIStore } from "@/stores/ui";
import Breadcrumb from "./Breadcrumb.vue";
import ContainerItem from "./ContainerItem.vue";
import MediaItem from "./MediaItem.vue";
import { Loader2, Search, X } from "lucide-vue-next";

const props = defineProps<{
    serverId: string;
    containerId: string;
}>();

const {
    getBrowseCached,
    browseContainer,
    loadMoreBrowse,
    hasMore,
    currentPath: breadcrumbPath,
    loading,
    loadingMore,
    error,
    searchResults,
    searchQuery,
    searchServer,
    clearSearch,
} = useMediaServers();

const searchInput = ref('');

async function handleSearch() {
    if (searchInput.value.trim()) {
        await searchServer(props.serverId, searchInput.value.trim());
    }
}

function handleClearSearch() {
    searchInput.value = '';
    clearSearch();
}

const isSearchMode = computed(() => searchQuery.value !== '');

const { playContent, addToQueue, attachAndPlayPlaylist, attachPlaylist } =
    useRenderers();
const uiStore = useUIStore();

const isRefreshing = ref(false);
const sentinelRef = ref<HTMLElement | null>(null);
let observer: IntersectionObserver | null = null;

const browseData = computed(() =>
    isSearchMode.value
        ? searchResults.value
        : getBrowseCached(props.serverId, props.containerId),
);

const containers = computed(
    () => browseData.value?.entries.filter((e) => e.is_container) || [],
);

const items = computed(
    () => browseData.value?.entries.filter((e) => !e.is_container) || [],
);

const canLoadMore = computed(() => !isSearchMode.value && hasMore(props.serverId, props.containerId));

function setupObserver() {
    if (observer) observer.disconnect();
    observer = new IntersectionObserver(
        (entries) => {
            if (entries[0]?.isIntersecting && canLoadMore.value && !loadingMore.value) {
                loadMoreBrowse(props.serverId, props.containerId);
            }
        },
        { threshold: 0.1 },
    );
    if (sentinelRef.value) observer.observe(sentinelRef.value);
}

onMounted(() => setupObserver());
onBeforeUnmount(() => observer?.disconnect());

watch(sentinelRef, (el) => {
    if (el) setupObserver();
});

// Charger le container au montage et quand containerId change
watch(
    () => props.containerId,
    async (newContainerId) => {
        if (newContainerId) {
            await browseContainer(props.serverId, newContainerId);
        }
    },
    { immediate: true },
);

// Recharger si le cache est invalidé (ContainersUpdated SSE)
watch(
    () => browseData.value,
    async (data) => {
        if (
            !data &&
            props.containerId &&
            !loading.value &&
            !isRefreshing.value
        ) {
            console.log(
                `[MediaBrowser] Cache invalidé pour ${props.serverId}/${props.containerId}, rechargement...`,
            );
            isRefreshing.value = true;
            await browseContainer(props.serverId, props.containerId, false);
            isRefreshing.value = false;
        }
    },
);

const emit = defineEmits<{
    navigate: [containerId: string];
}>();

function handleNavigate(containerId: string) {
    emit("navigate", containerId);
}

function handleBrowseContainer(containerId: string) {
    emit("navigate", containerId);
}

// Actions handlers pour les containers (playlists/albums)
async function handlePlayContainer(containerId: string, rendererId: string) {
    try {
        await attachAndPlayPlaylist(rendererId, props.serverId, containerId);
        uiStore.notifySuccess("Lecture de la playlist démarrée !");
    } catch (err) {
        const message = err instanceof Error ? err.message : "Erreur inconnue";
        uiStore.notifyError(
            `Erreur lors de la lecture de la playlist: ${message}`,
        );
    }
}

async function handleQueueContainer(containerId: string, rendererId: string) {
    try {
        await attachPlaylist(rendererId, props.serverId, containerId);
        uiStore.notifySuccess("Playlist attachée à la queue !");
    } catch (err) {
        const message = err instanceof Error ? err.message : "Erreur inconnue";
        uiStore.notifyError(
            `Erreur lors de l'ajout de la playlist: ${message}`,
        );
    }
}

// Actions handlers pour les items (tracks)
async function handlePlayItem(itemId: string, rendererId: string) {
    try {
        await playContent(rendererId, props.serverId, itemId);
        uiStore.notifySuccess("Lecture démarrée !");
    } catch (err) {
        const message = err instanceof Error ? err.message : "Erreur inconnue";
        uiStore.notifyError(`Erreur lors de la lecture: ${message}`);
    }
}

async function handleQueueItem(itemId: string, rendererId: string) {
    try {
        await addToQueue(rendererId, props.serverId, itemId);
        uiStore.notifySuccess("Ajouté à la queue !");
    } catch (err) {
        const message = err instanceof Error ? err.message : "Erreur inconnue";
        uiStore.notifyError(`Erreur lors de l'ajout à la queue: ${message}`);
    }
}
</script>

<template>
    <div class="media-browser">
        <!-- Breadcrumb -->
        <Breadcrumb
            :items="breadcrumbPath"
            :serverId="serverId"
            @navigate="handleNavigate"
        />

        <!-- Search bar -->
        <div class="search-bar">
            <div class="search-input-wrapper">
                <Search :size="16" class="search-icon" />
                <input
                    v-model="searchInput"
                    type="text"
                    class="search-input"
                    placeholder="Rechercher..."
                    @keyup.enter="handleSearch"
                />
                <button
                    v-if="searchInput || isSearchMode"
                    class="search-clear"
                    @click="handleClearSearch"
                    title="Effacer"
                >
                    <X :size="14" />
                </button>
            </div>
            <button class="btn btn-primary search-btn" @click="handleSearch">
                Rechercher
            </button>
        </div>

        <!-- Loading state -->
        <div v-if="loading" class="browser-loading">
            <Loader2 :size="32" class="spinner" />
            <p>Chargement...</p>
        </div>

        <!-- Error state -->
        <div v-else-if="error" class="browser-error">
            <p class="error-message">{{ error }}</p>
            <button
                class="btn btn-secondary"
                @click="browseContainer(serverId, containerId, false)"
            >
                Réessayer
            </button>
        </div>

        <!-- Content -->
        <div v-else class="browser-content">
            <!-- Containers section -->
            <div v-if="containers.length" class="browser-section">
                <h3 class="section-title">Dossiers et playlists</h3>
                <div class="entries-list">
                    <ContainerItem
                        v-for="container in containers"
                        :key="container.id"
                        :entry="container"
                        :server-id="serverId"
                        @browse="handleBrowseContainer"
                        @play-now="handlePlayContainer"
                        @add-to-queue="handleQueueContainer"
                    />
                </div>
            </div>

            <!-- Items section -->
            <div v-if="items.length" class="browser-section">
                <h3 class="section-title">Pistes</h3>
                <div class="entries-list">
                    <MediaItem
                        v-for="item in items"
                        :key="item.id"
                        :entry="item"
                        :server-id="serverId"
                        @play-now="handlePlayItem"
                        @add-to-queue="handleQueueItem"
                    />
                </div>
            </div>

            <!-- Empty state -->
            <div
                v-if="!containers.length && !items.length"
                class="browser-empty"
            >
                <p>{{ isSearchMode ? 'Aucun résultat' : 'Ce dossier est vide' }}</p>
            </div>

            <!-- Sentinel infinite scroll -->
            <div ref="sentinelRef" class="scroll-sentinel" />

            <!-- Spinner load more -->
            <div v-if="loadingMore" class="load-more-spinner">
                <Loader2 :size="20" class="spinner" />
            </div>
        </div>
    </div>
</template>

<style scoped>
.media-browser {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-lg);
    height: 100%;
}

/* Loading */
.browser-loading {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-md);
    color: var(--color-text-secondary);
}

.spinner {
    animation: spin 1s linear infinite;
}

/* @keyframes spin is now global in pmocontrol.css */

/* Error */
.browser-error {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-md);
}

.error-message {
    font-size: var(--text-base);
    color: var(--status-offline);
    margin: 0;
}

/* Content */
.browser-content {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xl);
    padding-right: var(--spacing-xs);
}

.browser-section {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
}

.section-title {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-text);
    margin: 0;
    padding-bottom: var(--spacing-sm);
    border-bottom: 1px solid var(--color-border);
}

.entries-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
}

/* Empty state */
.browser-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-tertiary);
    font-size: var(--text-base);
    padding: var(--spacing-xl);
}

.scroll-sentinel {
    height: 1px;
}

.load-more-spinner {
    display: flex;
    justify-content: center;
    padding: var(--spacing-md);
    color: var(--color-text-secondary);
}

/* Search */
.search-bar {
    display: flex;
    gap: var(--spacing-sm);
    align-items: center;
}

.search-input-wrapper {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
}

.search-icon {
    position: absolute;
    left: var(--spacing-sm);
    color: var(--color-text-tertiary);
    pointer-events: none;
}

.search-input {
    width: 100%;
    padding: var(--spacing-xs) var(--spacing-xl) var(--spacing-xs) calc(var(--spacing-sm) + 20px);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-secondary);
    color: var(--color-text);
    font-size: var(--text-sm);
}

.search-input:focus {
    outline: none;
    border-color: var(--color-primary);
}

.search-clear {
    position: absolute;
    right: var(--spacing-xs);
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-text-tertiary);
    display: flex;
    align-items: center;
    padding: 2px;
}

.search-clear:hover {
    color: var(--color-text);
}

.search-btn {
    white-space: nowrap;
    padding: var(--spacing-xs) var(--spacing-md);
    font-size: var(--text-sm);
}

/* Scrollbar styling */
.browser-content::-webkit-scrollbar {
    width: 6px;
}

.browser-content::-webkit-scrollbar-track {
    background: var(--color-bg-secondary);
    border-radius: var(--radius-full);
}

.browser-content::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: var(--radius-full);
}

.browser-content::-webkit-scrollbar-thumb:hover {
    background: var(--color-text-tertiary);
}
</style>
