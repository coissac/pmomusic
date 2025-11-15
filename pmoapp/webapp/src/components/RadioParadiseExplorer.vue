<template>
  <div class="radio-paradise-explorer">
    <div class="header">
      <div class="title-block">
        <h2>Radio Paradise Explorer</h2>
        <div class="status-bar">
          <span v-if="lastUpdated">Updated {{ formatTimestamp(lastUpdated) }}</span>
          <span v-if="nowPlaying">Current block #{{ nowPlaying.event }}</span>
        </div>
      </div>
      <div class="controls">
        <button @click="refreshNowPlaying" :disabled="loading" class="btn-primary">
          <span v-if="loading">⏳</span>
          <span v-else>🔄</span>
          Refresh
        </button>
        <button
          @click="togglePlayback"
          class="btn-play"
        >
          {{ isPlaying ? '⏹️ Stop' : '▶️ Play' }}
        </button>
        <select v-model="selectedChannel" @change="changeChannel" class="channel-select">
          <option v-for="channel in channels" :key="channel.id" :value="channel.id">
            {{ channel.name }}
          </option>
        </select>
      </div>
    </div>

    <!-- Stream Format & Mode Selection -->
    <div class="stream-controls-section">
      <h3>🎚️ Stream Configuration</h3>
      <div class="stream-controls-grid">
        <div class="control-group">
          <label>Stream Format</label>
          <div class="format-chips">
            <button
              v-for="format in streamFormats"
              :key="format.id"
              :class="['format-chip', { active: selectedFormat === format.id }]"
              @click="selectedFormat = format.id"
            >
              {{ format.name }}
            </button>
          </div>
          <div class="format-description">{{ currentFormatDescription }}</div>
        </div>
        <div class="control-group">
          <label>Playback Mode</label>
          <div class="mode-chips">
            <button
              :class="['mode-chip', { active: playbackMode === 'live' }]"
              @click="playbackMode = 'live'"
            >
              📡 Live
            </button>
            <button
              :class="['mode-chip', { active: playbackMode === 'historic' }]"
              @click="playbackMode = 'historic'"
            >
              🕰️ Historic
            </button>
          </div>
        </div>
        <div v-if="playbackMode === 'historic'" class="control-group">
          <label>Client ID (for history tracking)</label>
          <input
            v-model="historicClientId"
            type="text"
            placeholder="Enter unique client ID"
            class="client-id-input"
          />
        </div>
      </div>
    </div>

    <!-- Stream Diagnostics -->
    <div class="stream-diagnostics-section">
      <div class="section-header">
        <h3>🔧 Stream Diagnostics</h3>
        <button class="btn-tertiary" @click="copyStreamUrl">
          📋 Copy Stream URL
        </button>
      </div>
      <div class="diagnostics-grid">
        <div class="diagnostic-card">
          <div class="diagnostic-label">Current Stream URL</div>
          <div class="diagnostic-value">
            <code>{{ currentStreamUrl }}</code>
          </div>
        </div>
        <div class="diagnostic-card">
          <div class="diagnostic-label">Format</div>
          <div class="diagnostic-value">{{ selectedFormat.toUpperCase() }}</div>
        </div>
        <div class="diagnostic-card">
          <div class="diagnostic-label">Mode</div>
          <div class="diagnostic-value">{{ playbackMode === 'live' ? 'Live Stream' : 'Historical Playback' }}</div>
        </div>
        <div class="diagnostic-card">
          <div class="diagnostic-label">Channel Slug</div>
          <div class="diagnostic-value">{{ currentChannelSlug }}</div>
        </div>
      </div>
      <div class="stream-urls-list">
        <div class="url-item" v-for="format in streamFormats" :key="`url-${format.id}`">
          <strong>{{ format.name }}:</strong>
          <a :href="getStreamUrlFor(format.id)" target="_blank" class="stream-link">
            {{ getStreamUrlFor(format.id) }}
          </a>
        </div>
      </div>
    </div>

    <!-- Live Stream Metadata -->
    <div v-if="playbackMode === 'live'" class="stream-metadata-section">
      <div class="section-header">
        <h3>📻 Live Stream Metadata</h3>
        <button class="btn-secondary" @click="fetchStreamMetadata" :disabled="streamMetadataLoading">
          <span v-if="streamMetadataLoading">⏳ Loading…</span>
          <span v-else>🔄 Refresh Metadata</span>
        </button>
      </div>
      <div v-if="streamMetadataError" class="error-message inline-error">
        ❌ {{ streamMetadataError }}
      </div>
      <div v-else-if="streamMetadata" class="metadata-content">
        <pre class="metadata-json">{{ JSON.stringify(streamMetadata, null, 2) }}</pre>
      </div>
      <div v-else class="empty-placeholder">
        Click "Refresh Metadata" to fetch current stream metadata
      </div>
      <div v-if="streamMetadataLastUpdated" class="section-meta">
        Last updated: {{ formatTimestamp(streamMetadataLastUpdated) }}
      </div>
    </div>

    <!-- Quick Test Tools -->
    <div class="quick-test-section">
      <h3>🧪 Quick Test Commands</h3>
      <p class="section-description">Copy and paste these commands to test streams with external players</p>
      <div class="test-commands">
        <div class="command-group">
          <div class="command-label">
            <strong>ffplay (FLAC)</strong>
            <button class="btn-copy-small" @click="copyToClipboard(ffplayFlacCommand)">📋</button>
          </div>
          <code class="command-text">{{ ffplayFlacCommand }}</code>
        </div>
        <div class="command-group">
          <div class="command-label">
            <strong>ffplay (OGG)</strong>
            <button class="btn-copy-small" @click="copyToClipboard(ffplayOggCommand)">📋</button>
          </div>
          <code class="command-text">{{ ffplayOggCommand }}</code>
        </div>
        <div class="command-group">
          <div class="command-label">
            <strong>VLC</strong>
            <button class="btn-copy-small" @click="copyToClipboard(vlcCommand)">📋</button>
          </div>
          <code class="command-text">{{ vlcCommand }}</code>
        </div>
        <div class="command-group">
          <div class="command-label">
            <strong>curl (Download to file)</strong>
            <button class="btn-copy-small" @click="copyToClipboard(curlCommand)">📋</button>
          </div>
          <code class="command-text">{{ curlCommand }}</code>
        </div>
      </div>
    </div>

    <!-- Enhanced Media Player -->
    <div v-if="isPlaying || audioError" class="enhanced-player-section">
      <div class="player-header">
        <h3>🎵 Now Playing on Radio Paradise</h3>
        <div class="player-controls-header">
          <button @click="refreshPlayerMetadata" :disabled="playerMetadataLoading" class="btn-refresh-meta">
            <span v-if="playerMetadataLoading">⏳</span>
            <span v-else>🔄</span>
          </button>
          <button @click="stopPlayback" class="btn-stop">
            {{ audioError ? '✕ Close' : '⏹️ Stop' }}
          </button>
        </div>
      </div>

      <div v-if="audioError" class="audio-error-banner">
        ❌ {{ audioError }}
      </div>

      <div v-else class="player-content">
        <!-- Cover Art & Metadata -->
        <div class="player-info">
          <div v-if="playerMetadata?.image_url" class="player-cover">
            <img :src="playerMetadata.image_url" :alt="playerMetadata.title || 'Album cover'">
          </div>
          <div v-else class="player-cover-placeholder">
            🎵
          </div>

          <div class="player-metadata">
            <div v-if="playerMetadata" class="metadata-display">
              <div class="player-title">{{ playerMetadata.title || 'Unknown Title' }}</div>
              <div class="player-artist">{{ playerMetadata.artist || 'Unknown Artist' }}</div>
              <div v-if="playerMetadata.album" class="player-album">{{ playerMetadata.album }}</div>
              <div v-if="playerMetadata.year" class="player-year">{{ playerMetadata.year }}</div>
            </div>
            <div v-else class="metadata-loading">
              <span v-if="playerMetadataLoading">Loading metadata...</span>
              <span v-else>No metadata available</span>
            </div>
          </div>
        </div>

        <!-- Audio Element with Controls -->
        <div class="player-audio-controls">
          <audio
            ref="audioPlayer"
            controls
            @ended="handleAudioEnded"
            @error="handleAudioError"
            @play="handleAudioPlay"
            @pause="handleAudioPause"
          ></audio>
        </div>

        <!-- Stream Info -->
        <div class="player-stream-info">
          <div class="stream-info-item">
            <span class="info-label">Channel:</span>
            <span class="info-value">{{ channels.find(c => c.id === selectedChannel)?.name || 'Unknown' }}</span>
          </div>
          <div class="stream-info-item">
            <span class="info-label">Format:</span>
            <span class="info-value">{{ selectedFormat.toUpperCase() }}</span>
          </div>
          <div class="stream-info-item">
            <span class="info-label">Mode:</span>
            <span class="info-value">{{ playbackMode === 'live' ? 'Live' : 'Historic' }}</span>
          </div>
          <div v-if="playerMetadataLastUpdated" class="stream-info-item">
            <span class="info-label">Updated:</span>
            <span class="info-value">{{ formatTimestamp(playerMetadataLastUpdated) }}</span>
          </div>
        </div>
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

    <div v-if="nowPlaying" class="block-actions">
      <button
        class="btn-secondary"
        @click="loadUpcomingBlock"
        :disabled="upcomingLoading || !nowPlaying?.end_event"
      >
        <span v-if="upcomingLoading">⏳ Loading next block…</span>
        <span v-else>⏭️ Preview Next Block</span>
      </button>
      <div class="next-block-info">
        <span>Next event:</span>
        <code>{{ nowPlaying.end_event }}</code>
      </div>
    </div>

    <div v-if="upcomingError" class="error-message">
      ❌ {{ upcomingError }}
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

    <div v-if="upcomingBlock" class="songs-section upcoming-section">
      <div class="section-header">
        <h3>⏭️ Next Block Preview</h3>
        <div class="section-meta">
          <span>Event:</span>
          <code>{{ upcomingBlock.event }}</code>
          <span>Next event:</span>
          <code>{{ upcomingBlock.end_event }}</code>
        </div>
      </div>
      <div class="summary-row">
        <span>{{ upcomingBlock.songs.length }} song(s)</span>
        <span>Total duration: {{ formatDuration(upcomingBlock.length_ms) }}</span>
        <a :href="upcomingBlock.url" target="_blank" class="stream-link">Open block stream</a>
      </div>
      <div class="songs-list">
        <div v-for="song in upcomingBlock.songs" :key="`${upcomingBlock.event}-${song.index}`" class="song-item">
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
          </div>
        </div>
      </div>
    </div>

    <div class="block-search">
      <h3>🔎 Block Lookup</h3>
      <div class="search-controls">
        <input
          v-model="blockSearchId"
          type="number"
          inputmode="numeric"
          min="0"
          placeholder="Enter an event ID (e.g. 123456)"
        >
        <button
          class="btn-secondary"
          @click="lookupBlockById"
          :disabled="!blockSearchId || blockSearchLoading"
        >
          <span v-if="blockSearchLoading">⏳ Loading…</span>
          <span v-else>Load Block</span>
        </button>
        <button class="btn-tertiary" @click="clearBlockSearch" :disabled="!blockSearchResult && !blockSearchError && !blockSearchId">
          Clear
        </button>
      </div>
      <div v-if="blockSearchError" class="error-message">
        ❌ {{ blockSearchError }}
      </div>
      <div v-if="blockSearchResult" class="songs-section lookup-section">
        <div class="section-header">
          <h3>📂 Block {{ blockSearchResult.event }}</h3>
          <div class="section-meta">
            <span>Next event:</span>
            <code>{{ blockSearchResult.end_event }}</code>
          </div>
        </div>
        <div class="summary-row">
          <span>{{ blockSearchResult.songs.length }} song(s)</span>
          <span>Total duration: {{ formatDuration(blockSearchResult.length_ms) }}</span>
          <a :href="blockSearchResult.url" target="_blank" class="stream-link">Open block stream</a>
        </div>
        <div class="songs-list">
          <div v-for="song in blockSearchResult.songs" :key="`${blockSearchResult.event}-${song.index}`" class="song-item">
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
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Available Channels -->
    <div v-if="channelsError" class="error-message inline-error">
      ❌ {{ channelsError }}
    </div>

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
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'

