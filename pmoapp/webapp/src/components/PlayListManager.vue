<template>
    <div class="playlist-manager">
        <div class="header">
            <div>
                <h2>🎚️ Playlist Manager</h2>
                <p class="subtitle">
                    Inspect pmoplaylist state, debug lazy PKs and tune
                    capacities.
                </p>
            </div>
            <div class="header-stats">
                <span>{{ playlists.length }} playlists</span>
                <span>{{ totalTrackCount }} tracks</span>
                <span v-if="persistentCount > 0"
                    >{{ persistentCount }} persistent</span
                >
            </div>
            <div class="header-actions">
                <button @click="refreshPlaylists" :disabled="listLoading">
                    {{ listLoading ? "Refreshing..." : "Refresh" }}
                </button>
            </div>
        </div>

        <div class="panels">
            <section class="side-panel">
                <div class="section-card">
                    <h3>➕ Create Playlist</h3>
                    <form @submit.prevent="handleCreatePlaylist">
                        <label class="field">
                            <span>ID</span>
                            <input
                                v-model.trim="createForm.id"
                                type="text"
                                placeholder="unique id"
                                required
                            />
                        </label>
                        <label class="field">
                            <span>Title</span>
                            <input
                                v-model.trim="createForm.title"
                                type="text"
                                placeholder="Friendly label"
                            />
                        </label>
                        <label class="field">
                            <span>Role</span>
                            <select v-model="createForm.role">
                                <option
                                    v-for="option in roleOptions"
                                    :key="option.value"
                                    :value="option.value"
                                >
                                    {{ option.label }}
                                </option>
                            </select>
                        </label>
                        <label
                            v-if="createForm.role === 'custom'"
                            class="field"
                        >
                            <span>Custom role</span>
                            <input
                                v-model.trim="createForm.customRole"
                                type="text"
                                placeholder="ex: qobuz/discover"
                            />
                        </label>
                        <label class="field">
                            <span>Cover PK (optional)</span>
                            <input
                                v-model.trim="createForm.coverPk"
                                type="text"
                                placeholder="cover cache pk"
                            />
                        </label>
                        <div
                            class="cover-preview-row"
                            v-if="createCoverPreview"
                        >
                            <div class="cover-preview small">
                                <img
                                    :src="createCoverPreview"
                                    alt="Cover preview"
                                    loading="lazy"
                                />
                            </div>
                            <span class="small muted"
                                >Preview sourced from /covers/image/{{
                                    createForm.coverPk
                                }}</span
                            >
                        </div>
                        <label class="checkbox">
                            <input
                                type="checkbox"
                                v-model="createForm.persistent"
                            />
                            Persistent (stored in SQLite)
                        </label>
                        <div class="field-group">
                            <label class="field">
                                <span>Max size</span>
                                <input
                                    v-model="createForm.maxSize"
                                    type="number"
                                    min="0"
                                    placeholder="Unlimited"
                                />
                            </label>
                            <label class="field">
                                <span>Default TTL (s)</span>
                                <input
                                    v-model="createForm.defaultTtl"
                                    type="number"
                                    min="0"
                                    placeholder="Never expire"
                                />
                            </label>
                        </div>
                        <div class="form-messages">
                            <p v-if="createState.error" class="error">
                                ❌ {{ createState.error }}
                            </p>
                            <p v-if="createState.success" class="success">
                                ✅ {{ createState.success }}
                            </p>
                        </div>
                        <button
                            type="submit"
                            class="primary"
                            :disabled="createState.busy"
                        >
                            {{
                                createState.busy
                                    ? "Creating..."
                                    : "Create playlist"
                            }}
                        </button>
                    </form>
                </div>

                <div class="section-card list-section">
                    <div class="section-header">
                        <h3>📚 Playlists</h3>
                        <span v-if="listError" class="error small"
                            >❌ {{ listError }}</span
                        >
                    </div>
                    <div
                        v-if="listLoading && playlists.length === 0"
                        class="empty-state"
                    >
                        Loading playlists...
                    </div>
                    <div v-else-if="playlists.length === 0" class="empty-state">
                        No playlists yet. Create one above.
                    </div>
                    <div v-else class="playlist-list">
                        <article
                            v-for="playlist in sortedPlaylists"
                            :key="playlist.id"
                            :class="[
                                'playlist-card',
                                {
                                    selected:
                                        playlist.id === selectedPlaylistId,
                                },
                            ]"
                            @click="selectedPlaylistId = playlist.id"
                        >
                            <div class="card-title">
                                <strong>{{
                                    playlist.title || playlist.id
                                }}</strong>
                                <small>#{{ playlist.id }}</small>
                            </div>
                            <div class="card-metrics">
                                <span>{{ playlist.track_count }} tracks</span>
                                <span>{{
                                    formatRoleLabel(playlist.role)
                                }}</span>
                                <span v-if="playlist.persistent" class="pill"
                                    >Persistent</span
                                >
                                <span v-else class="pill transient"
                                    >Transient</span
                                >
                            </div>
                            <div v-if="playlist.artist" class="card-artist">
                                <span class="artist-label"
                                    >🎤 {{ playlist.artist }}</span
                                >
                            </div>
                            <div class="card-meta">
                                <span
                                    >Max:
                                    {{
                                        formatCapacity(playlist.max_size)
                                    }}</span
                                >
                                <span
                                    >TTL:
                                    {{
                                        formatTtl(playlist.default_ttl_secs)
                                    }}</span
                                >
                            </div>
                            <div class="card-meta">
                                <span
                                    >Updated
                                    {{
                                        formatRelativeDate(playlist.last_change)
                                    }}</span
                                >
                            </div>
                            <div class="card-actions">
                                <button
                                    class="btn-secondary"
                                    @click.stop="
                                        handleFlushPlaylist(playlist.id)
                                    "
                                    :disabled="
                                        flushState.busy &&
                                        flushState.targetId === playlist.id
                                    "
                                >
                                    {{
                                        flushState.busy &&
                                        flushState.targetId === playlist.id
                                            ? "Flushing..."
                                            : "Flush"
                                    }}
                                </button>
                                <button
                                    class="btn-danger"
                                    @click.stop="
                                        handleDeletePlaylist(playlist.id)
                                    "
                                    :disabled="
                                        deleteState.busy &&
                                        deleteState.targetId === playlist.id
                                    "
                                >
                                    {{
                                        deleteState.busy &&
                                        deleteState.targetId === playlist.id
                                            ? "Deleting..."
                                            : "Delete"
                                    }}
                                </button>
                            </div>
                        </article>
                    </div>
                </div>
            </section>

            <section class="detail-panel">
                <div class="section-card detail-card">
                    <template v-if="!selectedSummary">
                        <div class="empty-state">
                            Select a playlist to inspect details.
                        </div>
                    </template>
                    <template v-else>
                        <div class="detail-header">
                            <div class="detail-cover">
                                <img
                                    v-if="summaryCoverUrl"
                                    :src="summaryCoverUrl"
                                    alt="Playlist cover"
                                />
                                <div v-else class="cover-placeholder large">
                                    🎵
                                </div>
                            </div>
                            <div class="detail-header-main">
                                <h3>
                                    {{ detailTitle }}
                                    <span class="pill">{{
                                        selectedSummary
                                            ? formatRoleLabel(
                                                  selectedSummary.role,
                                              )
                                            : "Unknown"
                                    }}</span>
                                </h3>
                                <p class="detail-id">
                                    {{ detailId ? `#${detailId}` : "—" }}
                                </p>
                            </div>
                            <div class="detail-header-actions">
                                <button
                                    @click="loadSelectedPlaylist(false)"
                                    :disabled="detailLoading"
                                >
                                    {{
                                        detailLoading
                                            ? "Reloading..."
                                            : "Reload"
                                    }}
                                </button>
                            </div>
                        </div>
                        <div class="detail-stats">
                            <span
                                ><strong>{{
                                    selectedSummary
                                        ? selectedSummary.track_count
                                        : 0
                                }}</strong>
                                tracks</span
                            >
                            <span>
                                <strong>{{
                                    formatCapacity(
                                        selectedSummary
                                            ? selectedSummary.max_size
                                            : undefined,
                                    )
                                }}</strong>
                                capacity
                            </span>
                            <span>
                                <strong>{{
                                    formatTtl(
                                        selectedSummary
                                            ? selectedSummary.default_ttl_secs
                                            : undefined,
                                    )
                                }}</strong>
                                TTL
                            </span>
                            <span>
                                Last change:
                                {{
                                    selectedSummary
                                        ? formatRelativeDate(
                                              selectedSummary.last_change,
                                          )
                                        : "—"
                                }}
                            </span>
                            <span class="cover-stat">
                                Cover:
                                <template v-if="selectedSummary?.cover_pk">
                                    <code>{{ selectedSummary.cover_pk }}</code>
                                    <button
                                        class="btn-secondary btn-compact"
                                        @click="
                                            selectedSummary?.cover_pk &&
                                            copyPk(selectedSummary.cover_pk)
                                        "
                                    >
                                        Copy
                                    </button>
                                </template>
                                <template v-else>
                                    <span>None</span>
                                </template>
                            </span>
                        </div>
                        <div class="detail-messages">
                            <p v-if="detailError" class="error">
                                ❌ {{ detailError }}
                            </p>
                            <p v-if="updateState.message" class="success">
                                ✅ {{ updateState.message }}
                            </p>
                            <p v-if="addTracksState.success" class="success">
                                ✅ {{ addTracksState.success }}
                            </p>
                        </div>

                        <div
                            v-if="detailLoading && !selectedPlaylist"
                            class="empty-state"
                        >
                            Loading playlist details...
                        </div>
                        <div v-else-if="selectedPlaylist">
                            <div class="section-subcard">
                                <h4>Metadata & Config</h4>
                                <form @submit.prevent="handleUpdatePlaylist">
                                    <div class="field-grid">
                                        <label class="field">
                                            <span>Title</span>
                                            <input
                                                v-model.trim="updateForm.title"
                                                type="text"
                                            />
                                        </label>
                                        <label class="field">
                                            <span>Role</span>
                                            <select v-model="updateForm.role">
                                                <option
                                                    v-for="option in roleOptions"
                                                    :key="option.value"
                                                    :value="option.value"
                                                >
                                                    {{ option.label }}
                                                </option>
                                            </select>
                                        </label>
                                        <label
                                            v-if="updateForm.role === 'custom'"
                                            class="field"
                                        >
                                            <span>Custom role</span>
                                            <input
                                                v-model.trim="
                                                    updateForm.customRole
                                                "
                                                type="text"
                                            />
                                        </label>
                                        <label class="field">
                                            <span>Max size</span>
                                            <input
                                                v-model="updateForm.maxSize"
                                                type="number"
                                                min="0"
                                                placeholder="Unlimited"
                                            />
                                        </label>
                                        <label class="field">
                                            <span>Default TTL (s)</span>
                                            <input
                                                v-model="updateForm.defaultTtl"
                                                type="number"
                                                min="0"
                                                placeholder="Never expire"
                                            />
                                        </label>
                                    </div>
                                    <label class="field">
                                        <span>Cover PK</span>
                                        <input
                                            v-model="updateForm.coverPk"
                                            type="text"
                                            placeholder="cover cache pk (leave blank to clear)"
                                        />
                                    </label>
                                    <div class="cover-preview-row">
                                        <div class="cover-preview">
                                            <img
                                                v-if="updateCoverPreview"
                                                :src="updateCoverPreview"
                                                alt="Cover preview"
                                                loading="lazy"
                                            />
                                            <div
                                                v-else
                                                class="cover-placeholder small"
                                            >
                                                No cover
                                            </div>
                                        </div>
                                        <button
                                            type="button"
                                            class="btn-secondary btn-compact"
                                            @click="updateForm.coverPk = ''"
                                        >
                                            Clear
                                        </button>
                                    </div>
                                    <div class="form-messages">
                                        <p
                                            v-if="updateState.error"
                                            class="error"
                                        >
                                            ❌ {{ updateState.error }}
                                        </p>
                                    </div>
                                    <button
                                        type="submit"
                                        class="primary"
                                        :disabled="updateState.busy"
                                    >
                                        {{
                                            updateState.busy
                                                ? "Saving..."
                                                : "Apply changes"
                                        }}
                                    </button>
                                </form>
                            </div>

                            <div class="section-subcard">
                                <h4>Add Tracks</h4>
                                <form @submit.prevent="handleAddTracks">
                                    <label class="field">
                                        <span
                                            >Cache PKs (comma or newline
                                            separated)</span
                                        >
                                        <textarea
                                            v-model.trim="addTracksForm.pks"
                                            rows="3"
                                            placeholder="pk1, pk2, L:lazyPk"
                                        ></textarea>
                                    </label>
                                    <div class="field-group">
                                        <label class="field">
                                            <span>Force TTL (s)</span>
                                            <input
                                                v-model="addTracksForm.ttl"
                                                type="number"
                                                min="0"
                                                placeholder="Default"
                                            />
                                        </label>
                                        <label class="checkbox">
                                            <input
                                                type="checkbox"
                                                v-model="addTracksForm.lazy"
                                            />
                                            Insert as lazy references
                                        </label>
                                    </div>
                                    <div class="form-messages">
                                        <p
                                            v-if="addTracksState.error"
                                            class="error"
                                        >
                                            ❌ {{ addTracksState.error }}
                                        </p>
                                    </div>
                                    <button
                                        type="submit"
                                        class="primary"
                                        :disabled="addTracksState.busy"
                                    >
                                        {{
                                            addTracksState.busy
                                                ? "Adding..."
                                                : "Add tracks"
                                        }}
                                    </button>
                                </form>
                            </div>

                            <div class="section-subcard tracks">
                                <div class="section-header">
                                    <h4>Tracks</h4>
                                    <span
                                        v-if="
                                            selectedPlaylist.tracks.length === 0
                                        "
                                        class="small"
                                        >Empty playlist</span
                                    >
                                    <span v-else class="small">
                                        {{ selectedPlaylist.tracks.length }}
                                        entries
                                        <span v-if="lazyTracksCount > 0"
                                            >· {{ lazyTracksCount }} lazy</span
                                        >
                                    </span>
                                </div>
                                <div
                                    class="track-grid"
                                    v-if="sortedTracks.length > 0"
                                >
                                    <article
                                        v-for="track in sortedTracks"
                                        :key="`${track.cache_pk}-${track.added_at}`"
                                        class="track-card"
                                        :class="{ lazy: isLazyTrack(track) }"
                                    >
                                        <div class="cover-wrapper">
                                            <img
                                                v-if="trackCoverUrl(track, 200)"
                                                :src="trackCoverUrl(track, 200)"
                                                :alt="trackTitle(track)"
                                                loading="lazy"
                                            />
                                            <div
                                                v-else
                                                class="cover-placeholder"
                                            >
                                                🎵
                                            </div>
                                            <span
                                                v-if="
                                                    isLazyTrack(track) ||
                                                    trackLazyReference(track)
                                                "
                                                class="cover-lazy-pill"
                                            >
                                                {{
                                                    isLazyTrack(track)
                                                        ? "Lazy entry"
                                                        : "Lazy ref"
                                                }}
                                            </span>
                                        </div>
                                        <div class="track-info">
                                            <div class="track-title">
                                                {{ trackTitle(track) }}
                                            </div>
                                            <div
                                                class="track-artist"
                                                v-if="trackArtist(track)"
                                            >
                                                {{ trackArtist(track) }}
                                            </div>
                                            <div
                                                class="track-album"
                                                v-if="trackAlbum(track)"
                                            >
                                                {{ trackAlbum(track) }}
                                            </div>
                                            <div class="track-meta">
                                                <span
                                                    v-if="
                                                        trackDurationLabel(
                                                            track,
                                                        )
                                                    "
                                                >
                                                    {{
                                                        trackDurationLabel(
                                                            track,
                                                        )
                                                    }}
                                                </span>
                                                <span
                                                    v-if="
                                                        trackSampleRateLabel(
                                                            track,
                                                        )
                                                    "
                                                >
                                                    {{
                                                        trackSampleRateLabel(
                                                            track,
                                                        )
                                                    }}
                                                </span>
                                                <span
                                                    v-if="
                                                        trackBitrateLabel(track)
                                                    "
                                                >
                                                    {{
                                                        trackBitrateLabel(track)
                                                    }}
                                                </span>
                                            </div>
                                            <div class="pk-info">
                                                <div class="pk-line">
                                                    <span class="label"
                                                        >PK</span
                                                    >
                                                    <code>{{
                                                        track.cache_pk
                                                    }}</code>
                                                </div>
                                                <div
                                                    class="pk-line"
                                                    v-if="
                                                        trackLazyReference(
                                                            track,
                                                        )
                                                    "
                                                >
                                                    <span class="label"
                                                        >Lazy</span
                                                    >
                                                    <code>{{
                                                        trackLazyReference(
                                                            track,
                                                        )
                                                    }}</code>
                                                </div>
                                            </div>
                                            <div class="added-info">
                                                Added
                                                {{
                                                    formatRelativeDate(
                                                        track.added_at,
                                                    )
                                                }}
                                                · TTL
                                                {{
                                                    trackTtlLabel(
                                                        track.ttl_secs,
                                                    )
                                                }}
                                            </div>
                                        </div>
                                        <div class="track-actions">
                                            <button
                                                class="btn-secondary"
                                                @click="copyPk(track.cache_pk)"
                                            >
                                                Copy PK
                                            </button>
                                            <button
                                                class="btn-danger"
                                                @click="
                                                    handleRemoveTrack(
                                                        track.cache_pk,
                                                    )
                                                "
                                                :disabled="
                                                    isRemovingTrack(
                                                        track.cache_pk,
                                                    )
                                                "
                                            >
                                                {{
                                                    isRemovingTrack(
                                                        track.cache_pk,
                                                    )
                                                        ? "..."
                                                        : "Remove"
                                                }}
                                            </button>
                                        </div>
                                    </article>
                                </div>
                            </div>
                        </div>
                    </template>
                </div>
            </section>
        </div>
    </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type {
    PlaylistDetail,
    PlaylistSummary,
    PlaylistTrack,
} from "../services/playlists";
import {
    addTracksToPlaylist,
    createPlaylist,
    deletePlaylist,
    flushPlaylist,
    getPlaylistDetail,
    listPlaylists,
    removeTrackFromPlaylist,
    updatePlaylist,
} from "../services/playlists";

