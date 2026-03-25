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
    padding: var(--spacing-md);
    width: 100%;
    max-width: 1400px;
    margin: 0 auto;
    box-sizing: border-box;
    color: var(--color-text);
}

@media (max-width: 768px) {
    .playlist-manager {
        padding: var(--spacing-sm);
    }
}

.header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-md);
    border-radius: var(--radius-lg);
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.05);
    margin-bottom: var(--spacing-md);
}

.header h2 {
    margin: 0;
    color: var(--color-primary);
    font-size: var(--text-xl);
}

.subtitle {
    margin: 0.25rem 0 0;
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
}

.header-stats {
    display: flex;
    gap: var(--spacing-sm);
    flex-wrap: wrap;
}

.header-stats span {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius-full);
    padding: 0.25rem 0.75rem;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
}

.header-actions button {
    padding: 0.5rem 1rem;
    border-radius: var(--radius-md);
    border: none;
    background: var(--color-primary);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
    font-size: var(--text-sm);
    transition: opacity var(--transition-fast);
}

.header-actions button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

.panels {
    display: grid;
    grid-template-columns: 340px 1fr;
    gap: var(--spacing-md);
    align-items: start;
}

@media (max-width: 1100px) {
    .panels {
        grid-template-columns: 1fr;
    }
}

.section-card {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg);
    padding: var(--spacing-md);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
}

.section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--spacing-sm);
}

.empty-state {
    padding: var(--spacing-md);
    border: 1px dashed rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-md);
    text-align: center;
    color: var(--color-text-secondary);
}

.list-section {
    max-height: calc(100vh - 220px);
    overflow-y: auto;
}

.playlist-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
}

.playlist-card {
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-md);
    padding: var(--spacing-sm) var(--spacing-md);
    background: rgba(255, 255, 255, 0.05);
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    cursor: pointer;
    transition: border-color var(--transition-fast), background var(--transition-fast);
}

.playlist-card:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.2);
}

.playlist-card.selected {
    border-color: var(--color-primary);
    background: rgba(102, 126, 234, 0.1);
}

.card-title {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-sm);
    font-weight: 600;
    font-size: var(--text-sm);
}

.card-metrics,
.card-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-sm);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
}

.card-artist {
    margin-top: 0.15rem;
}

.artist-label {
    font-size: var(--text-xs);
    color: var(--color-primary);
    font-style: italic;
}

.card-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-sm);
    margin-top: 0.25rem;
}

.detail-card {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg);
    padding: var(--spacing-md);
    min-height: 400px;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
}

.detail-header {
    display: grid;
    grid-template-columns: 100px 1fr auto;
    gap: var(--spacing-md);
    align-items: center;
}

.detail-header-main {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
}

.detail-header-main h3 {
    margin: 0;
    display: flex;
    gap: var(--spacing-sm);
    align-items: center;
    font-size: var(--text-lg);
}

.detail-cover {
    width: 100px;
    height: 100px;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: rgba(255, 255, 255, 0.06);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.1);
}

.detail-cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.detail-id {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
}

.detail-header-actions button {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-md);
    padding: 0.4rem 0.8rem;
    color: var(--color-text);
    cursor: pointer;
    font-size: var(--text-sm);
    transition: background var(--transition-fast);
}

.detail-header-actions button:hover {
    background: rgba(255, 255, 255, 0.14);
}

@media (max-width: 900px) {
    .detail-header {
        grid-template-columns: 80px 1fr;
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
    color: var(--color-text-secondary);
    font-size: 1.75rem;
}

.cover-placeholder.small {
    font-size: 1rem;
}

.cover-preview-row {
    display: flex;
    gap: var(--spacing-sm);
    align-items: center;
    flex-wrap: wrap;
}

.cover-preview {
    width: 80px;
    height: 80px;
    border-radius: var(--radius-md);
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.05);
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
    width: 60px;
    height: 60px;
}

.btn-compact {
    padding: 0.2rem 0.5rem;
    font-size: var(--text-xs);
}

.detail-stats {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-md);
    font-size: var(--text-sm);
}

.cover-stat code {
    background: rgba(255, 255, 255, 0.08);
    padding: 0.15rem 0.4rem;
    border-radius: var(--radius-sm);
    margin-right: 0.4rem;
    font-size: var(--text-xs);
}