const API_BASE = '/api/radioparadise'
const SOURCE_API_BASE = '/api/sources'
const SOURCE_ID = 'radio-paradise'
const STREAM_BASE = '/radioparadise/stream'

const loading = ref(false)
const error = ref(null)
const nowPlaying = ref(null)
const channels = ref([])
const selectedChannel = ref(0)
const audioPlayer = ref(null)
const isPlaying = ref(false)
const audioError = ref('')
const lastUpdated = ref(null)

// New stream format controls
const selectedFormat = ref('flac')
const playbackMode = ref('live')
const historicClientId = ref('test-client-' + Math.random().toString(36).substring(7))

const streamFormats = [
  { id: 'flac', name: 'FLAC', description: 'Pure FLAC lossless audio stream' },
  { id: 'ogg', name: 'OGG-FLAC', description: 'FLAC in OGG container for better streaming' }
]

// Channel slug mapping
const channelSlugs = ['main', 'mellow', 'rock', 'eclectic']

// Computed properties
const currentChannelSlug = computed(() => {
  return channelSlugs[selectedChannel.value] || 'main'
})

const currentFormatDescription = computed(() => {
  const format = streamFormats.find(f => f.id === selectedFormat.value)
  return format ? format.description : ''
})

const currentStreamUrl = computed(() => {
  return getStreamUrlFor(selectedFormat.value)
})