import type { AudioCacheMetadata } from "../services/audioCache";
import {
    getCoverUrl as getAudioCoverUrl,
    getDurationMs,
    formatDuration,
    formatBitrate,
    formatSampleRate,
} from "../services/audioCache";

const playlists = ref<PlaylistSummary[]>([]);
const listLoading = ref(false);
const listError = ref("");
const selectedPlaylistId = ref<string | null>(null);
const selectedPlaylist = ref<PlaylistDetail | null>(null);
const detailLoading = ref(false);
const detailError = ref("");

const roleOptions = [
    { value: "user", label: "User" },
    { value: "album", label: "Album" },
    { value: "radio", label: "Radio" },
    { value: "source", label: "Source" },
    { value: "custom", label: "Custom..." },
];

const createForm = reactive({
    id: "",
    title: "",
    role: "user",
    customRole: "",
    coverPk: "",
    persistent: true,
    maxSize: "",
    defaultTtl: "",
});

const createState = reactive({
    busy: false,
    error: "",
    success: "",
});

const updateForm = reactive({
    title: "",
    role: "user",
    customRole: "",
    coverPk: "",
    maxSize: "",
    defaultTtl: "",
});

const updateState = reactive({
    busy: false,
    error: "",
    message: "",
});

const addTracksForm = reactive({
    pks: "",
    ttl: "",
    lazy: false,
});
const addTracksState = reactive({
    busy: false,
    error: "",
    success: "",
});

