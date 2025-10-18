<template>
  <div class="audio-cache-manager">
    <div class="header">
      <h2>🎵 Audio Cache Manager</h2>
      <div class="stats">
        <span>{{ tracks.length }} tracks</span>
        <span v-if="totalHits > 0">{{ totalHits }} hits</span>
      </div>
    </div>

    <!-- Formulaire d'ajout -->
    <div class="add-form">
      <h3>➕ Add New Track</h3>
      <form @submit.prevent="handleAddTrack">
        <div class="form-group">
          <input
            v-model="newTrackUrl"
            type="url"
            placeholder="https://example.com/track.flac"
            required
            :disabled="isAdding"
          />
          <input
            v-model="newTrackCollection"
            type="text"
            placeholder="Collection (optional)"
            :disabled="isAdding"
            class="collection-input"
          />
          <button type="submit" :disabled="isAdding || !newTrackUrl">
            {{ isAdding ? "Adding..." : "Add Track" }}
          </button>
        </div>
        <p v-if="addError" class="error">{{ addError }}</p>
        <p v-if="addSuccess" class="success">{{ addSuccess }}</p>
      </form>
    </div>

    <!-- Contrôles -->
    <div class="controls">
      <div class="sort-controls">
        <label>Sort by:</label>
        <select v-model="sortBy">
          <option value="hits">Most Used</option>
          <option value="last_used">Recently Used</option>
          <option value="recent">Recently Added</option>
        </select>
      </div>
      <div class="actions">
        <button @click="refreshTracks" :disabled="isLoading">
          {{ isLoading ? "Loading..." : "Refresh" }}
        </button>
        <button @click="handleConsolidate" :disabled="isConsolidating" class="btn-secondary">
          {{ isConsolidating ? "Consolidating..." : "Consolidate" }}
        </button>
        <button @click="handlePurge" class="btn-danger" :disabled="isPurging">
          {{ isPurging ? "Purging..." : "Purge All" }}
        </button>
      </div>
    </div>

    <!-- Liste des pistes -->
    <div v-if="isLoading && tracks.length === 0" class="loading-state">
      Loading tracks...
    </div>

    <div v-else-if="tracks.length === 0" class="empty-state">
      No tracks in cache. Add one using the form above!
    </div>

    <div v-else class="track-grid">
      <div
        v-for="track in sortedTracks"
        :key="track.pk"
        class="track-card"
        @click="selectedTrack = track"
      >
        <div class="track-icon">
          <div class="music-icon">🎵</div>
          <div class="track-overlay">
            <span class="hits">{{ track.hits }} plays</span>
          </div>
        </div>
        <div class="track-info">
          <div class="track-title">
            {{ track.metadata?.title || "Unknown Title" }}
          </div>
          <div class="track-artist">
            {{ track.metadata?.artist || "Unknown Artist" }}
          </div>
          <div class="track-album" v-if="track.metadata?.album">
            {{ track.metadata.album }}
          </div>
          <div class="pk">{{ track.pk }}</div>
          <div class="meta">
            <span v-if="track.metadata?.duration_ms">
              {{ formatDuration(track.metadata.duration_ms) }}
            </span>
            <span v-if="track.metadata?.sample_rate">
              {{ formatSampleRate(track.metadata.sample_rate) }}
            </span>
            <span v-if="track.metadata?.bitrate">
              {{ formatBitrate(track.metadata.bitrate) }}
            </span>
          </div>
          <div class="collection" v-if="track.collection">
            {{ track.collection }}
          </div>
          <div class="last-used" v-if="track.last_used">
            Last used: {{ formatDate(track.last_used) }}
          </div>
        </div>
        <div class="track-actions">
          <button
            @click.stop="playTrack(track.pk)"
            class="btn-play"
            title="Play"
          >
            ▶️
          </button>
          <button
            @click.stop="handleDeleteTrack(track.pk)"
            class="btn-delete"
            :disabled="deletingTracks.has(track.pk)"
            title="Delete"
          >
            {{ deletingTracks.has(track.pk) ? "..." : "🗑️" }}
          </button>
        </div>
      </div>
    </div>

    <!-- Modal de détails -->
    <div v-if="selectedTrack" class="modal" @click="selectedTrack = null">
      <div class="modal-content" @click.stop>
        <button class="modal-close" @click="selectedTrack = null">✕</button>
        <div class="modal-header">
          <div class="modal-icon">🎵</div>
          <h3>Track Details</h3>
        </div>
        <div class="modal-info">
          <div class="metadata-section" v-if="selectedTrack.metadata">
            <h4>Metadata</h4>
            <p><strong>Title:</strong> {{ selectedTrack.metadata.title || "Unknown" }}</p>
            <p><strong>Artist:</strong> {{ selectedTrack.metadata.artist || "Unknown" }}</p>
            <p v-if="selectedTrack.metadata.album"><strong>Album:</strong> {{ selectedTrack.metadata.album }}</p>
            <p v-if="selectedTrack.metadata.year"><strong>Year:</strong> {{ selectedTrack.metadata.year }}</p>
            <p v-if="selectedTrack.metadata.genre"><strong>Genre:</strong> {{ selectedTrack.metadata.genre }}</p>
            <p v-if="selectedTrack.metadata.track_number"><strong>Track:</strong> {{ selectedTrack.metadata.track_number }}</p>
            <p v-if="selectedTrack.metadata.duration_ms"><strong>Duration:</strong> {{ formatDuration(selectedTrack.metadata.duration_ms) }}</p>
            <p v-if="selectedTrack.metadata.sample_rate"><strong>Sample Rate:</strong> {{ formatSampleRate(selectedTrack.metadata.sample_rate) }}</p>
            <p v-if="selectedTrack.metadata.bitrate"><strong>Bitrate:</strong> {{ formatBitrate(selectedTrack.metadata.bitrate) }}</p>
            <p v-if="selectedTrack.metadata.channels"><strong>Channels:</strong> {{ selectedTrack.metadata.channels }}</p>
          </div>
          <div class="cache-section">
            <h4>Cache Info</h4>
            <p><strong>PK:</strong> {{ selectedTrack.pk }}</p>
            <p><strong>Source URL:</strong> <a :href="selectedTrack.source_url" target="_blank">{{ selectedTrack.source_url }}</a></p>
            <p><strong>Hits:</strong> {{ selectedTrack.hits }}</p>
            <p v-if="selectedTrack.collection"><strong>Collection:</strong> {{ selectedTrack.collection }}</p>
            <p v-if="selectedTrack.last_used"><strong>Last Used:</strong> {{ formatDate(selectedTrack.last_used) }}</p>
          </div>
          <div class="modal-actions">
            <button @click="playTrack(selectedTrack.pk)" class="btn-play">
              ▶️ Play
            </button>
            <button @click="downloadTrack(selectedTrack.pk)" class="btn-secondary">
              ⬇️ Download
            </button>
            <button @click="copyTrackUrl(selectedTrack.pk)" class="btn-secondary">
              📋 Copy URL
            </button>
            <button @click="handleDeleteTrack(selectedTrack.pk); selectedTrack = null" class="btn-danger">
              🗑️ Delete
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Lecteur audio (caché) -->
    <audio ref="audioPlayer" controls style="display: none;"></audio>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import type { AudioCacheEntry } from "../services/audioCache";