// Test commands computed properties
const ffplayFlacCommand = computed(() => {
  const url = getFullStreamUrl('flac')
  return `ffplay -nodisp "${url}"`
})

const ffplayOggCommand = computed(() => {
  const url = getFullStreamUrl('ogg')
  return `ffplay -nodisp "${url}"`
})

const vlcCommand = computed(() => {
  const url = getFullStreamUrl(selectedFormat.value)
  return `vlc "${url}"`
})

const curlCommand = computed(() => {
  const url = getFullStreamUrl(selectedFormat.value)
  const ext = selectedFormat.value === 'ogg' ? 'ogg' : 'flac'
  const slug = currentChannelSlug.value
  return `curl "${url}" -o "${slug}-stream.${ext}"`
})

function getStreamUrlFor(format) {
  const slug = currentChannelSlug.value
  if (playbackMode.value === 'historic') {
    return `${STREAM_BASE}/${slug}/historic/${historicClientId.value}/${format}`
  }
  return `${STREAM_BASE}/${slug}/${format}`
}

function getFullStreamUrl(format) {
  // Get full URL including protocol and host
  const baseUrl = window.location.origin
  return baseUrl + getStreamUrlFor(format)
}

function copyStreamUrl() {
  copyToClipboard(currentStreamUrl.value)
}

function copyToClipboard(text) {
  navigator.clipboard.writeText(text).then(() => {
    console.log('Copied to clipboard:', text)
    // Could add a toast notification here
  }).catch(err => {
    console.error('Failed to copy:', err)
  })
}
const upcomingBlock = ref(null)
const upcomingLoading = ref(false)
const upcomingError = ref('')
const blockSearchId = ref('')
const blockSearchResult = ref(null)
const blockSearchLoading = ref(false)
const blockSearchError = ref('')
const channelsError = ref('')