const flushState = reactive({
    busy: false,
    targetId: "",
});

const deleteState = reactive({
    busy: false,
    targetId: "",
});

const removingTracks = ref(new Set<string>());

const totalTrackCount = computed(() =>
    playlists.value.reduce((sum, item) => sum + item.track_count, 0),
);
const persistentCount = computed(
    () => playlists.value.filter((p) => p.persistent).length,
);

const selectedSummary = computed(() => {
    const detailSummary = selectedPlaylist.value?.summary;
    if (
        detailSummary &&
        (!selectedPlaylistId.value ||
            detailSummary.id === selectedPlaylistId.value)
    ) {
        return detailSummary;
    }

    return (
        playlists.value.find((p) => p.id === selectedPlaylistId.value) ??
        detailSummary ??
        null
    );
});

const detailTitle = computed(() => {
    const summary = selectedSummary.value;
    if (!summary) {
        return "Playlist";
    }
    return summary.title || summary.id || "Playlist";
});

const detailId = computed(() => selectedSummary.value?.id ?? "");

const summaryCoverUrl = computed(() => {
    const summary = selectedSummary.value;
    if (!summary) return undefined;
    if (summary.cover_url && summary.cover_url.length > 0) {
        return summary.cover_url;
    }
    return coverUrlFromPk(summary.cover_pk);
});

