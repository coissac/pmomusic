<template>
  <div class="audio-cache-manager">
    <div class="header">
      <h2>🎵 Audio Cache Manager</h2>
      <div class="stats">
        <span>{{ tracks.length }} tracks</span>
        <span v-if="lazyTracksCount > 0">{{ lazyTracksCount }} lazy pending</span>
        <span v-if="totalHits > 0">{{ totalHits }} hits</span>
      </div>
    </div>

    <!-- Formulaire d'ajout -->
    <div class="add-form">
      <h3>➕ Add New Track</h3>
      <form @submit.prevent="handleAddTrack">
        <div class="source-toggle">
          <label>
            <input type="radio" value="url" v-model="newTrackSourceType" /> Remote URL
          </label>
          <label>
            <input type="radio" value="path" v-model="newTrackSourceType" /> Local FLAC reference
          </label>
        </div>
        <div class="form-group">
          <template v-if="newTrackSourceType === 'url'">
            <input
              v-model="newTrackUrl"
              type="url"
              placeholder="https://example.com/track.flac"
              :required="newTrackSourceType === 'url'"
              :disabled="isAdding"
            />
          </template>
          <template v-else>
            <input
              v-model="newTrackPath"
              type="text"
              placeholder="/mnt/music/MyTrack.flac"
              :required="newTrackSourceType === 'path'"
              :disabled="isAdding"
            />
          </template>
        </div>
        <p class="local-tip" v-if="newTrackSourceType === 'path'">
          Local FLAC files are referenced without duplication. Removing the cache entry never deletes
          the original file.
        </p>
        <div class="form-group">
          <input
            v-model="newTrackCollection"
            type="text"
            placeholder="Collection (optional)"
            :disabled="isAdding"
            class="collection-input"
          />
          <button type="submit" :disabled="addButtonDisabled">
            {{ addButtonLabel }}
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
        :class="['track-card', { 'local-reference': isLocalFile(track) }]"
        @click="selectedTrack = track"
      >
        <div class="track-icon">
          <img
            v-if="getTrackCoverUrl(track, 400)"
            :src="getTrackCoverUrl(track, 400)"
            :alt="`Cover for ${track.metadata?.title || 'Unknown'}`"
            class="cover-image"
            @error="handleCoverError(track.pk)"
          />
          <div v-else class="music-icon">🎵</div>
          <div class="track-overlay">
            <span class="hits" v-if="!isLazyTrack(track)">{{ track.hits }} plays</span>
            <span
              v-else
              class="hits lazy"
              :class="lazyProviderClass(track)"
            >
              {{ lazyBadgeLabel(track) }}
            </span>
            <span v-if="isLocalFile(track)" class="local-pill">Local file</span>
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
          <div class="pk">
            {{ track.pk }}
            <span
              v-if="isLazyTrack(track)"
              class="lazy-tag"
              :class="lazyProviderClass(track)"
            >
              <span class="lazy-label">Lazy</span>
              <span class="lazy-provider-name" v-if="lazyProviderName(track)">
                {{ lazyProviderName(track) }}
              </span>
            </span>
          </div>
          <div class="meta">
            <span v-if="durationMs(track) !== undefined">
              {{ formatDuration(durationMs(track)!) }}
            </span>
            <span v-if="track.metadata?.sample_rate">
              {{ formatSampleRate(track.metadata.sample_rate) }}
            </span>
            <span v-if="track.metadata?.bitrate">
              {{ formatBitrate(track.metadata.bitrate) }}
            </span>
            <span v-if="conversionLabel(track)">
              {{ conversionLabel(track) }}
            </span>
          </div>
          <div class="local-path" v-if="isLocalFile(track)">
            <span class="local-badge">Local</span>
            <span class="local-path-text">
              {{ localSourcePath(track) || "Original file" }}
            </span>
          </div>
          <div class="collection" v-if="track.collection">
            {{ track.collection }}
          </div>
          <div class="last-used" v-if="track.last_used">
            Last used: {{ formatDate(track.last_used) }}
          </div>
          <div class="lazy-warning" v-if="isLazyTrack(track)">
            Audio not downloaded yet
            <span v-if="lazyProviderName(track)">({{ lazyProviderName(track) }} provider)</span>.
            First playback (or forcing download) will fetch it automatically.
          </div>
        </div>
        <div class="track-actions">
          <button
            @click.stop="playTrack(track.pk)"
            class="btn-play"
            :title="isLazyTrack(track) ? 'Trigger download & play once available' : 'Play'"
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
          <button
            @click.stop="downloadTrack(track.pk)"
            class="btn-secondary"
            :disabled="isLazyTrack(track)"
            :title="
              isLazyTrack(track)
                ? 'Download available once audio finished downloading'
                : 'Download original file'
            "
          >
            ⬇️
          </button>
          <button
            @click.stop="copyTrackUrl(track.pk)"
            class="btn-secondary"
            :disabled="isLazyTrack(track)"
            :title="
              isLazyTrack(track)
                ? 'URL available after download completes'
                : 'Copy stream URL'
            "
          >
            📋
          </button>
        </div>
      </div>
    </div>

    <!-- Modal de détails -->
    <div v-if="selectedTrack" class="modal" @click="selectedTrack = null">
      <div class="modal-content" @click.stop>
        <button class="modal-close" @click="selectedTrack = null">✕</button>
        <div class="modal-header">
          <img
            v-if="getTrackCoverUrl(selectedTrack, 200)"
            :src="getTrackCoverUrl(selectedTrack, 200)"
            :alt="`Cover for ${selectedTrack.metadata?.title || 'Unknown'}`"
            class="modal-cover-image"
            @error="handleCoverError(selectedTrack.pk)"
          />
          <div v-else class="modal-icon">🎵</div>
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
            <p v-if="durationMs(selectedTrack) !== undefined">
              <strong>Duration:</strong> {{ formatDuration(durationMs(selectedTrack)!) }}
            </p>
            <p v-if="selectedTrack.metadata.sample_rate"><strong>Sample Rate:</strong> {{ formatSampleRate(selectedTrack.metadata.sample_rate) }}</p>
            <p v-if="selectedTrack.metadata.bitrate"><strong>Bitrate:</strong> {{ formatBitrate(selectedTrack.metadata.bitrate) }}</p>
            <p v-if="selectedTrack.metadata.channels"><strong>Channels:</strong> {{ selectedTrack.metadata.channels }}</p>
            <p v-if="conversionLabel(selectedTrack)"><strong>Conversion:</strong> {{ conversionLabel(selectedTrack) }}</p>
          </div>
          <div class="cache-section">
            <h4>Cache Info</h4>
            <p><strong>PK:</strong> {{ selectedTrack.pk }}</p>
            <p><strong>Status:</strong> {{ trackStatusLabel(selectedTrack) }}</p>
            <p v-if="isLocalFile(selectedTrack)">
              <strong>Local file:</strong>
              <span class="local-path-text">
                {{ localSourcePath(selectedTrack) || "Original file retained" }}
              </span>
            </p>
            <p v-if="resolveTrackOrigin(selectedTrack)">
              <strong>Source URL:</strong>
              <a :href="resolveTrackOrigin(selectedTrack)" target="_blank">{{ resolveTrackOrigin(selectedTrack) }}</a>
            </p>
            <p v-else><strong>Source URL:</strong> Unknown</p>
            <p><strong>Hits:</strong> {{ selectedTrack.hits }}</p>
            <p v-if="selectedTrack.collection"><strong>Collection:</strong> {{ selectedTrack.collection }}</p>
            <p v-if="selectedTrack.last_used"><strong>Last Used:</strong> {{ formatDate(selectedTrack.last_used) }}</p>
          </div>
          <div class="modal-actions">
            <button @click="playTrack(selectedTrack.pk)" class="btn-play">
              ▶️ Play
            </button>
            <button
              @click="downloadTrack(selectedTrack.pk)"
              class="btn-secondary"
              :disabled="isLazyTrack(selectedTrack)"
            >
              ⬇️ Download
            </button>
            <button
              @click="copyTrackUrl(selectedTrack.pk)"
              class="btn-secondary"
              :disabled="isLazyTrack(selectedTrack)"
            >
              📋 Copy URL
            </button>
            <button @click="handleDeleteTrack(selectedTrack.pk); selectedTrack = null" class="btn-danger">
              🗑️ Delete
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Lecteur audio -->
    <div v-if="isPlaying || audioError" class="audio-player-container">
      <audio
        ref="audioPlayer"
        controls
        v-if="!audioError"
        @ended="handleAudioEnded"
        @error="handleAudioError"
      ></audio>
      <p v-if="audioError" class="audio-error">{{ audioError }}</p>
      <button @click="stopTrack" class="btn-stop" title="Stop">
        {{ audioError ? '✕ Close' : '⏹️ Stop' }}
      </button>
    </div>
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
  getOriginUrl,
  getDurationMs,
  formatDuration,
  formatBitrate,
  formatSampleRate,
  getCoverUrl,
} from "../services/audioCache";