// Stream metadata state
const streamMetadata = ref(null)
const streamMetadataLoading = ref(false)
const streamMetadataError = ref('')
const streamMetadataLastUpdated = ref(null)

// Player metadata state (for the enhanced player)
const playerMetadata = ref(null)
const playerMetadataLoading = ref(false)
const playerMetadataLastUpdated = ref(null)

let refreshTimerId = null
let playerMetadataTimerId = null

// Format duration from milliseconds to MM:SS
function formatDuration(ms) {
  if (typeof ms !== 'number' || !Number.isFinite(ms)) {
    return '--:--'
  }
  const seconds = Math.max(0, Math.floor(ms / 1000))
  const minutes = Math.floor(seconds / 60)
  const remainingSeconds = seconds % 60
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`
}

function formatTimestamp(date) {
  if (!date) return ''
  return date.toLocaleTimeString()
}

function buildQuery(extra = {}) {
  const params = new URLSearchParams()
  if (selectedChannel.value != null) {
    params.set('channel', selectedChannel.value.toString())
  }

  Object.entries(extra).forEach(([key, value]) => {
    if (value !== undefined && value !== null && value !== '') {
      params.set(key, String(value))
    }
  })

  const query = params.toString()
  return query ? `?${query}` : ''
}

async function fetchBlockByEvent(eventId) {
  const query = buildQuery()
  const response = await fetch(`${API_BASE}/block/${eventId}${query}`)
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`)
  }
  return await response.json()
}

function playAudio(url) {
  if (!url) {
    audioError.value = 'No audio URL available'
    isPlaying.value = false
    return
  }

  audioError.value = ''
  isPlaying.value = true

  nextTick(() => {
    const player = audioPlayer.value
    if (!player) {
      return
    }
    player.src = url
    player.play().catch((e) => {
      console.error('Failed to start playback:', e)
      audioError.value = `Cannot play stream: ${e.message}`
      isPlaying.value = false
    })
  })
}

