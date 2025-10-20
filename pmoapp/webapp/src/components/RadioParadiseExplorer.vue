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
          :disabled="!nowPlaying || !nowPlaying.stream_url"
          class="btn-play"
        >
          {{ isPlaying ? '⏹️ Stop' : '▶️ Play' }}
        </button>
        <select v-model="selectedChannel" @change="changeChannel" class="channel-select">
          <option v-for="channel in channels" :key="channel.id" :value="channel.id">
            {{ channel.name }}
          </option>
        </select>
        <select
          v-if="bitrates.length"
          v-model="selectedBitrate"
          @change="changeBitrate"
          class="bitrate-select"
        >
          <option v-for="bitrate in bitrates" :key="bitrate.id" :value="bitrate.id">
            {{ bitrate.name }}
          </option>
        </select>
      </div>
    </div>

    <div v-if="bitrates.length" class="bitrate-tags">
      <span
        v-for="bitrate in bitrates"
        :key="`chip-${bitrate.id}`"
        :class="['bitrate-chip', { active: selectedBitrate === bitrate.id }]"
      >
        {{ bitrate.name }}
      </span>
    </div>

    <div v-if="bitratesError" class="error-message inline-error">
      ❌ {{ bitratesError }}
    </div>

    <!-- Audio Player -->
    <div v-if="isPlaying || audioError" class="audio-player-container">
      <audio
        v-if="!audioError"
        ref="audioPlayer"
        controls
        @ended="handleAudioEnded"
        @error="handleAudioError"
      ></audio>
      <p v-if="audioError" class="audio-error">{{ audioError }}</p>
      <button @click="stopPlayback" class="btn-stop">
        {{ audioError ? '✕ Close' : '⏹️ Stop' }}
      </button>
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

    <div class="channel-tracks-section">
      <div class="section-header">
        <h3>🎧 Channel Tracks (Source API)</h3>
        <button class="btn-secondary" @click="fetchChannelTracks" :disabled="channelBrowseLoading">
          <span v-if="channelBrowseLoading">⏳ Refreshing…</span>
          <span v-else>Refresh Tracks</span>
        </button>
      </div>
      <div class="section-meta">
        <span>Object:</span>
        <code>{{ channelBrowse.object_id || channelObjectId(selectedChannel) }}</code>
        <span>Items:</span>
        <strong>{{ channelBrowse.returned_items }}</strong>
        <span v-if="channelBrowse.total">/ {{ channelBrowse.total }}</span>
        <span v-if="channelBrowse.update_id">Update ID: {{ channelBrowse.update_id }}</span>
        <span v-if="channelBrowseLastUpdated">
          Last refresh: {{ formatTimestamp(channelBrowseLastUpdated) }}
        </span>
      </div>
      <div v-if="channelBrowseError" class="error-message">
        ❌ {{ channelBrowseError }}
      </div>
      <div v-else-if="channelBrowseLoading" class="loading-message">
        ⏳ Loading channel tracks…
      </div>
      <div v-else>
        <div v-if="channelBrowse.containers.length" class="sub-container-notice">
          {{ channelBrowse.containers.length }} sub container(s) available.
        </div>
        <div v-if="channelBrowse.items.length" class="track-grid">
          <div
            v-for="item in channelBrowse.items"
            :key="item.id"
            :class="['track-card', { active: activeTrackId === item.id }]"
          >
            <div class="track-card-header">
              <span :class="trackStatusClass(item.__status)">
                {{ trackStatusLabel(item.__status) }}
              </span>
              <span v-if="item.update_id" class="track-metadata">update {{ item.update_id }}</span>
            </div>
            <div class="track-headline">
              <div class="track-title">{{ item.title }}</div>
              <div class="track-artist">{{ item.artist || item.creator || 'Unknown artist' }}</div>
            </div>
            <div class="track-meta">
              <span v-if="item.album">{{ item.album }}</span>
              <span v-if="item.resources && item.resources.length && item.resources[0].duration">
                ⏱ {{ item.resources[0].duration }}
              </span>
            </div>
            <div class="track-actions">
              <button class="btn-secondary" @click="playTrackItem(item)">▶️ Play Track</button>
              <a
                v-for="resource in item.resources"
                :key="resource.url"
                :href="resource.url"
                target="_blank"
                class="stream-link"
              >
                Open resource
              </a>
            </div>
          </div>
        </div>
        <div v-else class="empty-placeholder">
          No cached tracks yet for this channel. Refresh after playback starts.
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
import { ref, onMounted, onUnmounted, nextTick } from 'vue'