interface LazyDisplayInfo {
  prefix: string;
  display: string;
  className: string;
  isLegacy: boolean;
}

const LAZY_PROVIDER_LABELS: Record<string, string> = {
  QOBUZ: "Qobuz",
};
const LEGACY_LAZY_INFO: LazyDisplayInfo = {
  prefix: "legacy",
  display: "Legacy",
  className: "lazy-provider-legacy",
  isLegacy: true,
};

// --- États ---
const tracks = ref<AudioCacheEntry[]>([]);
const selectedTrack = ref<AudioCacheEntry | null>(null);
const isLoading = ref(false);
const sortBy = ref<"hits" | "last_used" | "recent">("hits");
const audioPlayer = ref<HTMLAudioElement | null>(null);
const LEGACY_LAZY_PREFIX = "L:";

// Formulaire d'ajout
const newTrackUrl = ref("");
const newTrackPath = ref("");
const newTrackCollection = ref("");
const newTrackSourceType = ref<"url" | "path">("url");
const isAdding = ref(false);
const addError = ref("");
const addSuccess = ref("");

// Contrôles
const isConsolidating = ref(false);
const isPurging = ref(false);
const deletingTracks = ref(new Set<string>());

// Lecteur audio
const isPlaying = ref(false);
const audioError = ref("");