// Fetch now playing info
async function refreshNowPlaying() {
  loading.value = true
  error.value = null

  try {
    const query = buildQuery()
    const response = await fetch(`${API_BASE}/now-playing${query}`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    nowPlaying.value = await response.json()
    lastUpdated.value = new Date()
    upcomingBlock.value = null
    upcomingError.value = ''
  } catch (e) {
    error.value = `Failed to fetch now playing: ${e.message}`
    console.error('Error fetching now playing:', e)
  } finally {
    loading.value = false
  }
}

async function fetchStreamMetadata() {
  streamMetadataLoading.value = true
  streamMetadataError.value = ''
  try {
    const slug = currentChannelSlug.value
    const response = await fetch(`/radioparadise/metadata/${slug}`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    streamMetadata.value = await response.json()
    streamMetadataLastUpdated.value = new Date()
  } catch (e) {
    streamMetadataError.value = `Failed to fetch stream metadata: ${e.message}`
    console.error('Error fetching stream metadata:', e)
  } finally {
    streamMetadataLoading.value = false
  }
}

// Fetch player metadata (for enhanced player)
async function refreshPlayerMetadata() {
  playerMetadataLoading.value = true
  try {
    const slug = currentChannelSlug.value
    const response = await fetch(`/radioparadise/metadata/${slug}`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    const metadata = await response.json()
    playerMetadata.value = metadata
    playerMetadataLastUpdated.value = new Date()

    // Update Media Session API if available
    updateMediaSession(metadata)
  } catch (e) {
    console.error('Error fetching player metadata:', e)
    // Don't show error to user, just log it
  } finally {
    playerMetadataLoading.value = false
  }
}

// Update browser Media Session API for notifications/lock screen
function updateMediaSession(metadata) {
  if ('mediaSession' in navigator && metadata) {
    navigator.mediaSession.metadata = new MediaMetadata({
      title: metadata.title || 'Unknown Title',
      artist: metadata.artist || 'Unknown Artist',
      album: metadata.album || 'Radio Paradise',
      artwork: metadata.image_url ? [
        { src: metadata.image_url, sizes: '512x512', type: 'image/jpeg' }
      ] : []
    })

    // Set up action handlers
    navigator.mediaSession.setActionHandler('play', () => {
      audioPlayer.value?.play()
    })
    navigator.mediaSession.setActionHandler('pause', () => {
      audioPlayer.value?.pause()
    })
    navigator.mediaSession.setActionHandler('stop', () => {
      stopPlayback()
    })
  }
}

// Start auto-refresh of player metadata
function startPlayerMetadataRefresh() {
  // Clear any existing timer
  if (playerMetadataTimerId) {
    clearInterval(playerMetadataTimerId)
  }

  // Fetch immediately
  refreshPlayerMetadata()

  // Then refresh every 10 seconds while playing
  playerMetadataTimerId = window.setInterval(() => {
    if (isPlaying.value) {
      refreshPlayerMetadata()
    }
  }, 10000)
}

// Stop auto-refresh of player metadata
function stopPlayerMetadataRefresh() {
  if (playerMetadataTimerId) {
    clearInterval(playerMetadataTimerId)
    playerMetadataTimerId = null
  }
}

// Fetch available channels
async function fetchChannels() {
  try {
    const response = await fetch(`${API_BASE}/channels`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    const data = await response.json()
    channels.value = data
    channelsError.value = ''

    if (channels.value.length > 0) {
      const hasSelected = channels.value.some(channel => channel.id === selectedChannel.value)
      if (!hasSelected) {
        selectedChannel.value = channels.value[0].id
      }
    }
  } catch (e) {
    channelsError.value = `Failed to fetch channels: ${e.message}`
    console.error('Error fetching channels:', e)
  }
}

// Select a channel
function selectChannel(channelId) {
  if (selectedChannel.value === channelId) {
    return
  }
  selectedChannel.value = channelId
  changeChannel()
}

async function changeChannel() {
  await refreshNowPlaying()
  blockSearchResult.value = null
  blockSearchError.value = ''
}

function playStream() {
  // Use the custom stream URL based on selected format and mode
  const streamUrl = currentStreamUrl.value

  if (!streamUrl) {
    audioError.value = 'No stream URL available'
    isPlaying.value = false
    return
  }

  playAudio(streamUrl)
  // Start fetching metadata for the player
  startPlayerMetadataRefresh()
}

function stopPlayback() {
  if (audioPlayer.value) {
    audioPlayer.value.pause()
    audioPlayer.value.currentTime = 0
    audioPlayer.value.src = ''
  }
  isPlaying.value = false
  audioError.value = ''

  // Stop metadata refresh
  stopPlayerMetadataRefresh()
  playerMetadata.value = null

  // Clear Media Session
  if ('mediaSession' in navigator) {
    navigator.mediaSession.metadata = null
  }
}

function togglePlayback() {
  if (isPlaying.value) {
    stopPlayback()
  } else {
    playStream()
  }
}

function handleAudioEnded() {
  isPlaying.value = false
  stopPlayerMetadataRefresh()
}

function handleAudioError() {
  const audio = audioPlayer.value
  if (audio?.error) {
    audioError.value = `Audio playback error (code ${audio.error.code})`
  } else {
    audioError.value = 'Unknown audio playback error'
  }
  isPlaying.value = false
  stopPlayerMetadataRefresh()
}

function handleAudioPlay() {
  isPlaying.value = true
  startPlayerMetadataRefresh()
}

function handleAudioPause() {
  // Note: We don't set isPlaying to false here because the user might resume
  // Only stop metadata refresh if the audio is actually stopped (not just paused)
}

async function loadUpcomingBlock() {
  if (!nowPlaying.value?.end_event) {
    return
  }

  upcomingLoading.value = true
  upcomingError.value = ''

  try {
    upcomingBlock.value = await fetchBlockByEvent(nowPlaying.value.end_event)
  } catch (e) {
    console.error('Failed to fetch upcoming block:', e)
    upcomingError.value = `Failed to load upcoming block: ${e.message}`
  } finally {
    upcomingLoading.value = false
  }
}

async function lookupBlockById() {
  if (!blockSearchId.value) {
    return
  }

  const eventId = Number(blockSearchId.value)
  if (!Number.isFinite(eventId) || eventId < 0) {
    blockSearchError.value = 'Please enter a valid event ID'
    return
  }

  blockSearchLoading.value = true
  blockSearchError.value = ''

  try {
    blockSearchResult.value = await fetchBlockByEvent(eventId)
  } catch (e) {
    console.error('Failed to fetch block by ID:', e)
    blockSearchError.value = `Failed to load block: ${e.message}`
    blockSearchResult.value = null
  } finally {
    blockSearchLoading.value = false
  }
}

function clearBlockSearch() {
  blockSearchId.value = ''
  blockSearchResult.value = null
  blockSearchError.value = ''
}

// Initialize on mount
onMounted(async () => {
  await fetchChannels()
  await refreshNowPlaying()

  // Auto-refresh every 30 seconds
  refreshTimerId = window.setInterval(() => {
    if (!loading.value) {
      refreshNowPlaying()
    }
  }, 30000)
})

onUnmounted(() => {
  if (refreshTimerId) {
    clearInterval(refreshTimerId)
  }
  if (playerMetadataTimerId) {
    clearInterval(playerMetadataTimerId)
  }
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

.title-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.status-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  font-size: 0.85rem;
  color: #9aa0a6;
}

.status-bar span {
  display: flex;
  align-items: center;
  gap: 6px;
}

.controls {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
  justify-content: flex-end;
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

.btn-play {
  background: rgba(46, 204, 113, 0.2);
  color: #2ecc71;
  border: 1px solid rgba(46, 204, 113, 0.4);
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  font-weight: bold;
  transition: background 0.3s;
}

.btn-play:hover:not(:disabled) {
  background: rgba(46, 204, 113, 0.35);
}

.btn-play:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: rgba(0, 212, 255, 0.12);
  color: #00d4ff;
  border: 1px solid rgba(0, 212, 255, 0.4);
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  font-weight: 600;
  transition: background 0.3s;
}

.btn-secondary:hover:not(:disabled) {
  background: rgba(0, 212, 255, 0.25);
}

.btn-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-tertiary {
  background: transparent;
  color: #bbb;
  border: 1px solid #444;
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  transition: border-color 0.3s, color 0.3s;
}

.btn-tertiary:hover:not(:disabled) {
  color: #fff;
  border-color: #666;
}

.btn-tertiary:disabled {
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

.bitrate-select {
  padding: 8px 12px;
  border-radius: 4px;
  border: 1px solid #333;
  background: #1a1a1a;
  color: #fff;
  cursor: pointer;
}

.bitrate-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 16px;
}

.bitrate-chip {
  padding: 4px 10px;
  border-radius: 999px;
  border: 1px solid #333;
  background: #1c1c1c;
  font-size: 0.8rem;
  color: #bbb;
}

.bitrate-chip.active {
  border-color: #00d4ff;
  color: #00d4ff;
  background: rgba(0, 212, 255, 0.12);
}

.error-message {
  background: #ff4444;
  color: white;
  padding: 12px;
  border-radius: 4px;
  margin-bottom: 20px;
}

.inline-error {
  margin-top: -8px;
  margin-bottom: 16px;
  font-size: 0.9rem;
}

.block-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 16px;
  margin-bottom: 20px;
}

.next-block-info {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.9rem;
  color: #9aa0a6;
}

.next-block-info code {
  background: rgba(255, 255, 255, 0.05);
  padding: 2px 6px;
  border-radius: 4px;
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
.section-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.section-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  color: #9aa0a6;
  font-size: 0.85rem;
}

.section-meta code {
  background: rgba(255, 255, 255, 0.05);
  padding: 2px 6px;
  border-radius: 4px;
}

.summary-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  margin-bottom: 16px;
  color: #9aa0a6;
}

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

.block-search {
  margin: 30px 0;
  padding: 20px;
  border: 1px solid #333;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.35);
}

.search-controls {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-bottom: 12px;
}

.search-controls input {
  padding: 8px 12px;
  border-radius: 4px;
  border: 1px solid #333;
  background: #111;
  color: #fff;
  flex: 1 1 220px;
}

/* Channels Section */
.channels-section {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  border: 1px solid #333;
}

.channel-tracks-section {
  margin-top: 32px;
  padding: 20px;
  border-radius: 8px;
  border: 1px solid #333;
  background: #141414;
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

.section-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
  font-size: 0.85rem;
  color: #9aa0a6;
}

.section-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  align-items: center;
}

.loading-message {
  margin-top: 16px;
  color: #9aa0a6;
}

.empty-placeholder {
  margin-top: 16px;
  padding: 16px;
  border: 1px dashed #444;
  border-radius: 6px;
  color: #9aa0a6;
  text-align: center;
}

.sub-container-notice {
  margin-top: 12px;
  font-size: 0.8rem;
  color: #9aa0a6;
}

.track-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 16px;
  margin-top: 18px;
}

.track-card {
  background: rgba(0, 212, 255, 0.08);
  border: 1px solid rgba(0, 212, 255, 0.2);
  border-radius: 8px;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  transition: border-color 0.2s, box-shadow 0.2s;
}

.status-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 12px;
  margin-top: 12px;
}