import {
  listTracks,
  addTrack,
  deleteTrack,
  purgeCache,
  consolidateCache,
  getTrackUrl,
  getOriginalTrackUrl,
  formatDuration,
  formatBitrate,
  formatSampleRate,
} from "../services/audioCache";

// --- États ---
const tracks = ref<AudioCacheEntry[]>([]);
const selectedTrack = ref<AudioCacheEntry | null>(null);
const isLoading = ref(false);
const sortBy = ref<"hits" | "last_used" | "recent">("hits");
const audioPlayer = ref<HTMLAudioElement | null>(null);

// Formulaire d'ajout
const newTrackUrl = ref("");
const newTrackCollection = ref("");
const isAdding = ref(false);
const addError = ref("");
const addSuccess = ref("");

// Contrôles
const isConsolidating = ref(false);
const isPurging = ref(false);
const deletingTracks = ref(new Set<string>());

// --- Computed ---
const totalHits = computed(() => tracks.value.reduce((sum, t) => sum + t.hits, 0));

const sortedTracks = computed(() => {
  const arr = [...tracks.value];
  switch (sortBy.value) {
    case "hits":
      return arr.sort((a, b) => b.hits - a.hits);
    case "last_used":
      return arr.sort((a, b) => {
        if (!a.last_used) return 1;
        if (!b.last_used) return -1;
        return new Date(b.last_used).getTime() - new Date(a.last_used).getTime();
      });
    case "recent":
      return arr.reverse();
    default:
      return arr;
  }
});