const API_BASE = '/api/radioparadise'
const SOURCE_API_BASE = '/api/sources'
const SOURCE_ID = 'radio-paradise'

const loading = ref(false)
const error = ref(null)
const nowPlaying = ref(null)
const channels = ref([])
const bitrates = ref([])
const selectedChannel = ref(0)
const selectedBitrate = ref(null)
const audioPlayer = ref(null)
const isPlaying = ref(false)
const audioError = ref('')
const activeTrackId = ref(null)
const lastUpdated = ref(null)
const upcomingBlock = ref(null)
const upcomingLoading = ref(false)
const upcomingError = ref('')
const blockSearchId = ref('')
const blockSearchResult = ref(null)
const blockSearchLoading = ref(false)
const blockSearchError = ref('')
const channelsError = ref('')
const bitratesError = ref('')
const channelBrowse = ref({
  object_id: '',
  containers: [],
  items: [],
  returned_containers: 0,
  returned_items: 0,
  total: 0,
  update_id: 0
})
const channelBrowseLoading = ref(false)
const channelBrowseError = ref('')
const channelBrowseLastUpdated = ref(null)
let refreshTimerId = null
let channelRefreshTimerId = null
const CHANNEL_TRACKS_REFRESH_INTERVAL = 5000
let channelTracksInFlight = false

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
  if (selectedBitrate.value != null) {
    params.set('bitrate', selectedBitrate.value.toString())
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

function channelObjectId(channelId) {
  return `${SOURCE_ID}:channel:${channelId}`
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
      activeTrackId.value = null
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

    if (isPlaying.value && !activeTrackId.value) {
      playStream()
    }
  } catch (e) {
    error.value = `Failed to fetch now playing: ${e.message}`
    console.error('Error fetching now playing:', e)
  } finally {
    loading.value = false
  }
}

function deriveTrackStatus(item) {
  const url = item?.resources?.[0]?.url || ''
  if (!url) {
    return 'pending'
  }
  if (url.includes('/audio/flac/') && !url.includes('#')) {
    return 'cached'
  }
  if (url.includes('#')) {
    return 'downloading'
  }
  return 'external'
}

function trackStatusLabel(status) {
  switch (status) {
    case 'cached':
      return 'Cached'
    case 'downloading':
      return 'Downloading'
    case 'external':
      return 'External'
    default:
      return 'Pending'
  }
}

function trackStatusClass(status) {
  return {
    'status-badge': true,
    cached: status === 'cached',
    downloading: status === 'downloading',
    external: status === 'external',
    pending: status === 'pending'
  }
}

