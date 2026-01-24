<script setup lang="ts">
import { computed } from "vue";
import { Music, Play } from "lucide-vue-next";
import type { QueueItem } from "@/services/pmocontrol/types";
import { useCoverImage } from "@/composables/useCoverImage";

const props = defineProps<{
    item: QueueItem;
    isCurrent: boolean;
}>();

const emit = defineEmits<{
    click: [item: QueueItem];
}>();

// Use the new cover image composable
const albumArtUri = computed(() => props.item.album_art_uri);
const {
    imageLoaded,
    imageError,
    coverImageRef,
    cacheBustedUrl,
    handleImageLoad,
    handleImageError,
} = useCoverImage(albumArtUri);

function handleClick(item: QueueItem) {
    console.log("[QueueItem] Click detected on item:", item.index, item.title);
    emit("click", item);
}
</script>

<template>
    <div
        :class="['queue-item', { current: isCurrent }]"
        @click="handleClick(item)"
    >
        <!-- Indicateur piste en cours -->
        <div class="current-indicator" v-if="isCurrent">
            <Play :size="16" fill="currentColor" />
        </div>

        <!-- Index (1-based pour l'affichage) -->
        <span class="item-index">{{ item.index + 1 }}</span>

        <!-- Cover miniature -->
        <div class="item-cover">
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
                :alt="item.album || 'Album cover'"
                class="cover-image"
                @load="handleImageLoad"
                @error="handleImageError"
            />
            <Music
                v-show="!cacheBustedUrl || imageError || !imageLoaded"
                :size="20"
            />
        </div>

        <!-- Métadonnées -->
        <div class="item-metadata">
            <div class="item-title">{{ item.title || "Sans titre" }}</div>
            <div class="item-artist">
                {{ item.artist || "Artiste inconnu" }}
                <span v-if="item.album"> • {{ item.album }}</span>
            </div>
        </div>
    </div>
</template>

<style scoped>
.queue-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-sm);
    border-radius: var(--radius-md);
    transition: background-color var(--transition-fast);
    position: relative;
    cursor: pointer;
}

.queue-item:hover {
    background-color: var(--color-bg-secondary);
}

.queue-item.current {
    background-color: var(--status-playing-bg);
    border: 1px solid var(--status-playing);
    font-weight: 600;
}

.current-indicator {
    position: absolute;
    left: var(--spacing-xs);
    color: var(--status-playing);
    display: flex;
    align-items: center;
}

.item-index {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    min-width: 2rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
}

.queue-item.current .item-index {
    margin-left: var(--spacing-lg);
}

.item-cover {
    width: 48px;
    height: 48px;
    background-color: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-tertiary);
    flex-shrink: 0;
    overflow: hidden;
}

.cover-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.item-metadata {
    flex: 1;
    min-width: 0;
}

.item-title {
    font-size: var(--text-base);
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.queue-item.current .item-title {
    color: var(--status-playing);
}

.item-artist {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
</style>