// --- Fonctions ---
async function refreshTracks() {
  isLoading.value = true;
  try {
    tracks.value = await listTracks();
  } finally {
    isLoading.value = false;
  }
}

async function handleAddTrack() {
  if (!newTrackUrl.value) return;
  isAdding.value = true;
  addError.value = "";
  addSuccess.value = "";
  try {
    const result = await addTrack(
      newTrackUrl.value,
      newTrackCollection.value || undefined
    );
    addSuccess.value = `Track added! PK: ${result.pk}`;
    newTrackUrl.value = "";
    newTrackCollection.value = "";
    await refreshTracks();
  } catch (e: any) {
    addError.value = e.message ?? "Failed to add track";
  } finally {
    isAdding.value = false;
    setTimeout(() => (addSuccess.value = ""), 3000);
  }
}

async function handleDeleteTrack(pk: string) {
  if (!confirm(`Delete track ${pk}?`)) return;
  deletingTracks.value.add(pk);
  try {
    await deleteTrack(pk);
    await refreshTracks();
  } finally {
    deletingTracks.value.delete(pk);
  }
}

async function handlePurge() {
  if (!confirm("⚠️ Delete ALL tracks?")) return;
  isPurging.value = true;
  try {
    await purgeCache();
    await refreshTracks();
  } finally {
    isPurging.value = false;
  }
}

async function handleConsolidate() {
  if (!confirm("Consolidate cache? This will re-download missing tracks.")) return;
  isConsolidating.value = true;
  try {
    await consolidateCache();
    await refreshTracks();
  } finally {
    isConsolidating.value = false;
  }
}

function playTrack(pk: string) {
  if (audioPlayer.value) {
    audioPlayer.value.src = getTrackUrl(pk);
    audioPlayer.value.play();
  }
}

function downloadTrack(pk: string) {
  window.open(getOriginalTrackUrl(pk), "_blank");
}

function copyTrackUrl(pk: string) {
  navigator.clipboard.writeText(window.location.origin + getTrackUrl(pk));
  alert("✅ URL copied!");
}

