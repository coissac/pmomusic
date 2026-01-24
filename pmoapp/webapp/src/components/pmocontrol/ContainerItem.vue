<script setup lang="ts">
import { computed } from "vue";
import type { ContainerEntry } from "@/services/pmocontrol/types";
import { Folder, Music } from "lucide-vue-next";
import ActionMenu from "./ActionMenu.vue";
import { useCoverImage } from "@/composables/useCoverImage";

const props = defineProps<{
    entry: ContainerEntry;
    serverId: string;
    showActions?: boolean;
}>();

const emit = defineEmits<{
    browse: [containerId: string];
    playNow: [containerId: string, rendererId: string];
    addToQueue: [containerId: string, rendererId: string];
}>();

// Use the new cover image composable
const albumArtUri = computed(() => props.entry.album_art_uri);
const {
    imageLoaded,
    imageError,
    coverImageRef,
    cacheBustedUrl,
    handleImageLoad,
    handleImageError,
} = useCoverImage(albumArtUri);

const iconComponent = computed(() => {
    const cls = props.entry.class.toLowerCase();
    if (cls.includes("playlist")) return Music;
    if (cls.includes("album")) return Music;
    return Folder;
});

const containerType = computed(() => {
    const cls = props.entry.class.toLowerCase();
    if (cls.includes("playlist")) return "Playlist";
    if (cls.includes("album")) return "Album";
    if (cls.includes("artist")) return "Artiste";
    if (cls.includes("genre")) return "Genre";
    return "Dossier";
});

const isPlayable = computed(() => {
    const cls = props.entry.class.toLowerCase();
    return cls.includes("playlist") || cls.includes("album");
});

function handleBrowse() {
    emit("browse", props.entry.id);
}

function handlePlayNow(rendererId: string) {
    emit("playNow", props.entry.id, rendererId);
}

function handleAddToQueue(rendererId: string) {
    emit("addToQueue", props.entry.id, rendererId);
}
</script>

<template>
    <div class="container-item">
        <!-- Main content (clickable) -->
        <button class="container-content" @click="handleBrowse">
            <!-- Cover avec icône de type en overlay -->
            <div class="container-cover">
                <img
                    ref="coverImageRef"
                    :style="{
                        opacity:
                            cacheBustedUrl && imageLoaded && !imageError
                                ? 1
                                : 0,
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
                    :alt="entry.title"
                    class="cover-image"
                    @load="handleImageLoad"
                    @error="handleImageError"
                />
                <div
                    v-show="!cacheBustedUrl || imageError || !imageLoaded"
                    class="cover-placeholder"
                >
                    <component :is="iconComponent" :size="28" />
                </div>
                <!-- Petite icône de type dans le coin inférieur droit -->
                <div v-if="isPlayable" class="type-badge">
                    <Folder :size="14" />
                </div>
            </div>

            <!-- Métadonnées -->
            <div class="container-metadata">
                <div class="container-title">{{ entry.title }}</div>
                <div class="container-details">
                    <span v-if="entry.artist" class="container-artist">{{
                        entry.artist
                    }}</span>
                    <span class="container-type">{{ containerType }}</span>
                    <span
                        v-if="entry.child_count !== null"
                        class="container-count"
                    >
                        {{ entry.child_count }} élément{{
                            entry.child_count > 1 ? "s" : ""
                        }}
                    </span>
                </div>
            </div>
        </button>

        <!-- Actions menu -->
        <div class="container-actions">
            <ActionMenu
                type="container"
                :entry-id="entry.id"
                :server-id="serverId"
                @play-now="handlePlayNow"
                @add-to-queue="handleAddToQueue"
            />
        </div>
    </div>
</template>

<style scoped>
.container-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm);
    border-radius: var(--radius-md);
    transition: background-color var(--transition-fast);
    border: 1px solid transparent;
}

.container-item:hover {
    background-color: var(--color-bg-secondary);
    border-color: var(--color-border);
}

.container-content {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-align: left;
    min-width: 0;
}

/* Cover avec image et icône de type */
.container-cover {
    position: relative;
    flex-shrink: 0;
    width: 64px;
    height: 64px;
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
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-primary);
}

.type-badge {
    position: absolute;
    bottom: 4px;
    right: 4px;
    width: 20px;
    height: 20px;
    background-color: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
}

/* Métadonnées */
.container-metadata {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
}

.container-title {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.3;
}

.container-details {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-xs) var(--spacing-sm);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    line-height: 1.4;
}

.container-artist {
    font-weight: 500;
    color: var(--color-text);
}

.container-type {
    font-weight: 500;
}

.container-count::before {
    content: "•";
    margin-right: var(--spacing-sm);
}

.container-actions {
    flex-shrink: 0;
}

.btn-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all var(--transition-fast);
}

.btn-icon:hover {
    background-color: var(--color-bg-tertiary);
    color: var(--color-text);
}
</style>