.detail-messages .error,
.detail-messages .success {
    margin: 0;
}

.section-subcard {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-md);
    padding: var(--spacing-md);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
}

.section-subcard h4 {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: 600;
}

form {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
}

.field,
.checkbox {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: var(--text-sm);
}

.checkbox {
    flex-direction: row;
    align-items: center;
}

.field input,
.field select,
.field textarea {
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-md);
    padding: 0.45rem var(--spacing-sm);
    color: var(--color-text);
    font-size: var(--text-sm);
}

.field textarea {
    resize: vertical;
}

.field-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: var(--spacing-sm);
}

.field-group {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-sm);
}

.form-messages {
    min-height: 1rem;
}

.primary {
    background: var(--color-primary);
    color: #fff;
    border: none;
    padding: 0.5rem var(--spacing-md);
    border-radius: var(--radius-md);
    cursor: pointer;
    font-weight: 600;
    font-size: var(--text-sm);
    transition: opacity var(--transition-fast);
}

.primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

button {
    font-size: var(--text-sm);
}

.btn-secondary {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    color: var(--color-text);
    border-radius: var(--radius-md);
    padding: 0.4rem 0.8rem;
    cursor: pointer;
    transition: background var(--transition-fast);
}

.btn-secondary:hover {
    background: rgba(255, 255, 255, 0.14);
}

.btn-danger {
    background: rgba(192, 57, 43, 0.8);
    border: 1px solid rgba(211, 84, 0, 0.6);
    color: #fff;
    border-radius: var(--radius-md);
    padding: 0.4rem 0.8rem;
    cursor: pointer;
    transition: background var(--transition-fast);
}

.btn-danger:hover {
    background: rgba(192, 57, 43, 1);
}

button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

.pill {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-full);
    padding: 0.1rem 0.55rem;
    font-size: var(--text-xs);
}

.small {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
}

.muted {
    color: var(--color-text-secondary);
}

.tracks .section-header {
    margin-bottom: var(--spacing-sm);
}

.track-grid {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
}

.track-card {
    display: grid;
    grid-template-columns: 80px 1fr auto;
    gap: var(--spacing-md);
    padding: var(--spacing-sm) var(--spacing-md);
    border-radius: var(--radius-md);
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.04);
    align-items: center;
    transition: background var(--transition-fast);
}

.track-card:hover {
    background: rgba(255, 255, 255, 0.07);
}

@media (max-width: 900px) {
    .track-card {
        grid-template-columns: 80px 1fr;
    }

    .track-actions {
        grid-column: span 2;
        flex-direction: row;
    }
}

.track-card.lazy {
    border-color: rgba(212, 160, 23, 0.5);
    background: rgba(212, 160, 23, 0.05);
}

.cover-wrapper {
    width: 80px;
    height: 80px;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: rgba(255, 255, 255, 0.06);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    flex-shrink: 0;
}

.cover-wrapper img {
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.track-card .cover-placeholder {
    font-size: 1.5rem;
    color: var(--color-text-secondary);
}

.cover-lazy-pill {
    position: absolute;
    right: 4px;
    bottom: 4px;
    background: #d4a017;
    color: #1a1a1a;
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-full);
    font-size: 0.65rem;
    font-weight: 600;
}

.track-info {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
}

.track-title {
    font-size: var(--text-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.track-artist,
.track-album {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.track-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-sm);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    margin-top: 0.15rem;
}

.pk-info {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: var(--text-xs);
    word-break: break-all;
    color: var(--color-text-secondary);
    margin-top: 0.25rem;
}

.pk-line {
    display: flex;
    gap: 0.3rem;
    align-items: center;
}

.pk-line .label {
    font-size: 0.7rem;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    flex-shrink: 0;
}

.pk-line code {
    background: rgba(0, 0, 0, 0.3);
    padding: 0.1rem 0.35rem;
    border-radius: var(--radius-sm);
    font-size: 0.7rem;
}

.added-info {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
}

.track-actions {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
}

.error {
    color: var(--status-offline);
}

.success {
    color: var(--status-playing);
}
</style>