function formatDate(dateString: string) {
  const d = new Date(dateString);
  const diff = Date.now() - d.getTime();
  const days = Math.floor(diff / (1000 * 60 * 60 * 24));
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days} days ago`;
  return d.toLocaleDateString();
}

onMounted(() => refreshTracks());
</script>

<style scoped>
.audio-cache-manager {
  padding: 1rem;
  width: 100%;
  max-width: 100%;
  margin: 0;
  box-sizing: border-box;
}

@media (min-width: 1400px) {
  .audio-cache-manager {
    padding: 2rem;
    max-width: 1400px;
    margin: 0 auto;
  }
}

@media (max-width: 768px) {
  .audio-cache-manager {
    padding: 0.5rem;
  }
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
  padding-bottom: 1rem;
  border-bottom: 2px solid #444;
}

.header h2 {
  margin: 0;
  color: #61dafb;
}

.stats {
  display: flex;
  gap: 1rem;
  font-size: 0.9rem;
  color: #999;
}

/* Formulaire d'ajout */
.add-form {
  background: #2a2a2a;
  padding: 1.5rem;
  border-radius: 8px;
  margin-bottom: 1.5rem;
}

.add-form h3 {
  margin-top: 0;
  color: #61dafb;
}

.form-group {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.form-group input {
  padding: 0.75rem;
  border: 1px solid #444;
  border-radius: 4px;
  background: #1a1a1a;
  color: #fff;
  font-size: 1rem;
}

.form-group input[type="url"] {
  flex: 2;
  min-width: 250px;
}

.collection-input {
  flex: 1;
  min-width: 150px;
}

.form-group button {
  padding: 0.75rem 1.5rem;
  background: #61dafb;
  color: #000;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-weight: bold;
  transition: all 0.2s;
}

.form-group button:hover:not(:disabled) {
  background: #4fa8c5;
}

.form-group button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.error {
  color: #ff6b6b;
  margin-top: 0.5rem;
}

.success {
  color: #51cf66;
  margin-top: 0.5rem;
}

/* Contrôles */
.controls {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
  padding: 1rem;
  background: #2a2a2a;
  border-radius: 8px;
  flex-wrap: wrap;
  gap: 1rem;
}

.sort-controls {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.sort-controls label {
  color: #999;
}

.sort-controls select {
  padding: 0.5rem;
  border: 1px solid #444;
  border-radius: 4px;
  background: #1a1a1a;
  color: #fff;
}

.actions {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}

button {
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
  transition: all 0.2s;
}

button:not(.btn-danger):not(.btn-secondary):not(.btn-play):not(.btn-delete) {
  background: #61dafb;
  color: #000;
}

button:not(.btn-danger):not(.btn-secondary):not(.btn-play):not(.btn-delete):hover:not(:disabled) {
  background: #4fa8c5;
}

.btn-secondary {
  background: #555;
  color: #fff;
}

.btn-secondary:hover:not(:disabled) {
  background: #666;
}

.btn-danger {
  background: #ff6b6b;
  color: #fff;
}

.btn-danger:hover:not(:disabled) {
  background: #ee5a52;
}

.btn-play {
  background: #51cf66;
  color: #fff;
}

.btn-play:hover:not(:disabled) {
  background: #40c057;
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* États */
.loading-state,
.empty-state {
  text-align: center;
  padding: 3rem;
  color: #999;
  font-size: 1.2rem;
}

/* Grille de pistes */
.track-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 1.5rem;
}

.track-card {
  background: #2a2a2a;
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
  display: flex;
  flex-direction: column;
}

.track-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 8px 16px rgba(0, 0, 0, 0.3);
}

.track-icon {
  position: relative;
  width: 100%;
  padding-top: 56.25%; /* Ratio 16:9 */
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  overflow: hidden;
}

.music-icon {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 4rem;
  opacity: 0.3;
}

.track-overlay {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: linear-gradient(to top, rgba(0, 0, 0, 0.8), transparent);
  padding: 0.5rem;
  color: #fff;
}

.hits {
  font-size: 0.9rem;
}

.track-info {
  padding: 1rem;
  flex: 1;
}

.track-title {
  font-size: 1.1rem;
  font-weight: bold;
  color: #61dafb;
  margin-bottom: 0.25rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.track-artist {
  color: #ccc;
  margin-bottom: 0.25rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.track-album {
  color: #999;
  font-size: 0.9rem;
  margin-bottom: 0.5rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pk {
  font-family: monospace;
  color: #777;
  font-size: 0.8rem;
  margin-bottom: 0.5rem;
}

.meta {
  display: flex;
  gap: 0.75rem;
  font-size: 0.85rem;
  color: #999;
  flex-wrap: wrap;
}

.collection {
  color: #888;
  font-size: 0.85rem;
  margin-top: 0.5rem;
  font-style: italic;
}

.last-used {
  color: #777;
  font-size: 0.8rem;
  margin-top: 0.5rem;
}

.track-actions {
  padding: 0 1rem 1rem;
  display: flex;
  gap: 0.5rem;
}

.track-actions button {
  flex: 1;
  padding: 0.5rem;
}

/* Modal */
.modal {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.9);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 2rem;
}

.modal-content {
  background: #2a2a2a;
  border-radius: 12px;
  max-width: 700px;
  max-height: 90vh;
  overflow: auto;
  position: relative;
  width: 100%;
}

.modal-close {
  position: absolute;
  top: 1rem;
  right: 1rem;
  background: rgba(0, 0, 0, 0.5);
  color: #fff;
  border: none;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.2rem;
  z-index: 1;
}

.modal-close:hover {
  background: rgba(0, 0, 0, 0.8);
}

.modal-header {
  padding: 1.5rem;
  border-bottom: 1px solid #444;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.modal-icon {
  font-size: 3rem;
}

.modal-header h3 {
  margin: 0;
  color: #61dafb;
}

.modal-info {
  padding: 1.5rem;
}

.metadata-section,
.cache-section {
  margin-bottom: 1.5rem;
}

.metadata-section h4,
.cache-section h4 {
  color: #61dafb;
  margin-top: 0;
  margin-bottom: 1rem;
}

.modal-info p {
  margin: 0.5rem 0;
  color: #ccc;
}

.modal-info a {
  color: #61dafb;
  text-decoration: none;
  word-break: break-all;
}

.modal-info a:hover {
  text-decoration: underline;
}

.modal-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 1.5rem;
  flex-wrap: wrap;
}

.modal-actions button {
  flex: 1;
  min-width: 120px;
}
</style>