.status-card {
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: 8px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.status-label {
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: #9aa0a6;
}

.status-value {
  font-size: 1.05rem;
  font-weight: 600;
  color: #f5f5f5;
}

.track-card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 0.7rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
  color: #bbb;
}

.status-badge.cached {
  background: rgba(46, 204, 113, 0.18);
  border-color: rgba(46, 204, 113, 0.45);
  color: #2ecc71;
}

.status-badge.caching {
  background: rgba(255, 193, 7, 0.15);
  border-color: rgba(255, 193, 7, 0.4);
  color: #ffc107;
}

.status-badge.failed {
  background: rgba(255, 87, 34, 0.18);
  border-color: rgba(255, 87, 34, 0.5);
  color: #ff7043;
}

.status-badge.pending {
  background: rgba(0, 212, 255, 0.12);
  border-color: rgba(0, 212, 255, 0.4);
  color: #00d4ff;
}

.track-metadata {
  font-size: 0.7rem;
  color: #666;
}

.track-card.active {
  border-color: rgba(46, 204, 113, 0.8);
  box-shadow: 0 0 12px rgba(46, 204, 113, 0.25);
}

.track-headline {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.track-title {
  font-weight: 600;
  color: #f5f5f5;
}

.track-artist {
  color: #9aa0a6;
  font-size: 0.9rem;
}

.track-meta {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  font-size: 0.85rem;
  color: #9aa0a6;
}

.track-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
}

.inline-error {
  margin-top: 6px;
  font-size: 0.8rem;
  color: #ff6b6b;
}

.inline-success {
  margin-top: 6px;
  font-size: 0.8rem;
  color: #2ecc71;
}

.formats-list {
  margin-top: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  padding-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.8rem;
  color: #9aa0a6;
}

.format-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.history-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 12px;
}

.history-item {
  padding: 12px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.25);
  border: 1px solid rgba(255, 255, 255, 0.05);
}

.history-title {
  font-weight: 600;
  color: #f5f5f5;
}