const sortedPlaylists = computed(() => {
    return [...playlists.value].sort(
        (a, b) =>
            new Date(b.last_change).getTime() -
            new Date(a.last_change).getTime(),
    );
});

const sortedTracks = computed(() => {
    const detail = selectedPlaylist.value;
    if (!detail) return [];
    return [...detail.tracks].sort(
        (a, b) =>
            new Date(b.added_at).getTime() - new Date(a.added_at).getTime(),
    );
});

const lazyTracksCount = computed(() =>
    selectedPlaylist.value
        ? selectedPlaylist.value.tracks.filter((track) => isLazyTrack(track))
              .length
        : 0,
);

const updateCoverPreview = computed(
    () => coverUrlFromPk(updateForm.coverPk) ?? summaryCoverUrl.value,
);
const createCoverPreview = computed(() => coverUrlFromPk(createForm.coverPk));

function formatRelativeDate(dateString: string) {
    const date = new Date(dateString);
    if (Number.isNaN(date.getTime())) return dateString;
    const diff = Date.now() - date.getTime();
    const minutes = Math.floor(diff / (1000 * 60));
    if (minutes < 1) return "Just now";
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} h ago`;
    const days = Math.floor(hours / 24);
    if (days === 1) return "Yesterday";
    if (days < 7) return `${days} days ago`;
    return date.toLocaleString();
}

function formatRoleLabel(role: string) {
    if (!role) return "Unknown";
    return role.charAt(0).toUpperCase() + role.slice(1);
}

function formatCapacity(capacity?: number | null) {
    if (capacity === null || capacity === undefined) return "∞";
    return capacity.toString();
}

function formatTtl(ttl?: number | null) {
    if (ttl === null || ttl === undefined) return "∞";
    if (ttl < 60) return `${ttl}s`;
    const minutes = Math.round(ttl / 60);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.round(minutes / 60);
    return `${hours}h`;
}

function coverUrlFromPk(pk?: string | null, size = 256): string | undefined {
    if (!pk) return undefined;
    const trimmed = pk.trim();
    if (!trimmed) return undefined;
    return `/covers/image/${trimmed}/${size}`;
}

function trackTtlLabel(ttl?: number | null) {
    if (ttl === null || ttl === undefined) return "inherit";
    if (ttl === 0) return "Expires now";
    if (ttl < 60) return `${ttl}s`;
    return `${Math.round(ttl / 60)}m`;
}

function copyPk(pk: string) {
    navigator.clipboard.writeText(pk);
    alert(`Copied ${pk}`);
}

function isRemovingTrack(pk: string) {
    return removingTracks.value.has(pk);
}

async function refreshPlaylists() {
    listLoading.value = true;
    listError.value = "";
    try {
        const data = await listPlaylists();
        playlists.value = data;
        if (data.length === 0) {
            selectedPlaylistId.value = null;
            selectedPlaylist.value = null;
        } else {
            const first = data[0];
            if (!first) {
                selectedPlaylistId.value = null;
                return;
            }
            if (
                selectedPlaylistId.value &&
                !data.some(
                    (playlist) => playlist.id === selectedPlaylistId.value,
                )
            ) {
                selectedPlaylistId.value = first.id;
            }
            if (!selectedPlaylistId.value) {
                selectedPlaylistId.value = first.id;
            }
        }
    } catch (error) {
        listError.value = (error as Error).message ?? String(error);
    } finally {
        listLoading.value = false;
    }
}

watch(
    () => selectedPlaylistId.value,
    (id, previous) => {
        if (id) {
            if (previous !== id) {
                selectedPlaylist.value = null;
                detailError.value = "";
            }
            loadSelectedPlaylist(previous !== id);
        } else {
            selectedPlaylist.value = null;
        }
    },
);

async function loadSelectedPlaylist(syncForms = true) {
    const id = selectedPlaylistId.value;
    if (!id) return;
    detailLoading.value = true;
    detailError.value = "";
    try {
        const detail = await getPlaylistDetail(id);
        selectedPlaylist.value = detail;
        if (syncForms) {
            syncUpdateForm(detail);
        }
    } catch (error) {
        detailError.value = (error as Error).message ?? String(error);
    } finally {
        detailLoading.value = false;
    }
}

function trackMetadata(track: PlaylistTrack): AudioCacheMetadata | null {
    return track.metadata ?? null;
}

function trackTitle(track: PlaylistTrack): string {
    const metadata = trackMetadata(track);
    return metadata?.title || track.cache_pk;
}

function trackArtist(track: PlaylistTrack): string | undefined {
    const metadata = trackMetadata(track);
    return metadata?.artist || undefined;
}

function trackAlbum(track: PlaylistTrack): string | undefined {
    const metadata = trackMetadata(track);
    return metadata?.album || undefined;
}

function trackDurationLabel(track: PlaylistTrack): string | undefined {
    const duration = getDurationMs(trackMetadata(track));
    if (duration === undefined) return undefined;
    return formatDuration(duration);
}

function trackSampleRateLabel(track: PlaylistTrack): string | undefined {
    const metadata = trackMetadata(track);
    if (!metadata?.sample_rate) return undefined;
    return formatSampleRate(metadata.sample_rate);
}

function trackBitrateLabel(track: PlaylistTrack): string | undefined {
    const metadata = trackMetadata(track);
    if (!metadata?.bitrate) return undefined;
    return formatBitrate(metadata.bitrate);
}

function trackCoverUrl(track: PlaylistTrack, size = 200): string | undefined {
    if (track.cover_url) {
        if (size && track.cover_source === "cover_pk") {
            const [rawBase, query] = track.cover_url.split("?");
            const base = rawBase ?? track.cover_url;
            if (base) {
                const segments = base.split("/");
                const lastIndex = segments.length - 1;
                if (lastIndex >= 0) {
                    const last = segments[lastIndex];
                    if (last && /^\d+$/.test(last)) {
                        segments[lastIndex] = String(size);
                        const rebuilt = segments.join("/");
                        return query ? `${rebuilt}?${query}` : rebuilt;
                    }
                }
            }
        }
        return track.cover_url;
    }

    const metadata = trackMetadata(track);
    if (!metadata) return undefined;
    try {
        return getAudioCoverUrl(metadata, size) || undefined;
    } catch {
        return undefined;
    }
}

function trackLazyReference(track: PlaylistTrack): string | undefined {
    return track.lazy_pk ?? undefined;
}

function isLazyPk(pk: string): boolean {
    return pk.startsWith("L:");
}

function isLazyTrack(track: PlaylistTrack): boolean {
    return isLazyPk(track.cache_pk);
}

function syncUpdateForm(detail: PlaylistDetail | null) {
    if (!detail) return;
    updateForm.title = detail.summary.title || "";
    if (roleOptions.some((opt) => opt.value === detail.summary.role)) {
        updateForm.role = detail.summary.role;
        updateForm.customRole = "";
    } else {
        updateForm.role = "custom";
        updateForm.customRole = detail.summary.role;
    }
    updateForm.coverPk = detail.summary.cover_pk ?? "";
    updateForm.maxSize =
        detail.summary.max_size === null ||
        detail.summary.max_size === undefined
            ? ""
            : String(detail.summary.max_size);
    updateForm.defaultTtl =
        detail.summary.default_ttl_secs === null ||
        detail.summary.default_ttl_secs === undefined
            ? ""
            : String(detail.summary.default_ttl_secs);
}

function resetCreateForm() {
    createForm.id = "";
    createForm.title = "";
    createForm.role = "user";
    createForm.customRole = "";
    createForm.coverPk = "";
    createForm.persistent = true;
    createForm.maxSize = "";
    createForm.defaultTtl = "";
}

function parseOptionalNumber(value: string): number | undefined {
    const trimmed = value.trim();
    if (!trimmed) return undefined;
    const parsed = Number(trimmed);
    if (!Number.isFinite(parsed) || parsed < 0) {
        throw new Error("Value must be a positive number");
    }
    return Math.floor(parsed);
}

function resolveRoleValue(
    roleValue: string,
    customValue: string,
): string | undefined {
    if (roleValue === "custom") {
        const trimmed = customValue.trim();
        return trimmed || undefined;
    }
    return roleValue || undefined;
}

async function handleCreatePlaylist() {
    createState.error = "";
    createState.success = "";
    if (!createForm.id.trim()) {
        createState.error = "ID is required";
        return;
    }
    if (createForm.role === "custom" && !createForm.customRole.trim()) {
        createState.error = "Provide a custom role name";
        return;
    }

    let maxSize: number | undefined;
    let defaultTtl: number | undefined;
    try {
        if (createForm.maxSize.trim()) {
            maxSize = parseOptionalNumber(createForm.maxSize) ?? undefined;
        }
        if (createForm.defaultTtl.trim()) {
            defaultTtl =
                parseOptionalNumber(createForm.defaultTtl) ?? undefined;
        }
    } catch (error) {
        createState.error = (error as Error).message;
        return;
    }

    const roleValue = resolveRoleValue(createForm.role, createForm.customRole);
    const coverPk = createForm.coverPk.trim();

    createState.busy = true;
    try {
        const detail = await createPlaylist({
            id: createForm.id.trim(),
            title: createForm.title || undefined,
            role: roleValue,
            cover_pk: coverPk || undefined,
            persistent: createForm.persistent,
            max_size: maxSize,
            default_ttl_secs: defaultTtl,
        });
        createState.success = `Playlist ${detail.summary.id} created`;
        resetCreateForm();
        await refreshPlaylists();
        selectedPlaylistId.value = detail.summary.id;
        selectedPlaylist.value = detail;
        syncUpdateForm(detail);
    } catch (error) {
        createState.error = (error as Error).message ?? String(error);
    } finally {
        createState.busy = false;
    }
}

async function handleUpdatePlaylist() {
    if (!selectedSummary.value) return;
    updateState.error = "";
    updateState.message = "";
    const summary = selectedSummary.value;

    let maxSizePayload: number | null | undefined;
    let defaultTtlPayload: number | null | undefined;

    try {
        if (updateForm.maxSize.trim() === "") {
            maxSizePayload =
                summary.max_size === null || summary.max_size === undefined
                    ? undefined
                    : null;
        } else {
            const value = parseOptionalNumber(updateForm.maxSize);
            if (value !== summary.max_size) {
                maxSizePayload = value ?? null;
            }
        }

        if (updateForm.defaultTtl.trim() === "") {
            defaultTtlPayload =
                summary.default_ttl_secs === null ||
                summary.default_ttl_secs === undefined
                    ? undefined
                    : null;
        } else {
            const ttlValue = parseOptionalNumber(updateForm.defaultTtl);
            if (ttlValue !== summary.default_ttl_secs) {
                defaultTtlPayload = ttlValue ?? null;
            }
        }
    } catch (error) {
        updateState.error = (error as Error).message;
        return;
    }

    const payload: Record<string, unknown> = {};
    const trimmedTitle = updateForm.title.trim();
    if (trimmedTitle !== summary.title) {
        payload.title = trimmedTitle;
    }

    const resolvedRole = resolveRoleValue(
        updateForm.role,
        updateForm.customRole,
    );
    if (updateForm.role === "custom" && !updateForm.customRole.trim()) {
        updateState.error = "Provide a custom role name";
        return;
    }

    if (resolvedRole && resolvedRole !== summary.role) {
        payload.role = resolvedRole;
    }

    if (maxSizePayload !== undefined) {
        payload.max_size = maxSizePayload;
    }
    if (defaultTtlPayload !== undefined) {
        payload.default_ttl_secs = defaultTtlPayload;
    }

    const trimmedCoverPk = updateForm.coverPk.trim();
    const currentCoverPk = summary.cover_pk ?? "";
    if (trimmedCoverPk === "") {
        if (currentCoverPk !== "") {
            payload.cover_pk = null;
        }
    } else if (trimmedCoverPk !== currentCoverPk) {
        payload.cover_pk = trimmedCoverPk;
    }

    if (Object.keys(payload).length === 0) {
        updateState.message = "No changes to apply";
        return;
    }

    updateState.busy = true;
    try {
        const detail = await updatePlaylist(selectedSummary.value.id, payload);
        selectedPlaylist.value = detail;
        updateState.message = "Playlist updated";
        await refreshPlaylists();
    } catch (error) {
        updateState.error = (error as Error).message ?? String(error);
    } finally {
        updateState.busy = false;
    }
}

function extractPkList(raw: string) {
    return raw
        .split(/[\n,]+/)
        .map((pk) => pk.trim())
        .filter((pk) => pk.length > 0);
}

async function handleAddTracks() {
    if (!selectedSummary.value) return;
    addTracksState.error = "";
    addTracksState.success = "";

    const pkList = extractPkList(addTracksForm.pks);
    if (pkList.length === 0) {
        addTracksState.error = "Provide at least one PK";
        return;
    }

    let ttlValue: number | undefined;
    try {
        ttlValue = parseOptionalNumber(addTracksForm.ttl);
    } catch (error) {
        addTracksState.error = (error as Error).message;
        return;
    }

    addTracksState.busy = true;
    try {
        const detail = await addTracksToPlaylist(selectedSummary.value.id, {
            cache_pks: pkList,
            ttl_secs: ttlValue,
            lazy: addTracksForm.lazy,
        });
        selectedPlaylist.value = detail;
        addTracksState.success = `${pkList.length} track(s) added`;
        addTracksForm.pks = "";
        addTracksForm.ttl = "";
        addTracksForm.lazy = false;
        await refreshPlaylists();
    } catch (error) {
        addTracksState.error = (error as Error).message ?? String(error);
    } finally {
        addTracksState.busy = false;
    }
}

async function handleRemoveTrack(cachePk: string) {
    if (!selectedSummary.value || !cachePk) return;
    removingTracks.value.add(cachePk);
    try {
        const detail = await removeTrackFromPlaylist(
            selectedSummary.value.id,
            cachePk,
        );
        selectedPlaylist.value = detail;
        await refreshPlaylists();
    } catch (error) {
        alert((error as Error).message ?? String(error));
    } finally {
        removingTracks.value.delete(cachePk);
    }
}

async function handleFlushPlaylist(id: string) {
    flushState.busy = true;
    flushState.targetId = id;
    try {
        const detail = await flushPlaylist(id);
        if (selectedPlaylistId.value === id) {
            selectedPlaylist.value = detail;
        }
        await refreshPlaylists();
    } catch (error) {
        alert((error as Error).message ?? String(error));
    } finally {
        flushState.busy = false;
        flushState.targetId = "";
    }
}

async function handleDeletePlaylist(id: string) {
    if (!confirm(`Delete playlist ${id}?`)) return;
    deleteState.busy = true;
    deleteState.targetId = id;
    try {
        await deletePlaylist(id);
        if (selectedPlaylistId.value === id) {
            selectedPlaylistId.value = null;
            selectedPlaylist.value = null;
        }
        await refreshPlaylists();
    } catch (error) {
        alert((error as Error).message ?? String(error));
    } finally {
        deleteState.busy = false;
        deleteState.targetId = "";
    }
}

onMounted(() => {
    refreshPlaylists();
});
</script>

<style scoped>
.playlist-manager {
    padding: 1rem;
    width: 100%;
    max-width: 1400px;
    margin: 0 auto;
    box-sizing: border-box;
    color: #f5f5f5;
}

@media (max-width: 768px) {
    .playlist-manager {
        padding: 0.5rem;
    }
}

.header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem;
    border-radius: 12px;
    border: 1px solid #2b2b2b;
    background: #151515;
}

.header h2 {
    margin: 0;
    color: #61dafb;
}

.subtitle {
    margin: 0.25rem 0 0;
    color: #b0b0b0;
}

.header-stats {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
}

.header-stats span {
    background: #232323;
    border-radius: 999px;
    padding: 0.35rem 0.75rem;
    font-size: 0.85rem;
    color: #ddd;
}

.header-actions button {
    padding: 0.6rem 1.2rem;
    border-radius: 6px;
    border: none;
    background: #61dafb;
    color: #0b0b0b;
    font-weight: 600;
    cursor: pointer;
}

.header-actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
}

.panels {
    display: grid;
    grid-template-columns: 360px 1fr;
    gap: 1.5rem;
    align-items: start;
}

@media (max-width: 1100px) {
    .panels {
        grid-template-columns: 1fr;
    }
}

.section-card {
    background: #1f1f1f;
    border: 1px solid #2f2f2f;
    border-radius: 12px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
}

.section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
}

.empty-state {
    padding: 1rem;
    border: 1px dashed #3a3a3a;
    border-radius: 8px;
    text-align: center;
    color: #a8a8a8;
}

.list-section {
    max-height: calc(100vh - 220px);
    overflow-y: auto;
}

.playlist-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
}

.playlist-card {
    border: 1px solid #2a2a2a;
    border-radius: 10px;
    padding: 0.85rem;
    background: #242424;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    cursor: pointer;
    transition:
        border-color 0.2s,
        box-shadow 0.2s;
}

.playlist-card.selected {
    border-color: #61dafb;
    box-shadow: 0 0 0 1px rgba(97, 218, 251, 0.4);
}

.card-title {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    font-weight: 600;
}

.card-metrics,
.card-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    font-size: 0.85rem;
    color: #bdbdbd;
}

.card-artist {
    margin-top: 0.25rem;
}

.artist-label {
    font-size: 0.85rem;
    color: #90caf9;
    font-style: italic;
}

.card-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
}

.detail-card {
    background: #111;
    border: 1px solid #2b2b2b;
    border-radius: 14px;
    padding: 1.25rem;
    min-height: 400px;
    display: flex;
    flex-direction: column;
    gap: 1rem;
}

.detail-header {
    display: grid;
    grid-template-columns: 140px 1fr auto;
    gap: 1rem;
    align-items: center;
}

.detail-header-main h3 {
    margin: 0;
    display: flex;
    gap: 0.5rem;
    align-items: center;
}

.detail-cover {
    width: 120px;
    height: 120px;
    border-radius: 10px;
    overflow: hidden;
    background: #1a1a1a;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid #2f2f2f;
}

.detail-cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.detail-id {
    margin: 0.2rem 0 0;
    color: #9c9c9c;
    font-size: 0.85rem;
}

.detail-header-actions button {
    background: #232323;
    border: 1px solid #3d3d3d;
    border-radius: 6px;
    padding: 0.5rem 0.9rem;
    color: #f5f5f5;
    cursor: pointer;
}

@media (max-width: 900px) {
    .detail-header {
        grid-template-columns: 100px 1fr;
    }

    .detail-header-actions {
        grid-column: span 2;
        justify-self: flex-start;
    }
}

.cover-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #1b1b1b;
    color: #666;
    font-size: 2rem;
}

.cover-placeholder.small {
    font-size: 1rem;
}

.cover-preview-row {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    flex-wrap: wrap;
}

.cover-preview {
    width: 96px;
    height: 96px;
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid #2d2d2d;
    background: #0f0f0f;
    display: flex;
    align-items: center;
    justify-content: center;
}

.cover-preview img {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.cover-preview.small {
    width: 72px;
    height: 72px;
}

.btn-compact {
    padding: 0.25rem 0.6rem;
    font-size: 0.8rem;
}

.detail-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    font-size: 0.95rem;
}

.cover-stat code {
    background: #0f0f0f;
    padding: 0.15rem 0.4rem;
    border-radius: 4px;
    margin-right: 0.4rem;
}

.detail-messages .error,
.detail-messages .success {
    margin: 0;
}

.section-subcard {
    background: #181818;
    border: 1px solid #2b2b2b;
    border-radius: 10px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
}

.section-subcard h4 {
    margin: 0;
    color: #e7e7e7;
}

form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
}

.field,
.checkbox {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    font-size: 0.9rem;
}

.checkbox {
    flex-direction: row;
    align-items: center;
}

.field input,
.field select,
.field textarea {
    background: #0f0f0f;
    border: 1px solid #3f3f3f;
    border-radius: 6px;
    padding: 0.5rem;
    color: #f5f5f5;
}

.field textarea {
    resize: vertical;
}

.field-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 0.75rem;
}

.field-group {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
}

.form-messages {
    min-height: 1rem;
}

.primary {
    background: #4b9ed0;
    color: #fff;
    border: none;
    padding: 0.6rem 1rem;
    border-radius: 6px;
    cursor: pointer;
    font-weight: 600;
}

.primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

button {
    font-size: 0.9rem;
}

.btn-secondary {
    background: #2c2c2c;
    border: 1px solid #3d3d3d;
    color: #f5f5f5;
}

.btn-danger {
    background: #c0392b;
    border: 1px solid #d35400;
    color: #fff;
}

button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

.pill {
    background: #2f2f2f;
    border-radius: 999px;
    padding: 0.15rem 0.6rem;
    font-size: 0.75rem;
}

.small {
    font-size: 0.85rem;
    color: #a5a5a5;
}

.muted {
    color: #8a8a8a;
}

.tracks .section-header {
    margin-bottom: 0.5rem;
}

.track-grid {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
}

.track-card {
    display: grid;
    grid-template-columns: 120px 1fr auto;
    gap: 1rem;
    padding: 0.9rem;
    border-radius: 12px;
    border: 1px solid #2a2a2a;
    background: #16181f;
    align-items: center;
}

@media (max-width: 900px) {
    .track-card {
        grid-template-columns: 1fr;
    }

    .cover-wrapper {
        width: 100%;
        height: 220px;
    }
}

.track-card.lazy {
    border-color: #d4a017;
    box-shadow: 0 0 0 1px rgba(212, 160, 23, 0.4);
}

.cover-wrapper {
    width: 120px;
    height: 120px;
    border-radius: 10px;
    overflow: hidden;
    background: #1f1f1f;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
}

.cover-wrapper img {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.track-card .cover-placeholder {
    font-size: 2rem;
    color: #6f6f6f;
}

.cover-lazy-pill {
    position: absolute;
    right: 6px;
    bottom: 6px;
    background: #d4a017;
    color: #1a1a1a;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 600;
}

.track-info {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
}

.track-title {
    font-size: 1rem;
    font-weight: 600;
}

.track-artist,
.track-album {
    font-size: 0.9rem;
    color: #bbbbbb;
}

.track-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    font-size: 0.85rem;
    color: #a0a0a0;
}

.pk-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
    word-break: break-word;
}

.pk-line {
    display: flex;
    gap: 0.4rem;
    align-items: center;
}

.pk-line .label {
    font-size: 0.75rem;
    color: #888;
    text-transform: uppercase;
}

.pk-line code {
    background: #0f0f0f;
    padding: 0.2rem 0.4rem;
    border-radius: 4px;
}

.added-info {
    font-size: 0.8rem;
    color: #909090;
}

.track-actions {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

.error {
    color: #ff7a7a;
}

.success {
    color: #6adf8b;
}
</style>
.detail-header-main { display: flex; flex-direction: column; gap: 0.25rem; }
