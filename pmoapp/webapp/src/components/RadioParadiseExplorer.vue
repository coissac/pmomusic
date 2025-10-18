<template>
  <div class="radio-paradise-explorer">
    <div class="header">
      <h2>Radio Paradise Explorer</h2>
      <div class="controls">
        <button @click="refreshNowPlaying" :disabled="loading" class="btn-primary">
          <span v-if="loading">⏳</span>
          <span v-else>🔄</span>
          Refresh
        </button>
        <select v-model="selectedChannel" @change="changeChannel" class="channel-select">
          <option v-for="channel in channels" :key="channel.id" :value="channel.id">
            {{ channel.name }}
          </option>
        </select>
      </div>
    </div>

    <div v-if="error" class="error-message">
      ❌ {{ error }}
    </div>

    <!-- Now Playing Section -->
    <div v-if="nowPlaying" class="now-playing-section">
      <h3>🎵 Now Playing</h3>
      <div v-if="nowPlaying.current_song" class="current-song">
        <div v-if="nowPlaying.current_song.cover_url" class="cover-art">
          <img :src="nowPlaying.current_song.cover_url" :alt="`${nowPlaying.current_song.album} cover`">
        </div>
        <div class="song-details">
          <div class="artist">{{ nowPlaying.current_song.artist }}</div>
          <div class="title">{{ nowPlaying.current_song.title }}</div>
          <div class="album">{{ nowPlaying.current_song.album }}</div>
          <div class="metadata">
            <span v-if="nowPlaying.current_song.year" class="year">{{ nowPlaying.current_song.year }}</span>
            <span class="duration">{{ formatDuration(nowPlaying.current_song.duration_ms) }}</span>
            <span v-if="nowPlaying.current_song.rating" class="rating">⭐ {{ nowPlaying.current_song.rating.toFixed(1) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Block Info -->
    <div v-if="nowPlaying" class="block-info">
      <h3>📦 Block Info</h3>
      <div class="block-details">
        <div class="info-row">
          <span class="label">Event ID:</span>
          <span class="value">{{ nowPlaying.event }}</span>
        </div>
        <div class="info-row">
          <span class="label">Next Event:</span>
          <span class="value">{{ nowPlaying.end_event }}</span>
        </div>
        <div class="info-row">
          <span class="label">Block Length:</span>
          <span class="value">{{ formatDuration(nowPlaying.block_length_ms) }}</span>
        </div>
        <div class="info-row">
          <span class="label">Songs in Block:</span>
          <span class="value">{{ nowPlaying.songs.length }}</span>
        </div>
        <div class="info-row">
          <span class="label">Stream URL:</span>
          <a :href="nowPlaying.stream_url" target="_blank" class="stream-link">{{ nowPlaying.stream_url }}</a>
        </div>
      </div>
    </div>

    <!-- Songs List -->
    <div v-if="nowPlaying && nowPlaying.songs" class="songs-section">
      <h3>🎼 Songs in Current Block</h3>
      <div class="songs-list">
        <div
          v-for="song in nowPlaying.songs"
          :key="song.index"
          :class="['song-item', { 'current': song.index === nowPlaying.current_song_index }]"
        >
          <div class="song-number">{{ song.index + 1 }}</div>
          <div v-if="song.cover_url" class="song-cover-mini">
            <img :src="song.cover_url" :alt="`${song.album} cover`">
          </div>
          <div class="song-info">
            <div class="song-title">{{ song.title }}</div>
            <div class="song-artist">{{ song.artist }}</div>
            <div class="song-album">{{ song.album }}</div>
          </div>
          <div class="song-meta">
            <div class="song-year" v-if="song.year">{{ song.year }}</div>
            <div class="song-duration">{{ formatDuration(song.duration_ms) }}</div>
            <div class="song-elapsed">@{{ formatDuration(song.elapsed_ms) }}</div>
            <div v-if="song.rating" class="song-rating">⭐ {{ song.rating.toFixed(1) }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Available Channels -->
    <div class="channels-section">
      <h3>📻 Available Channels</h3>
      <div class="channels-grid">
        <div
          v-for="channel in channels"
          :key="channel.id"
          :class="['channel-card', { 'active': channel.id === selectedChannel }]"
          @click="selectChannel(channel.id)"
        >
          <div class="channel-name">{{ channel.name }}</div>
          <div class="channel-description">{{ channel.description }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'

const API_BASE = '/api/radioparadise'

const loading = ref(false)
const error = ref(null)
const nowPlaying = ref(null)
const channels = ref([])
const selectedChannel = ref(0)

// Format duration from milliseconds to MM:SS
function formatDuration(ms) {
  const seconds = Math.floor(ms / 1000)
  const minutes = Math.floor(seconds / 60)
  const remainingSeconds = seconds % 60
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`
}

// Fetch now playing info
async function refreshNowPlaying() {
  loading.value = true
  error.value = null

  try {
    const response = await fetch(`${API_BASE}/now-playing`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    nowPlaying.value = await response.json()
  } catch (e) {
    error.value = `Failed to fetch now playing: ${e.message}`
    console.error('Error fetching now playing:', e)
  } finally {
    loading.value = false
  }
}

// Fetch available channels
async function fetchChannels() {
  try {
    const response = await fetch(`${API_BASE}/channels`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    channels.value = await response.json()
  } catch (e) {
    error.value = `Failed to fetch channels: ${e.message}`
    console.error('Error fetching channels:', e)
  }
}

// Select a channel
function selectChannel(channelId) {
  selectedChannel.value = channelId
  // For now, just highlight it - we could implement channel switching
  // when the API supports it
}

// Change channel (placeholder for future implementation)
function changeChannel() {
  console.log('Channel changed to:', selectedChannel.value)
  // TODO: Implement channel switching in the API
}

// Initialize on mount
onMounted(async () => {
  await fetchChannels()
  await refreshNowPlaying()

  // Auto-refresh every 30 seconds
  setInterval(() => {
    if (!loading.value) {
      refreshNowPlaying()
    }
  }, 30000)
})
</script>

<style scoped>
.radio-paradise-explorer {
  padding: 20px;
  max-width: 1200px;
  margin: 0 auto;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
  padding-bottom: 10px;
  border-bottom: 2px solid #333;
}

.header h2 {
  margin: 0;
  color: #00d4ff;
}

.controls {
  display: flex;
  gap: 10px;
  align-items: center;
}

.btn-primary {
  background: #00d4ff;
  color: #000;
  border: none;
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  font-weight: bold;
  transition: background 0.3s;
}

.btn-primary:hover:not(:disabled) {
  background: #00a8cc;
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.channel-select {
  padding: 8px 12px;
  border-radius: 4px;
  border: 1px solid #333;
  background: #1a1a1a;
  color: #fff;
  cursor: pointer;
}

.error-message {
  background: #ff4444;
  color: white;
  padding: 12px;
  border-radius: 4px;
  margin-bottom: 20px;
}

/* Now Playing Section */
.now-playing-section {
  background: linear-gradient(135deg, #1a1a1a 0%, #2a2a2a 100%);
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid #333;
}

.now-playing-section h3 {
  margin-top: 0;
  color: #00d4ff;
}

.current-song {
  display: flex;
  gap: 20px;
  align-items: flex-start;
}

.cover-art {
  flex-shrink: 0;
}

.cover-art img {
  width: 200px;
  height: 200px;
  object-fit: cover;
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
}

.song-details {
  flex: 1;
}

.artist {
  font-size: 1.8em;
  font-weight: bold;
  color: #00d4ff;
  margin-bottom: 8px;
}

.title {
  font-size: 1.4em;
  margin-bottom: 8px;
  color: #fff;
}

.album {
  font-size: 1.1em;
  color: #999;
  margin-bottom: 12px;
}

.metadata {
  display: flex;
  gap: 15px;
  font-size: 0.9em;
  color: #666;
}

.metadata span {
  padding: 4px 8px;
  background: #333;
  border-radius: 4px;
}

/* Block Info */
.block-info {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid #333;
}

.block-info h3 {
  margin-top: 0;
  color: #00d4ff;
}

.block-details {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.info-row {
  display: flex;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid #333;
}

.info-row:last-child {
  border-bottom: none;
}

.label {
  font-weight: bold;
  color: #999;
}

.value {
  color: #fff;
}

.stream-link {
  color: #00d4ff;
  text-decoration: none;
  word-break: break-all;
}

.stream-link:hover {
  text-decoration: underline;
}

/* Songs List */
.songs-section {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid #333;
}

.songs-section h3 {
  margin-top: 0;
  color: #00d4ff;
}

.songs-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.song-item {
  display: flex;
  gap: 12px;
  padding: 12px;
  background: #2a2a2a;
  border-radius: 4px;
  align-items: center;
  transition: background 0.3s;
}

.song-item:hover {
  background: #333;
}

.song-item.current {
  background: #003d4d;
  border-left: 4px solid #00d4ff;
}

.song-number {
  font-weight: bold;
  color: #666;
  min-width: 30px;
  text-align: center;
}

.song-cover-mini img {
  width: 50px;
  height: 50px;
  object-fit: cover;
  border-radius: 4px;
}

.song-info {
  flex: 1;
  min-width: 0;
}

.song-title {
  font-weight: bold;
  color: #fff;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.song-artist {
  color: #00d4ff;
  font-size: 0.9em;
}

.song-album {
  color: #999;
  font-size: 0.85em;
}

.song-meta {
  display: flex;
  gap: 10px;
  font-size: 0.85em;
  color: #666;
  align-items: center;
}

.song-meta > div {
  padding: 2px 6px;
  background: #1a1a1a;
  border-radius: 3px;
}

/* Channels Section */
.channels-section {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  border: 1px solid #333;
}

.channels-section h3 {
  margin-top: 0;
  color: #00d4ff;
}

.channels-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  gap: 15px;
}

.channel-card {
  background: #2a2a2a;
  padding: 15px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.3s;
  border: 2px solid transparent;
}

.channel-card:hover {
  background: #333;
  transform: translateY(-2px);
}

.channel-card.active {
  border-color: #00d4ff;
  background: #003d4d;
}

.channel-name {
  font-weight: bold;
  color: #00d4ff;
  margin-bottom: 8px;
  font-size: 1.1em;
}

.channel-description {
  color: #999;
  font-size: 0.9em;
}
</style>
