<template>
  <div class="playlist-manager">
    <div class="header">
      <div>
        <h2>🎚️ Playlist Manager</h2>
        <p class="subtitle">Inspect pmoplaylist state, debug lazy PKs and tune capacities.</p>
      </div>
      <div class="header-stats">
        <span>{{ playlists.length }} playlists</span>
        <span>{{ totalTrackCount }} tracks</span>
        <span v-if="persistentCount > 0">{{ persistentCount }} persistent</span>
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
              <input v-model.trim="createForm.id" type="text" placeholder="unique id" required />
            </label>
            <label class="field">
              <span>Title</span>
              <input v-model.trim="createForm.title" type="text" placeholder="Friendly label" />
            </label>
            <label class="field">
              <span>Role</span>
              <select v-model="createForm.role">
                <option v-for="option in roleOptions" :key="option.value" :value="option.value">
                  {{ option.label }}
                </option>
              </select>
            </label>
            <label v-if="createForm.role === 'custom'" class="field">
              <span>Custom role</span>
              <input
                v-model.trim="createForm.customRole"
                type="text"
                placeholder="ex: qobuz/discover"
              />
            </label>
            <label class="checkbox">
              <input type="checkbox" v-model="createForm.persistent" />
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
              <p v-if="createState.error" class="error">❌ {{ createState.error }}</p>
              <p v-if="createState.success" class="success">✅ {{ createState.success }}</p>
            </div>
            <button type="submit" class="primary" :disabled="createState.busy">
              {{ createState.busy ? "Creating..." : "Create playlist" }}
            </button>
          </form>
        </div>

        <div class="section-card list-section">
          <div class="section-header">
            <h3>📚 Playlists</h3>
            <span v-if="listError" class="error small">❌ {{ listError }}</span>
          </div>
          <div v-if="listLoading && playlists.length === 0" class="empty-state">
            Loading playlists...
          </div>
          <div v-else-if="playlists.length === 0" class="empty-state">
            No playlists yet. Create one above.
          </div>
          <div v-else class="playlist-list">
            <article
              v-for="playlist in sortedPlaylists"
              :key="playlist.id"
              :class="['playlist-card', { selected: playlist.id === selectedPlaylistId }]"
              @click="selectedPlaylistId = playlist.id"
            >
              <div class="card-title">
                <strong>{{ playlist.title || playlist.id }}</strong>
                <small>#{{ playlist.id }}</small>
              </div>
              <div class="card-metrics">
                <span>{{ playlist.track_count }} tracks</span>
                <span>{{ formatRoleLabel(playlist.role) }}</span>
                <span v-if="playlist.persistent" class="pill">Persistent</span>
                <span v-else class="pill transient">Transient</span>
              </div>
              <div class="card-meta">
                <span>Max: {{ formatCapacity(playlist.max_size) }}</span>
                <span>TTL: {{ formatTtl(playlist.default_ttl_secs) }}</span>
              </div>
              <div class="card-meta">
                <span>Updated {{ formatRelativeDate(playlist.last_change) }}</span>
              </div>
              <div class="card-actions">
                <button
                  class="btn-secondary"
                  @click.stop="handleFlushPlaylist(playlist.id)"
                  :disabled="flushState.busy && flushState.targetId === playlist.id"
                >
                  {{ flushState.busy && flushState.targetId === playlist.id ? "Flushing..." : "Flush" }}
                </button>
                <button
                  class="btn-danger"
                  @click.stop="handleDeletePlaylist(playlist.id)"
                  :disabled="deleteState.busy && deleteState.targetId === playlist.id"
                >
                  {{ deleteState.busy && deleteState.targetId === playlist.id ? "Deleting..." : "Delete" }}
                </button>
              </div>
            </article>
          </div>
        </div>
      </section>

      <section class="detail-panel">
        <div class="section-card detail-card">
          <template v-if="!selectedSummary">
            <div class="empty-state">Select a playlist to inspect details.</div>
          </template>
          <template v-else>
            <div class="detail-header">
              <div>
                <h3>
                  {{ selectedSummary?.title || selectedSummary?.id || "Untitled" }}
                  <span class="pill">{{ formatRoleLabel(selectedSummary?.role || "") }}</span>
                </h3>
                <p class="detail-id">#{{ selectedSummary?.id }}</p>
              </div>
              <div class="detail-header-actions">
                <button @click="loadSelectedPlaylist(false)" :disabled="detailLoading">
                  {{ detailLoading ? "Reloading..." : "Reload" }}
                </button>
              </div>
            </div>
            <div class="detail-stats">
              <span><strong>{{ selectedSummary?.track_count ?? 0 }}</strong> tracks</span>
              <span><strong>{{ formatCapacity(selectedSummary?.max_size) }}</strong> capacity</span>
              <span><strong>{{ formatTtl(selectedSummary?.default_ttl_secs) }}</strong> TTL</span>
              <span>
                Last change:
                {{ selectedSummary ? formatRelativeDate(selectedSummary.last_change) : "—" }}
              </span>
            </div>
            <div class="detail-messages">
              <p v-if="detailError" class="error">❌ {{ detailError }}</p>
              <p v-if="updateState.message" class="success">✅ {{ updateState.message }}</p>
              <p v-if="addTracksState.success" class="success">✅ {{ addTracksState.success }}</p>
            </div>

            <div v-if="detailLoading && !selectedPlaylist" class="empty-state">
              Loading playlist details...
            </div>
            <div v-else-if="selectedPlaylist">
              <div class="section-subcard">
                <h4>Metadata & Config</h4>
                <form @submit.prevent="handleUpdatePlaylist">
                  <div class="field-grid">
                    <label class="field">
                      <span>Title</span>
                      <input v-model.trim="updateForm.title" type="text" />
                    </label>
                    <label class="field">
                      <span>Role</span>
                      <select v-model="updateForm.role">
                        <option v-for="option in roleOptions" :key="option.value" :value="option.value">
                          {{ option.label }}
                        </option>
                      </select>
                    </label>
                    <label v-if="updateForm.role === 'custom'" class="field">
                      <span>Custom role</span>
                      <input v-model.trim="updateForm.customRole" type="text" />
                    </label>
                    <label class="field">
                      <span>Max size</span>
                      <input v-model="updateForm.maxSize" type="number" min="0" placeholder="Unlimited" />
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
                  <div class="form-messages">
                    <p v-if="updateState.error" class="error">❌ {{ updateState.error }}</p>
                  </div>
                  <button type="submit" class="primary" :disabled="updateState.busy">
                    {{ updateState.busy ? "Saving..." : "Apply changes" }}
                  </button>
                </form>
              </div>

              <div class="section-subcard">
                <h4>Add Tracks</h4>
                <form @submit.prevent="handleAddTracks">
                  <label class="field">
                    <span>Cache PKs (comma or newline separated)</span>
                    <textarea
                      v-model.trim="addTracksForm.pks"
                      rows="3"
                      placeholder="pk1, pk2, L:lazyPk"
                    ></textarea>
                  </label>
                  <div class="field-group">
                    <label class="field">
                      <span>Force TTL (s)</span>
                      <input v-model="addTracksForm.ttl" type="number" min="0" placeholder="Default" />
                    </label>
                    <label class="checkbox">
                      <input type="checkbox" v-model="addTracksForm.lazy" />
                      Insert as lazy references
                    </label>
                  </div>
                  <div class="form-messages">
                    <p v-if="addTracksState.error" class="error">❌ {{ addTracksState.error }}</p>
                  </div>
                  <button type="submit" class="primary" :disabled="addTracksState.busy">
                    {{ addTracksState.busy ? "Adding..." : "Add tracks" }}
                  </button>
                </form>
              </div>

              <div class="section-subcard tracks">
                <div class="section-header">
                  <h4>Tracks</h4>
                  <span v-if="selectedPlaylist.tracks.length === 0" class="small">Empty playlist</span>
                  <span v-else class="small">
                    {{ selectedPlaylist.tracks.length }} entries
                    <span v-if="lazyTracksCount > 0">· {{ lazyTracksCount }} lazy</span>
                  </span>
                </div>
                <div class="track-table" v-if="selectedPlaylist.tracks.length > 0">
                  <div class="track-row track-header">
                    <span>PK</span>
                    <span>Added</span>
                    <span>TTL</span>
                    <span>Actions</span>
                  </div>
                  <div
                    v-for="track in sortedTracks"
                    :key="track.cache_pk"
                    class="track-row"
                    :class="{ lazy: isLazyTrack(track.cache_pk) }"
                  >
                    <span class="pk">
                      {{ track.cache_pk }}
                      <span v-if="isLazyTrack(track.cache_pk)" class="pill warning">Lazy</span>
                    </span>
                    <span>{{ formatRelativeDate(track.added_at) }}</span>
                    <span>{{ trackTtlLabel(track.ttl_secs) }}</span>
                    <span class="row-actions">
                      <button class="btn-secondary" @click="copyPk(track.cache_pk)">Copy</button>
                      <button
                        class="btn-danger"
                        @click="handleRemoveTrack(track.cache_pk)"
                        :disabled="isRemovingTrack(track.cache_pk)"
                      >
                        {{ isRemovingTrack(track.cache_pk) ? "..." : "Remove" }}
                      </button>
                    </span>
                  </div>
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
import type { PlaylistDetail, PlaylistSummary } from "../services/playlists";
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
  playlists.value.reduce((sum, item) => sum + item.track_count, 0)
);
const persistentCount = computed(() => playlists.value.filter((p) => p.persistent).length);