// Gestion des erreurs de chargement des covers
const failedCovers = ref(new Set<string>());

// --- Computed ---
const totalHits = computed(() => tracks.value.reduce((sum, t) => sum + t.hits, 0));
const lazyTracksCount = computed(() =>
  tracks.value.reduce((acc, track) => (isLazyTrack(track) ? acc + 1 : acc), 0)
);

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

const addButtonDisabled = computed(() => {
  if (isAdding.value) return true;
  const value =
    newTrackSourceType.value === "url"
      ? newTrackUrl.value?.trim()
      : newTrackPath.value?.trim();
  return !value;
});

const addButtonLabel = computed(() => {
  if (newTrackSourceType.value === "url") {
    return isAdding.value ? "Adding..." : "Add Track";
  }
  return isAdding.value ? "Linking..." : "Add Local File";
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
  const useUrl = newTrackSourceType.value === "url";
  const rawValue = useUrl ? newTrackUrl.value.trim() : newTrackPath.value.trim();
  if (!rawValue) {
    addError.value = useUrl ? "URL is required" : "Local path is required";
    return;
  }
  isAdding.value = true;
  addError.value = "";
  addSuccess.value = "";
  try {
    const result = await addTrack({
      url: useUrl ? rawValue : undefined,
      path: useUrl ? undefined : rawValue,
      collection: newTrackCollection.value || undefined,
    });
    addSuccess.value =
      newTrackSourceType.value === "path"
        ? `Local file linked! PK: ${result.pk}`
        : `Track added! PK: ${result.pk}`;
    newTrackUrl.value = "";
    newTrackPath.value = "";
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
  const track = getTrackByPk(pk);
  let confirmMessage = `Delete track ${pk}?`;
  if (track && isLocalFile(track)) {
    const path = localSourcePath(track);
    confirmMessage =
      `Remove cached reference for local file?\nPK: ${pk}` +
      (path ? `\nSource: ${path}` : "") +
      "\nOriginal file will remain untouched.";
  }
  if (!confirm(confirmMessage)) return;
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
  audioError.value = "";
  isPlaying.value = true;

  // Attendre que le DOM soit mis à jour (car le lecteur audio est dans un v-if)
  setTimeout(() => {
    if (audioPlayer.value) {
      const url = getTrackUrl(pk);
      audioPlayer.value.src = url;
      audioPlayer.value.play().catch((error) => {
        console.error("Failed to play audio:", error);
        audioError.value = `Cannot play audio: ${error.message}. Your browser may not support FLAC format.`;
        isPlaying.value = false;
      });
    }
  }, 100);
}

function stopTrack() {
  if (audioPlayer.value) {
    audioPlayer.value.pause();
    audioPlayer.value.currentTime = 0;
    audioPlayer.value.src = "";
  }
  isPlaying.value = false;
  audioError.value = "";
}

function handleAudioEnded() {
  isPlaying.value = false;
  audioError.value = "";
}

function handleAudioError() {
  const audio = audioPlayer.value;
  if (audio?.error) {
    let message = "Audio playback error: ";
    switch (audio.error.code) {
      case 1:
        message += "Loading aborted";
        break;
      case 2:
        message += "Network error";
        break;
      case 3:
        message += "Format not supported";
        break;
      case 4:
        message += "Source not found";
        break;
      default:
        message += "Unknown error";
    }
    console.error('Audio player error:', message, 'code:', audio.error.code);
    audioError.value = message;
    isPlaying.value = false;
  }
}

function downloadTrack(pk: string) {
  const track = getTrackByPk(pk);
  if (track && isLazyTrack(track)) {
    alert("This track is still in lazy cache. Play it once to download the audio before exporting.");
    return;
  }
  window.open(getOriginalTrackUrl(pk), "_blank");
}

function copyTrackUrl(pk: string) {
  const track = getTrackByPk(pk);
  if (track && isLazyTrack(track)) {
    alert("URL available after the lazy audio has been downloaded.");
    return;
  }
  navigator.clipboard.writeText(window.location.origin + getTrackUrl(pk));
  alert("✅ URL copied!");
}

function resolveTrackOrigin(track: AudioCacheEntry | null | undefined): string | undefined {
  return track ? getOriginUrl(track) : undefined;
}

function durationMs(track: AudioCacheEntry | null): number | undefined {
  return track ? getDurationMs(track.metadata) : undefined;
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

function formatConversion(
  conversion?: { mode?: string; input_codec?: string; details?: string } | null
): string | undefined {
  if (!conversion || !conversion.mode) return undefined;
  const modeLower = conversion.mode.toLowerCase();
  const modeLabel =
    modeLower === "passthrough"
      ? "Passthrough"
      : modeLower === "transcode"
      ? "Transcoded"
      : conversion.mode.charAt(0).toUpperCase() + conversion.mode.slice(1);

  if (conversion.input_codec) {
    const codec = conversion.input_codec.toUpperCase();
    if (modeLower === "passthrough") {
      return `${modeLabel} (${codec})`;
    }
    return `${modeLabel} (${codec} → FLAC)`;
  }

  if (conversion.details) {
    return `${modeLabel} – ${conversion.details}`;
  }

  return modeLabel;
}

function conversionLabel(track: AudioCacheEntry | null): string | undefined {
  return formatConversion(track?.metadata?.conversion ?? undefined);
}

function getTrackCoverUrl(track: AudioCacheEntry | null, size?: number): string | undefined {
  if (!track || failedCovers.value.has(track.pk)) return undefined;
  return getCoverUrl(track.metadata, size);
}

function handleCoverError(pk: string) {
  failedCovers.value.add(pk);
}

function prettifyLazyProvider(prefix: string): string {
  if (LAZY_PROVIDER_LABELS[prefix]) {
    return LAZY_PROVIDER_LABELS[prefix];
  }
  const normalized = prefix.replace(/[^A-Za-z0-9]+/g, " ").trim();
  if (!normalized) {
    return prefix.trim().toUpperCase() || "Lazy";
  }
  return normalized
    .split(/\s+/)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1).toLowerCase())
    .join(" ");
}

function buildLazyClass(prefix: string): string {
  return `lazy-provider-${prefix.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
}

function extractLazyInfoFromPk(pk: string | undefined | null): LazyDisplayInfo | undefined {
  if (!pk) return undefined;
  if (pk.startsWith(LEGACY_LAZY_PREFIX)) {
    return LEGACY_LAZY_INFO;
  }
  const separatorIndex = pk.indexOf(":");
  if (separatorIndex <= 0) {
    return undefined;
  }
  const prefix = pk.slice(0, separatorIndex);
  if (!prefix) {
    return undefined;
  }
  return {
    prefix,
    display: prettifyLazyProvider(prefix),
    className: buildLazyClass(prefix),
    isLegacy: false,
  };
}

function getLazyInfo(track: AudioCacheEntry | null | undefined): LazyDisplayInfo | undefined {
  if (!track?.pk) return undefined;
  return extractLazyInfoFromPk(track.pk);
}

function isLazyTrack(track: AudioCacheEntry | null | undefined): boolean {
  return !!getLazyInfo(track);
}

function lazyProviderName(track: AudioCacheEntry | null | undefined): string | undefined {
  return getLazyInfo(track)?.display;
}

function lazyProviderClass(track: AudioCacheEntry | null | undefined): string {
  return getLazyInfo(track)?.className ?? "lazy-provider-generic";
}

function lazyBadgeLabel(track: AudioCacheEntry | null | undefined): string {
  const info = getLazyInfo(track);
  if (!info) return "";
  return info.isLegacy ? "Lazy" : `Lazy - ${info.display}`;
}

function trackStatusLabel(track: AudioCacheEntry | null): string {
  if (isLocalFile(track)) {
    return "Local FLAC reference (original preserved)";
  }
  const info = getLazyInfo(track);
  if (info) {
    const provider = info.isLegacy ? "" : ` - ${info.display}`;
    return `Lazy${provider} (audio pending download)`;
  }
  return "Cached";
}

function getTrackByPk(pk: string): AudioCacheEntry | undefined {
  return tracks.value.find((t) => t.pk === pk);
}

function isLocalFile(track: AudioCacheEntry | null | undefined): boolean {
  return track?.metadata?.local_passthrough === true;
}

function localSourcePath(track: AudioCacheEntry | null | undefined): string | undefined {
  if (!track) return undefined;
  const metaValue = track.metadata?.local_source_path;
  if (typeof metaValue === "string" && metaValue.trim().length > 0) {
    return metaValue;
  }
  const origin = resolveTrackOrigin(track);
  if (origin?.startsWith("file://")) {
    return origin.replace("file://", "");
  }
  return undefined;
}

onMounted(() => {
  refreshTracks();
});
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

.source-toggle {
  display: flex;
  gap: 1rem;
  margin-bottom: 0.75rem;
  flex-wrap: wrap;
  color: #ccc;
  font-size: 0.95rem;
}

.source-toggle input {
  margin-right: 0.3rem;
}

.local-tip {
  margin: 0.25rem 0 0.75rem;
  font-size: 0.85rem;
  color: #bbb;
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

.track-card.local-reference {
  border: 1px solid rgba(97, 218, 251, 0.6);
  box-shadow: 0 0 0 1px rgba(97, 218, 251, 0.1);
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

.cover-image {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
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

.track-overlay .hits.lazy {
  display: inline-flex;
  align-items: center;
  background: rgba(156, 39, 176, 0.85);
  color: #fff;
  font-weight: bold;
  padding: 0.2rem 0.6rem;
  border-radius: 999px;
  font-size: 0.8rem;
}

.local-pill {
  display: inline-block;
  margin-left: 0.5rem;
  padding: 0.15rem 0.5rem;
  border-radius: 999px;
  background: rgba(97, 218, 251, 0.9);
  color: #0c1924;
  font-size: 0.75rem;
  font-weight: 600;
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

.lazy-tag {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  margin-left: 0.5rem;
  padding: 0.15rem 0.6rem;
  border-radius: 999px;
  background: rgba(156, 39, 176, 0.25);
  color: #f5f5f5;
  font-size: 0.7rem;
  text-transform: uppercase;
  font-weight: 600;
  border: 1px solid rgba(156, 39, 176, 0.4);
}

.lazy-tag .lazy-label {
  font-weight: 700;
  letter-spacing: 0.5px;
}

.lazy-tag .lazy-provider-name {
  font-size: 0.6rem;
  text-transform: none;
  letter-spacing: 0.4px;
  opacity: 0.9;
}

.meta {
  display: flex;
  gap: 0.75rem;
  font-size: 0.85rem;
  color: #999;
  flex-wrap: wrap;
}

.local-path {
  margin: 0.4rem 0;
  font-size: 0.8rem;
  color: #a0f0ff;
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  align-items: baseline;
}

.local-badge {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.8px;
  background: rgba(97, 218, 251, 0.2);
  border: 1px solid rgba(97, 218, 251, 0.4);
  border-radius: 999px;
  padding: 0.1rem 0.5rem;
  color: #61dafb;
}

.local-path-text {
  font-family: "Fira Code", "SFMono-Regular", Consolas, monospace;
  color: #e3f7ff;
  word-break: break-all;
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

.lazy-warning {
  margin-top: 0.75rem;
  font-size: 0.85rem;
  color: #ffb347;
  background: rgba(255, 152, 0, 0.15);
  border: 1px solid rgba(255, 152, 0, 0.3);
  padding: 0.5rem;
  border-radius: 6px;
}

.track-overlay .hits.lazy.lazy-provider-legacy,
.lazy-tag.lazy-provider-legacy {
  background: rgba(255, 152, 0, 0.85);
  color: #000;
  border-color: rgba(255, 193, 7, 0.8);
}

.track-overlay .hits.lazy.lazy-provider-qobuz,
.lazy-tag.lazy-provider-qobuz {
  background: rgba(76, 175, 80, 0.9);
  color: #fff;
  border-color: rgba(165, 214, 167, 0.9);
}

.track-overlay .hits.lazy.lazy-provider-generic,
.lazy-tag.lazy-provider-generic {
  background: rgba(103, 58, 183, 0.85);
  color: #fff;
  border-color: rgba(179, 157, 219, 0.8);
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

.modal-cover-image {
  width: 80px;
  height: 80px;
  object-fit: cover;
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
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

/* Lecteur audio */
.audio-player-container {
  position: fixed;
  bottom: 2rem;
  right: 2rem;
  background: #2a2a2a;
  padding: 1rem;
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
  display: flex;
  gap: 1rem;
  align-items: center;
  z-index: 1001;
}

.audio-player-container audio {
  max-width: 400px;
}

.audio-error {
  color: #ff6b6b;
  margin: 0;
  padding: 0.5rem;
  background: rgba(255, 107, 107, 0.1);
  border-radius: 4px;
  max-width: 400px;
}

.btn-stop {
  background: #ff6b6b;
  color: #fff;
  padding: 0.75rem 1.5rem;
  white-space: nowrap;
}

.btn-stop:hover:not(:disabled) {
  background: #ee5a52;
}

@media (max-width: 768px) {
  .audio-player-container {
    bottom: 1rem;
    right: 1rem;
    left: 1rem;
    flex-direction: column;
  }

  .audio-player-container audio {
    max-width: 100%;
    width: 100%;
  }
}
</style>