.history-meta {
  margin-top: 4px;
  font-size: 0.8rem;
  color: #9aa0a6;
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

/* Enhanced Media Player Section */
.enhanced-player-section {
  background: linear-gradient(135deg, #1a1a2e 0%, #0f0f1e 100%);
  border-radius: 12px;
  padding: 24px;
  margin: 20px 0 32px;
  border: 2px solid rgba(0, 212, 255, 0.4);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}

.player-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
  padding-bottom: 16px;
  border-bottom: 1px solid rgba(0, 212, 255, 0.2);
}

.player-header h3 {
  margin: 0;
  color: #00d4ff;
  font-size: 1.3rem;
}

.player-controls-header {
  display: flex;
  gap: 10px;
  align-items: center;
}

.btn-refresh-meta {
  background: rgba(0, 212, 255, 0.15);
  border: 1px solid rgba(0, 212, 255, 0.4);
  color: #00d4ff;
  padding: 8px 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 1.1rem;
  transition: all 0.2s;
  min-width: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.btn-refresh-meta:hover:not(:disabled) {
  background: rgba(0, 212, 255, 0.25);
  border-color: rgba(0, 212, 255, 0.6);
}

.btn-refresh-meta:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-stop {
  padding: 8px 16px;
  border-radius: 6px;
  border: 1px solid rgba(231, 76, 60, 0.4);
  cursor: pointer;
  background: rgba(231, 76, 60, 0.15);
  color: #e74c3c;
  font-weight: bold;
  transition: all 0.2s;
}

.btn-stop:hover {
  background: rgba(231, 76, 60, 0.25);
  border-color: rgba(231, 76, 60, 0.6);
}

.audio-error-banner {
  background: rgba(255, 68, 68, 0.15);
  border: 1px solid rgba(255, 68, 68, 0.4);
  color: #ff6b6b;
  padding: 16px;
  border-radius: 8px;
  margin-bottom: 16px;
  font-weight: 500;
}

.player-content {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.player-info {
  display: flex;
  gap: 24px;
  align-items: flex-start;
}

.player-cover {
  flex-shrink: 0;
}

.player-cover img {
  width: 180px;
  height: 180px;
  object-fit: cover;
  border-radius: 12px;
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.6);
  border: 2px solid rgba(0, 212, 255, 0.2);
}

.player-cover-placeholder {
  width: 180px;
  height: 180px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #2a2a3e 0%, #1a1a2e 100%);
  border-radius: 12px;
  border: 2px dashed rgba(0, 212, 255, 0.3);
  font-size: 4rem;
  color: rgba(0, 212, 255, 0.3);
}

.player-metadata {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 8px;
  min-width: 0;
}

.metadata-display {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.player-title {
  font-size: 1.8rem;
  font-weight: bold;
  color: #ffffff;
  line-height: 1.2;
  word-wrap: break-word;
}

.player-artist {
  font-size: 1.4rem;
  color: #00d4ff;
  font-weight: 600;
  line-height: 1.3;
}

.player-album {
  font-size: 1.1rem;
  color: #9aa0a6;
  font-style: italic;
}

.player-year {
  font-size: 0.95rem;
  color: #666;
  background: rgba(255, 255, 255, 0.05);
  padding: 4px 10px;
  border-radius: 4px;
  display: inline-block;
  align-self: flex-start;
  margin-top: 4px;
}

.metadata-loading {
  color: #9aa0a6;
  font-size: 1rem;
  font-style: italic;
}

.player-audio-controls {
  width: 100%;
}

.player-audio-controls audio {
  width: 100%;
  border-radius: 8px;
  background: #0a0a0a;
  outline: none;
}

.player-stream-info {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
  padding: 16px;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 8px;
  border: 1px solid rgba(0, 212, 255, 0.15);
}

.stream-info-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.info-label {
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #9aa0a6;
  font-weight: 600;
}

.info-value {
  font-size: 1rem;
  color: #f5f5f5;
  font-weight: 500;
}

/* Stream Controls Section */
.stream-controls-section {
  background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid rgba(0, 212, 255, 0.3);
}

.stream-controls-section h3 {
  margin-top: 0;
  color: #00d4ff;
  margin-bottom: 16px;
}

.stream-controls-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 20px;
}

.control-group {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.control-group label {
  font-weight: 600;
  color: #9aa0a6;
  font-size: 0.9rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.format-chips,
.mode-chips {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.format-chip,
.mode-chip {
  padding: 10px 18px;
  border-radius: 6px;
  border: 2px solid rgba(0, 212, 255, 0.3);
  background: rgba(0, 212, 255, 0.08);
  color: #00d4ff;
  cursor: pointer;
  font-weight: 600;
  transition: all 0.2s;
  font-size: 0.9rem;
}

.format-chip:hover,
.mode-chip:hover {
  background: rgba(0, 212, 255, 0.15);
  border-color: rgba(0, 212, 255, 0.5);
  transform: translateY(-2px);
}

.format-chip.active,
.mode-chip.active {
  background: rgba(0, 212, 255, 0.25);
  border-color: #00d4ff;
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.3);
}

.format-description {
  padding: 8px 12px;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 4px;
  font-size: 0.85rem;
  color: #9aa0a6;
  font-style: italic;
}

.client-id-input {
  padding: 10px 12px;
  border-radius: 6px;
  border: 1px solid #333;
  background: #0f0f0f;
  color: #fff;
  font-family: 'Courier New', monospace;
  font-size: 0.9rem;
}

.client-id-input:focus {
  outline: none;
  border-color: #00d4ff;
  box-shadow: 0 0 8px rgba(0, 212, 255, 0.2);
}

/* Stream Diagnostics Section */
.stream-diagnostics-section {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid #333;
}

.stream-diagnostics-section h3 {
  margin-top: 0;
  color: #00d4ff;
}

/* Stream Metadata Section */
.stream-metadata-section {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid #333;
}

.stream-metadata-section h3 {
  margin-top: 0;
  color: #00d4ff;
}

.metadata-content {
  margin-top: 12px;
}

.metadata-json {
  background: #0f0f0f;
  border: 1px solid #333;
  border-radius: 6px;
  padding: 16px;
  overflow-x: auto;
  font-family: 'Courier New', monospace;
  font-size: 0.85rem;
  color: #00d4ff;
  line-height: 1.5;
  max-height: 400px;
  overflow-y: auto;
}

.metadata-json::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

.metadata-json::-webkit-scrollbar-track {
  background: #1a1a1a;
  border-radius: 4px;
}

.metadata-json::-webkit-scrollbar-thumb {
  background: #333;
  border-radius: 4px;
}

.metadata-json::-webkit-scrollbar-thumb:hover {
  background: #555;
}

/* Quick Test Section */
.quick-test-section {
  background: linear-gradient(135deg, #2a1a1a 0%, #1a1a1a 100%);
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 20px;
  border: 1px solid rgba(255, 193, 7, 0.3);
}

.quick-test-section h3 {
  margin-top: 0;
  color: #ffc107;
  margin-bottom: 8px;
}

.section-description {
  color: #9aa0a6;
  font-size: 0.9rem;
  margin-bottom: 16px;
}

.test-commands {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.command-group {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 193, 7, 0.2);
  border-radius: 6px;
  padding: 12px;
}

.command-label {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.command-label strong {
  color: #ffc107;
  font-size: 0.9rem;
}

.btn-copy-small {
  background: rgba(255, 193, 7, 0.15);
  border: 1px solid rgba(255, 193, 7, 0.3);
  color: #ffc107;
  padding: 4px 10px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.85rem;
  transition: all 0.2s;
}

.btn-copy-small:hover {
  background: rgba(255, 193, 7, 0.25);
  border-color: rgba(255, 193, 7, 0.5);
}

.command-text {
  display: block;
  background: #0a0a0a;
  padding: 10px 12px;
  border-radius: 4px;
  font-family: 'Courier New', monospace;
  font-size: 0.85rem;
  color: #00d4ff;
  overflow-x: auto;
  white-space: nowrap;
  border: 1px solid #222;
}

.command-text::-webkit-scrollbar {
  height: 6px;
}

.command-text::-webkit-scrollbar-track {
  background: #111;
  border-radius: 3px;
}

.command-text::-webkit-scrollbar-thumb {
  background: #333;
  border-radius: 3px;
}

.command-text::-webkit-scrollbar-thumb:hover {
  background: #555;
}

.diagnostics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 12px;
  margin-bottom: 20px;
}

.diagnostic-card {
  background: rgba(0, 212, 255, 0.05);
  border: 1px solid rgba(0, 212, 255, 0.15);
  border-radius: 6px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.diagnostic-label {
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #9aa0a6;
  font-weight: 600;
}

.diagnostic-value {
  font-size: 1rem;
  font-weight: 600;
  color: #f5f5f5;
}

.diagnostic-value code {
  background: rgba(0, 0, 0, 0.4);
  padding: 4px 8px;
  border-radius: 4px;
  font-family: 'Courier New', monospace;
  font-size: 0.85rem;
  color: #00d4ff;
  word-break: break-all;
  display: block;
  margin-top: 4px;
}

.stream-urls-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: rgba(0, 0, 0, 0.3);
  padding: 16px;
  border-radius: 6px;
}

.url-item {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
  padding: 8px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.url-item:last-child {
  border-bottom: none;
}

.url-item strong {
  color: #00d4ff;
  min-width: 120px;
}

@media (max-width: 768px) {
  .header {
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
  }

  .controls {
    width: 100%;
    justify-content: flex-start;
  }

  .block-actions {
    flex-direction: column;
    align-items: flex-start;
  }

  .summary-row {
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .search-controls {
    flex-direction: column;
    align-items: stretch;
  }

  .stream-controls-grid {
    grid-template-columns: 1fr;
  }

  .diagnostics-grid {
    grid-template-columns: 1fr;
  }

  /* Enhanced Player Responsive */
  .player-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
  }

  .player-controls-header {
    width: 100%;
    justify-content: flex-end;
  }

  .player-info {
    flex-direction: column;
    align-items: center;
    text-align: center;
  }

  .player-cover img,
  .player-cover-placeholder {
    width: 150px;
    height: 150px;
  }

  .player-title {
    font-size: 1.4rem;
  }

  .player-artist {
    font-size: 1.1rem;
  }

  .player-album {
    font-size: 1rem;
  }

  .player-stream-info {
    grid-template-columns: 1fr;
  }
}
</style>