const selectedSummary = computed(() => {
  const detailSummary = selectedPlaylist.value?.summary;
  if (
    detailSummary &&
    (!selectedPlaylistId.value || detailSummary.id === selectedPlaylistId.value)
  ) {
    return detailSummary;
  }

  return playlists.value.find((p) => p.id === selectedPlaylistId.value) ?? detailSummary ?? null;
});

const sortedPlaylists = computed(() => {
  return [...playlists.value].sort(
    (a, b) => new Date(b.last_change).getTime() - new Date(a.last_change).getTime()
  );
});

const sortedTracks = computed(() => {
  const detail = selectedPlaylist.value;
  if (!detail) return [];
  return [...detail.tracks].sort(
    (a, b) => new Date(b.added_at).getTime() - new Date(a.added_at).getTime()
  );
});

const lazyTracksCount = computed(() =>
  selectedPlaylist.value
    ? selectedPlaylist.value.tracks.filter((track) => isLazyTrack(track.cache_pk)).length
    : 0
);

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

function trackTtlLabel(ttl?: number | null) {
  if (ttl === null || ttl === undefined) return "inherit";
  if (ttl === 0) return "Expires now";
  if (ttl < 60) return `${ttl}s`;
  return `${Math.round(ttl / 60)}m`;
}

function isLazyTrack(pk: string) {
  return pk.startsWith("L:");
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
        !data.some((playlist) => playlist.id === selectedPlaylistId.value)
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
      loadSelectedPlaylist(previous !== id);
    } else {
      selectedPlaylist.value = null;
    }
  }
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
  updateForm.maxSize =
    detail.summary.max_size === null || detail.summary.max_size === undefined
      ? ""
      : String(detail.summary.max_size);
  updateForm.defaultTtl =
    detail.summary.default_ttl_secs === null || detail.summary.default_ttl_secs === undefined
      ? ""
      : String(detail.summary.default_ttl_secs);
}