async function fetchChannelTracks(options = {}) {
  const { silent = false } = options
  if (channelTracksInFlight) {
    return
  }
  channelTracksInFlight = true
  if (!silent) {
    channelBrowseLoading.value = true
  }
  channelBrowseError.value = ''

  try {
    const params = new URLSearchParams()
    params.set('object_id', channelObjectId(selectedChannel.value))
    params.set('requested_count', '0')

    const response = await fetch(`${SOURCE_API_BASE}/${SOURCE_ID}/browse?${params.toString()}`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }

    const data = await response.json()
    channelBrowse.value = {
      object_id: data.object_id || channelObjectId(selectedChannel.value),
      containers: data.containers ?? [],
      items: data.items ?? [],
      returned_containers: data.returned_containers ?? (data.containers?.length ?? 0),
      returned_items: data.returned_items ?? (data.items?.length ?? 0),
      total: data.total ?? ((data.containers?.length ?? 0) + (data.items?.length ?? 0)),
      update_id: data.update_id ?? 0
    }
    // annotate each item with status for UI
    channelBrowse.value.items = channelBrowse.value.items.map((item) => ({
      ...item,
      __status: deriveTrackStatus(item)
    }))
    channelBrowseLastUpdated.value = new Date()
  } catch (e) {
    channelBrowseError.value = `Failed to load channel tracks: ${e.message}`
    console.error('Error fetching channel tracks:', e)
  } finally {
    channelTracksInFlight = false
    if (!silent) {
      channelBrowseLoading.value = false
    }
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

// Fetch available bitrates
async function fetchBitrates() {
  try {
    const response = await fetch(`${API_BASE}/bitrates`)
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }
    const data = await response.json()
    bitrates.value = data
    bitratesError.value = ''

    if (bitrates.value.length > 0 && selectedBitrate.value === null) {
      const flac = bitrates.value.find(bitrate => /flac/i.test(bitrate.name))
      selectedBitrate.value = (flac || bitrates.value[0]).id
    }
  } catch (e) {
    bitratesError.value = `Failed to fetch bitrates: ${e.message}`
    console.error('Error fetching bitrates:', e)
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
  await fetchChannelTracks()
  blockSearchResult.value = null
  blockSearchError.value = ''
}

async function changeBitrate() {
  await refreshNowPlaying()
  await fetchChannelTracks()
  blockSearchResult.value = null
  blockSearchError.value = ''
}

function playStream() {
  if (!nowPlaying.value?.stream_url) {
    audioError.value = 'No stream URL available'
    isPlaying.value = false
    activeTrackId.value = null
    return
  }

  activeTrackId.value = null
  playAudio(nowPlaying.value.stream_url)
}

function stopPlayback() {
  if (audioPlayer.value) {
    audioPlayer.value.pause()
    audioPlayer.value.currentTime = 0
    audioPlayer.value.src = ''
  }
  isPlaying.value = false
  audioError.value = ''
  activeTrackId.value = null
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
  activeTrackId.value = null
}

function handleAudioError() {
  const audio = audioPlayer.value
  if (audio?.error) {
    audioError.value = `Audio playback error (code ${audio.error.code})`
  } else {
    audioError.value = 'Unknown audio playback error'
  }
  isPlaying.value = false
  activeTrackId.value = null
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

function playTrackItem(item) {
  const resource = item?.resources?.find(res => res.url)
  if (!resource) {
    audioError.value = 'No audio resource available for this track'
    isPlaying.value = false
    return
  }

  activeTrackId.value = item.id
  playAudio(resource.url)
}

// Initialize on mount
onMounted(async () => {
  await fetchChannels()
  await fetchBitrates()
  await refreshNowPlaying()
  await fetchChannelTracks()

  // Auto-refresh every 30 seconds
  refreshTimerId = window.setInterval(() => {
    if (!loading.value) {
      refreshNowPlaying()
    }
  }, 30000)

  // Auto-refresh channel tracks every few seconds
  channelRefreshTimerId = window.setInterval(() => {
    if (!channelBrowseLoading.value) {
      fetchChannelTracks({ silent: true })
    }
  }, CHANNEL_TRACKS_REFRESH_INTERVAL)
})

onUnmounted(() => {
  if (refreshTimerId) {
    clearInterval(refreshTimerId)
  }
  if (channelRefreshTimerId) {
    clearInterval(channelRefreshTimerId)
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

.status-badge.downloading {
  background: rgba(255, 193, 7, 0.15);
  border-color: rgba(255, 193, 7, 0.4);
  color: #ffc107;
}

.status-badge.external {
  background: rgba(0, 212, 255, 0.12);
  border-color: rgba(0, 212, 255, 0.4);
  color: #00d4ff;
}

.status-badge.pending {
  background: rgba(255, 87, 34, 0.15);
  border-color: rgba(255, 87, 34, 0.4);
  color: #ff6d3a;
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

.audio-player-container {
  margin: 12px 0 24px;
  padding: 16px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.2);
  border: 1px solid #333;
  display: flex;
  gap: 12px;
  align-items: center;
}

.audio-error {
  margin: 0;
  color: #ff6b6b;
  flex: 1;
}

.btn-stop {
  padding: 8px 16px;
  border-radius: 4px;
  border: none;
  cursor: pointer;
  background: rgba(231, 76, 60, 0.2);
  color: #e74c3c;
  font-weight: bold;
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
}
</style>