function resetCreateForm() {
  createForm.id = "";
  createForm.title = "";
  createForm.role = "user";
  createForm.customRole = "";
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

function resolveRoleValue(roleValue: string, customValue: string): string | undefined {
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
      defaultTtl = parseOptionalNumber(createForm.defaultTtl) ?? undefined;
    }
  } catch (error) {
    createState.error = (error as Error).message;
    return;
  }

  const roleValue = resolveRoleValue(createForm.role, createForm.customRole);

  createState.busy = true;
  try {
    const detail = await createPlaylist({
      id: createForm.id.trim(),
      title: createForm.title || undefined,
      role: roleValue,
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

  let maxSizePayload: number | null | undefined;
  let defaultTtlPayload: number | null | undefined;

  try {
    if (updateForm.maxSize.trim() === "") {
      maxSizePayload =
        selectedSummary.value.max_size === null || selectedSummary.value.max_size === undefined
          ? undefined
          : null;
    } else {
      const value = parseOptionalNumber(updateForm.maxSize);
      if (value !== selectedSummary.value.max_size) {
        maxSizePayload = value ?? null;
      }
    }

    if (updateForm.defaultTtl.trim() === "") {
      defaultTtlPayload =
        selectedSummary.value.default_ttl_secs === null ||
        selectedSummary.value.default_ttl_secs === undefined
          ? undefined
          : null;
    } else {
      const ttlValue = parseOptionalNumber(updateForm.defaultTtl);
      if (ttlValue !== selectedSummary.value.default_ttl_secs) {
        defaultTtlPayload = ttlValue ?? null;
      }
    }
  } catch (error) {
    updateState.error = (error as Error).message;
    return;
  }

  const payload: Record<string, unknown> = {};
  const trimmedTitle = updateForm.title.trim();
  if (trimmedTitle !== selectedSummary.value.title) {
    payload.title = trimmedTitle;
  }

  const resolvedRole = resolveRoleValue(updateForm.role, updateForm.customRole);
  if (updateForm.role === "custom" && !updateForm.customRole.trim()) {
    updateState.error = "Provide a custom role name";
    return;
  }

  if (resolvedRole && resolvedRole !== selectedSummary.value.role) {
    payload.role = resolvedRole;
  }

  if (maxSizePayload !== undefined) {
    payload.max_size = maxSizePayload;
  }
  if (defaultTtlPayload !== undefined) {
    payload.default_ttl_secs = defaultTtlPayload;
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
    const detail = await removeTrackFromPlaylist(selectedSummary.value.id, cachePk);
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
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  padding: 1rem;
  color: #f3f3f3;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 1rem;
  border-bottom: 1px solid #333;
  padding-bottom: 0.5rem;
}

.subtitle {
  color: #888;
  margin: 0.25rem 0 0;
}

.header-stats {
  display: flex;
  gap: 0.75rem;
  font-weight: 500;
}

.header-actions button {
  padding: 0.5rem 1rem;
}

.panels {
  display: grid;
  grid-template-columns: 340px 1fr;
  gap: 1.5rem;
}

.section-card {
  background: #232323;
  border: 1px solid #3a3a3a;
  border-radius: 10px;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  color: inherit;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.empty-state {
  padding: 1rem;
  text-align: center;
  color: #999;
  border: 1px dashed #444;
  border-radius: 6px;
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
  border: 1px solid #333;
  border-radius: 8px;
  padding: 0.75rem;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  background: #2b2b2b;
  color: inherit;
}

.playlist-card.selected {
  border-color: #569cd6;
  box-shadow: 0 0 0 1px rgba(86, 156, 214, 0.4);
}

.card-title {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
}

.card-metrics,
.card-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  font-size: 0.9rem;
  color: #bbb;
}

.card-actions {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.detail-panel {
  min-height: 400px;
}

.detail-card {
  min-height: 400px;
}

.detail-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 0.75rem;
}

.detail-id {
  color: #aaa;
  margin: 0;
}

.detail-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem;
  font-size: 0.95rem;
}

.detail-messages .error,
.detail-messages .success {
  margin: 0;
}

.section-subcard {
  border: 1px solid #333;
  border-radius: 8px;
  padding: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  background: #1a1a1a;
}

.section-subcard h4 {
  margin: 0;
}

form {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  font-size: 0.9rem;
}

.field input,
.field select,
.field textarea {
  background: #111;
  border: 1px solid #3f3f3f;
  border-radius: 6px;
  padding: 0.5rem;
  color: #f6f6f6;
}

.field textarea {
  resize: vertical;
}

.field-group {
  display: flex;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 0.75rem;
}

.checkbox {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.9rem;
}

.form-messages {
  min-height: 1rem;
}

.form-messages .error,
.form-messages .success {
  margin: 0;
}

.primary {
  background: #569cd6;
  color: #fff;
  border: none;
  padding: 0.5rem 1rem;
  border-radius: 6px;
  cursor: pointer;
}

.primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

button {
  background: #333;
  color: #fff;
  border: 1px solid transparent;
  padding: 0.4rem 0.75rem;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.9rem;
}

.btn-secondary {
  background: #2c2c2c;
  border-color: #444;
}

.btn-danger {
  background: #a33;
  border-color: #c55;
}

button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.pill {
  background: #444;
  color: #eee;
  border-radius: 999px;
  padding: 0.1rem 0.5rem;
  font-size: 0.75rem;
}

.pill.transient {
  background: #5b3d99;
}

.pill.warning {
  background: #b78000;
}

.detail-header-actions {
  display: flex;
  gap: 0.5rem;
}

.tracks .track-table {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.track-row {
  display: grid;
  grid-template-columns: 1fr 140px 80px 160px;
  gap: 0.5rem;
  align-items: center;
  padding: 0.4rem;
  border-radius: 6px;
  background: #121212;
  border: 1px solid #2f2f2f;
  font-size: 0.9rem;
  color: inherit;
}

.track-row.lazy {
  border-color: #b78000;
  background: rgba(183, 128, 0, 0.15);
}

.track-header {
  font-weight: 600;
  background: transparent;
  border: none;
}

.row-actions {
  display: flex;
  gap: 0.5rem;
  justify-content: flex-end;
}

.pk {
  word-break: break-all;
}

.error {
  color: #ff8a8a;
}

.success {
  color: #7ed07e;
}

.small {
  font-size: 0.85rem;
  color: #aaa;
}

@media (max-width: 1100px) {
  .panels {
    grid-template-columns: 1fr;
  }

  .list-section {
    max-height: none;
  }
}
</style>
