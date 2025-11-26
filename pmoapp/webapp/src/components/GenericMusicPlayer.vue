<template>
  <div class="music-player">
    <div class="player-header">
      <h1>🎵 PMO Music Player</h1>
      <p class="subtitle">Lecteur générique basé sur l'API pmosource</p>
    </div>

    <!-- Error Message -->
    <div v-if="error" class="error-banner">
      ❌ {{ error }}
    </div>

    <!-- Source Selection -->
    <div class="sources-section">
      <h2>📻 Sources Musicales</h2>
      <div v-if="sourcesLoading" class="loading-message">
        Chargement des sources...
      </div>
      <div v-else-if="sources.length === 0" class="empty-message">
        Aucune source musicale disponible
      </div>
      <div v-else class="sources-grid">
        <div
          v-for="source in sources"
          :key="source.id"
          :class="['source-card', { active: selectedSource?.id === source.id }]"
          @click="selectSource(source)"
        >
          <div class="source-image">
            <img :src="getSourceImageUrl(source.id)" :alt="source.name" />
          </div>
          <div class="source-info">
            <div class="source-name">{{ source.name }}</div>
            <div class="source-capabilities">
              <span v-if="source.supports_fifo" class="capability-badge">FIFO</span>
              <span v-if="source.capabilities.supports_search" class="capability-badge">Search</span>
              <span v-if="source.capabilities.supports_favorites" class="capability-badge">Favorites</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Browser Section (si une source est sélectionnée) -->
    <div v-if="selectedSource" class="browser-section">
      <div class="browser-header">
        <h2>📂 {{ selectedSource.name }}</h2>
        <button @click="refreshBrowser" :disabled="browserLoading" class="btn-refresh">
          <span v-if="browserLoading">⏳</span>
          <span v-else>🔄</span>
          Actualiser
        </button>
      </div>

      <!-- Breadcrumb Navigation -->
      <div class="breadcrumb">
        <button
          v-for="(crumb, index) in breadcrumbs"
          :key="crumb.id"
          @click="navigateToContainer(crumb)"
          :class="['breadcrumb-item', { active: index === breadcrumbs.length - 1 }]"
        >
          {{ crumb.title }}
        </button>
      </div>

      <!-- Browser Loading -->
      <div v-if="browserLoading" class="loading-message">
        Chargement...
      </div>

      <!-- Containers List -->
      <div v-if="browseResult && browseResult.containers.length > 0" class="containers-section">
        <h3>📁 Dossiers</h3>
        <div class="containers-list">
          <div
            v-for="container in browseResult.containers"
            :key="container.id"
            class="container-item"
            @click="navigateIntoContainer(container)"
          >
            <div class="container-icon">📁</div>
            <div class="container-info">
              <div class="container-title">{{ container.title }}</div>
              <div class="container-meta">
                <span v-if="container.child_count">{{ container.child_count }} éléments</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Items List -->
      <div v-if="browseResult && browseResult.items.length > 0" class="items-section">
        <h3>🎵 Morceaux</h3>
        <div class="items-list">
          <div
            v-for="(item, index) in browseResult.items"
            :key="item.id"
            :class="['item-card', {
              active: currentTrack?.id === item.id,
              playing: isPlaying && currentTrack?.id === item.id
            }]"
          >
            <div class="item-number">{{ index + 1 }}</div>
            <div class="item-cover">
              <img
                v-if="item.album_art"
                :src="item.album_art"
                :alt="item.title"
                @error="handleImageError"
              />
              <div v-else class="item-cover-placeholder">🎵</div>
            </div>
            <div class="item-info">
              <div class="item-title">{{ item.title }}</div>
              <div class="item-artist">{{ item.artist || item.creator || 'Artiste inconnu' }}</div>
              <div class="item-album">{{ item.album || '' }}</div>
            </div>
            <div class="item-actions">
              <button
                @click="playTrack(item)"
                :class="['btn-play-item', { active: isPlaying && currentTrack?.id === item.id }]"
              >
                <span v-if="isPlaying && currentTrack?.id === item.id">⏸️</span>
                <span v-else>▶️</span>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Empty State -->
      <div v-if="browseResult && browseResult.total === 0" class="empty-message">
        Ce container est vide
      </div>
    </div>

    <!-- Now Playing Section -->
    <div v-if="currentTrack || audioError" class="now-playing-section">
      <div class="now-playing-header">
        <h2>🎧 Lecture en cours</h2>
        <button v-if="isPlaying" @click="stopPlayback" class="btn-stop">
          ⏹️ Arrêter
        </button>
      </div>

      <div v-if="audioError" class="audio-error">
        ❌ {{ audioError }}
      </div>

      <div v-if="currentTrack" class="now-playing-content">
        <div class="now-playing-cover">
          <img
            v-if="currentTrack.album_art"
            :src="currentTrack.album_art"
            :alt="currentTrack.title"
            @error="handleImageError"
          />
          <div v-else class="now-playing-cover-placeholder">🎵</div>
        </div>
        <div class="now-playing-info">
          <div class="now-playing-title">{{ currentTrack.title }}</div>
          <div class="now-playing-artist">{{ currentTrack.artist || currentTrack.creator || 'Artiste inconnu' }}</div>
          <div class="now-playing-album">{{ currentTrack.album || '' }}</div>
          <div class="now-playing-source">Source: {{ selectedSource?.name }}</div>
        </div>
      </div>

      <!-- Progress Bar -->
      <div v-if="currentTrack" class="progress-container">
        <div class="progress-time">{{ formatTime(currentTime) }}</div>
        <div class="progress-bar">
          <div
            class="progress-fill"
            :style="{ width: duration > 0 ? (currentTime / duration * 100) + '%' : '0%' }"
          ></div>
        </div>
        <div class="progress-time">
          {{ duration > 0 ? formatTime(duration) : formatTime(parseDuration(currentTrack.resources[0]?.duration)) }}
        </div>
      </div>

      <!-- Audio Player -->
      <div class="audio-player-container">
        <audio
          ref="audioPlayer"
          preload="auto"
          @ended="handleAudioEnded"
          @error="handleAudioError"
          @play="handleAudioPlay"
          @pause="handleAudioPause"
          @timeupdate="handleTimeUpdate"
          @loadedmetadata="handleLoadedMetadata"
          @durationchange="handleDurationChange"
          @canplay="handleCanPlay"
        ></audio>
      </div>
    </div>

    <!-- Debug Info -->
    <div v-if="currentTrack" class="debug-info">
      <details>
        <summary>🔧 Informations de debug</summary>
        <div class="debug-content">
          <div><strong>Source ID:</strong> {{ selectedSource?.id }}</div>
          <div><strong>Object ID:</strong> {{ currentTrack.id }}</div>
          <div><strong>Current URI:</strong> {{ currentUri }}</div>
          <div><strong>Track Class:</strong> {{ currentTrack.class }}</div>
        </div>
      </details>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import {
  listSources,
  browseSource,
  resolveUri,
  getSourceImageUrl,
  type SourceInfo,
  type BrowseResponse,
  type BrowseItem,
  type BrowseContainer,
} from '../services/pmosource'

// State
const sources = ref<SourceInfo[]>([])
const sourcesLoading = ref(false)
const selectedSource = ref<SourceInfo | null>(null)
const browseResult = ref<BrowseResponse | null>(null)
const browserLoading = ref(false)
const error = ref<string | null>(null)

// Breadcrumb navigation
interface Breadcrumb {
  id: string
  title: string
}

const breadcrumbs = ref<Breadcrumb[]>([])
const currentObjectId = ref<string | null>(null)

// Audio player state
const currentTrack = ref<BrowseItem | null>(null)
const currentUri = ref<string | null>(null)
const isPlaying = ref(false)
const audioError = ref<string | null>(null)
const audioPlayer = ref<HTMLAudioElement | null>(null)
const currentTime = ref(0)
const duration = ref(0)

// Retry logic for fragile browser decoders
const MAX_AUDIO_RETRIES = 3
const RETRY_DELAY_MS = 800 // Réduit pour des transitions plus rapides
const retryCount = ref(0)
let retryTimer: ReturnType<typeof setTimeout> | null = null

// Metadata refresh via SSE
let metadataEventSource: EventSource | null = null

// Load sources on mount
onMounted(async () => {
  await loadSources()
})

// Cleanup on unmount
onUnmounted(() => {
  stopMetadataRefresh()
  clearRetryTimer()
})

/**
 * Charge la liste des sources disponibles
 */
async function loadSources() {
  sourcesLoading.value = true
  error.value = null

  try {
    const result = await listSources()
    sources.value = result.sources
  } catch (e: any) {
    error.value = `Erreur lors du chargement des sources: ${e.message}`
    console.error('Failed to load sources:', e)
  } finally {
    sourcesLoading.value = false
  }
}

/**
 * Sélectionne une source et charge son contenu racine
 */
async function selectSource(source: SourceInfo) {
  selectedSource.value = source
  currentObjectId.value = null
  breadcrumbs.value = [{ id: source.id, title: source.name }]
  await loadBrowseResult()
}

/**
 * Charge le contenu d'un container
 */
async function loadBrowseResult() {
  if (!selectedSource.value) return

  browserLoading.value = true
  error.value = null

  try {
    browseResult.value = await browseSource(
      selectedSource.value.id,
      currentObjectId.value || undefined
    )
  } catch (e: any) {
    error.value = `Erreur lors de la navigation: ${e.message}`
    console.error('Failed to browse:', e)
  } finally {
    browserLoading.value = false
  }
}

/**
 * Navigue dans un container enfant
 */
async function navigateIntoContainer(container: BrowseContainer) {
  currentObjectId.value = container.id
  breadcrumbs.value.push({
    id: container.id,
    title: container.title,
  })
  await loadBrowseResult()
}

/**
 * Navigue vers un container du breadcrumb
 */
async function navigateToContainer(crumb: Breadcrumb) {
  // Find the index of this crumb
  const index = breadcrumbs.value.findIndex((c) => c.id === crumb.id)
  if (index === -1) return

  // Truncate breadcrumbs to this point
  breadcrumbs.value = breadcrumbs.value.slice(0, index + 1)

  // Set current object ID (root if it's the first breadcrumb)
  if (index === 0) {
    currentObjectId.value = null
  } else {
    currentObjectId.value = crumb.id
  }

  await loadBrowseResult()
}

/**
 * Actualise le contenu courant
 */
async function refreshBrowser() {
  await loadBrowseResult()
}

/**
 * Démarre le stream de métadonnées en temps réel via SSE
 */
function startMetadataRefresh() {
  if (!selectedSource.value || !currentTrack.value) return

  // Stop any existing connection
  stopMetadataRefresh()

  // Build SSE endpoint URL
  const params = new URLSearchParams({ object_id: currentTrack.value.id })
  const sseUrl = `/api/sources/${selectedSource.value.id}/item/stream?${params.toString()}`

  console.log('Connecting to SSE:', sseUrl)

  // Create EventSource connection
  metadataEventSource = new EventSource(sseUrl)

  metadataEventSource.onmessage = (event) => {
    try {
      const previous = currentTrack.value
      const metadata = JSON.parse(event.data)
      if (currentTrack.value && metadata.id === currentTrack.value.id) {
        currentTrack.value = metadata
        console.log('Metadata updated via SSE:', metadata.title)

        // Si le morceau change (live stream), remettre la progression à zéro
        // IMPORTANT: Ne pas recharger l'URI car c'est un flux continu !
        if (hasTrackChanged(previous, metadata)) {
          console.log('Nouveau morceau dans le flux:', metadata.title)
          resetProgressForNewTrack(metadata)
        }

        // Mettre à jour la durée si elle a changé
        const newDuration = metadata.resources?.[0]?.duration
        if (newDuration) {
          const parsedDuration = parseDuration(newDuration)
          if (parsedDuration > 0) {
            duration.value = parsedDuration
          }
        }
      }
    } catch (e: any) {
      console.error('Failed to parse SSE metadata:', e)
    }
  }

  metadataEventSource.onerror = (error) => {
    console.error('SSE connection error:', error)
    // Don't show error to user, SSE will auto-reconnect
  }
}

/**
 * Arrête le stream de métadonnées
 */
function stopMetadataRefresh() {
  if (metadataEventSource) {
    metadataEventSource.close()
    metadataEventSource = null
    console.log('SSE connection closed')
  }
}

/**
 * Joue un morceau
 */
async function playTrack(item: BrowseItem) {
  if (!selectedSource.value) return

  // Si c'est le même morceau en cours de lecture, toggle play/pause
  if (currentTrack.value?.id === item.id && isPlaying.value) {
    audioPlayer.value?.pause()
    return
  }

  resetAudioRetries()
  audioError.value = null
  currentTrack.value = item
  const preferredUri = pickPreferredResourceUri(item)

  // Initialise la durée depuis les métadonnées du morceau si disponible
  currentTime.value = 0
  const trackDuration = item.resources[0]?.duration
  if (trackDuration) {
    duration.value = parseDuration(trackDuration)
  } else {
    duration.value = 0
  }

  try {
    // Résout l'URI du morceau via l'API
    const result = await resolveUri(selectedSource.value.id, item.id)
    let uri = result.uri

    // Si resolveUri n'a pas retourné d'URI, utiliser notre URI préférée (basée sur le support navigateur)
    if (!uri && preferredUri) {
      uri = preferredUri
      console.log('Utilisation de l\'URI préférée (auto-détectée):', uri)
    }

    // Si les deux sont disponibles, privilégier l'URI la mieux supportée
    if (uri && preferredUri && uri !== preferredUri) {
      // Vérifier si l'URI préférée est mieux supportée
      const resolvedFormat = getAudioFormat(uri)
      const preferredFormat = getAudioFormat(preferredUri)

      if (preferredFormat && canPlayAudioType(preferredFormat)) {
        if (!resolvedFormat || !canPlayAudioType(resolvedFormat)) {
          console.log(`Remplacement de ${uri} par ${preferredUri} (meilleur support navigateur)`)
          uri = preferredUri
        }
      }
    }

    if (!uri) {
      throw new Error('Aucune URI audio disponible')
    }

    await startPlaybackFromUri(uri, true)
  } catch (e: any) {
    audioError.value = `Erreur lors de la lecture: ${e.message}`
    console.error('Failed to play track:', e)
  }
}

/**
 * Arrête la lecture
 */
function stopPlayback() {
  stopMetadataRefresh()

  if (audioPlayer.value) {
    audioPlayer.value.pause()
    audioPlayer.value.currentTime = 0
    audioPlayer.value.src = ''
  }
  isPlaying.value = false
  currentTrack.value = null
  currentUri.value = null
  audioError.value = null
  currentTime.value = 0
  duration.value = 0
  resetAudioRetries()
}

/**
 * Gère la fin de la lecture
 */
function handleAudioEnded() {
  isPlaying.value = false
  currentTime.value = 0
  stopMetadataRefresh()
  // On peut implémenter ici une logique de lecture automatique du prochain morceau
}

/**
 * Gère les erreurs audio
 */
function handleAudioError() {
  stopMetadataRefresh()

  const audio = audioPlayer.value
  const errorCode = audio?.error?.code
  const baseMessage = errorCode
    ? `Erreur de lecture audio (code ${errorCode})`
    : 'Erreur de lecture audio inconnue'

  if (selectedSource.value && currentTrack.value && retryCount.value < MAX_AUDIO_RETRIES) {
    isPlaying.value = false
    retryCount.value += 1
    audioError.value = `${baseMessage} - tentative de reprise (${retryCount.value}/${MAX_AUDIO_RETRIES})`
    scheduleRetryPlayback()
    return
  }

  audioError.value = baseMessage
  isPlaying.value = false
}

/**
 * Gère le démarrage de la lecture
 */
function handleAudioPlay() {
  isPlaying.value = true
  audioError.value = null
  startMetadataRefresh()
}

/**
 * Gère la pause
 */
function handleAudioPause() {
  isPlaying.value = false
  // Don't stop refresh on pause, user might resume
}

/**
 * Gère la mise à jour du temps de lecture
 */
function handleTimeUpdate() {
  if (audioPlayer.value) {
    currentTime.value = audioPlayer.value.currentTime
  }
}

/**
 * Gère le chargement des métadonnées audio
 */
function handleLoadedMetadata() {
  if (audioPlayer.value && !isNaN(audioPlayer.value.duration)) {
    duration.value = audioPlayer.value.duration
  }
}

/**
 * Gère le changement de durée
 */
function handleDurationChange() {
  if (audioPlayer.value && !isNaN(audioPlayer.value.duration)) {
    duration.value = audioPlayer.value.duration
  }
}

/**
 * Gère l'événement canplay (audio prêt à être joué)
 */
function handleCanPlay() {
  console.log('Audio ready to play (canplay event)')
}

/**
 * Formatte le temps en MM:SS
 */
function formatTime(seconds: number): string {
  if (!isFinite(seconds)) return '--:--'
  const mins = Math.floor(seconds / 60)
  const secs = Math.floor(seconds % 60)
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

/**
 * Parse une durée au format UPnP (H:MM:SS ou H:MM:SS.mmm) en secondes
 */
function parseDuration(durationStr: string | null | undefined): number {
  if (!durationStr) return 0
  const parts = durationStr.split(':')
  if (parts.length === 3) {
    const hours = parseInt(parts[0] || '0', 10) || 0
    const minutes = parseInt(parts[1] || '0', 10) || 0
    const seconds = parseFloat(parts[2] || '0') || 0
    return hours * 3600 + minutes * 60 + seconds
  }
  return 0
}

/**
 * Gère les erreurs de chargement d'image
 */
function handleImageError(event: Event) {
  const img = event.target as HTMLImageElement
  img.style.display = 'none'
}

/**
 * Détecte un changement de morceau (utile pour les streams live)
 */
function hasTrackChanged(
  previous: BrowseItem | null,
  next: BrowseItem | null
): boolean {
  if (!previous || !next) return false
  return (
    previous.title !== next.title ||
    previous.album !== next.album ||
    (previous.artist || previous.creator) !== (next.artist || next.creator) ||
    previous.album_art !== next.album_art
  )
}

/**
 * Remet la progression à zéro pour un nouveau morceau
 */
function resetProgressForNewTrack(track: BrowseItem | null) {
  currentTime.value = 0
  if (audioPlayer.value) {
    audioPlayer.value.currentTime = 0
  }
  const d = track?.resources?.[0]?.duration
  duration.value = d ? parseDuration(d) : 0
}

/**
 * Détecte si un type MIME audio est supporté par le navigateur
 */
function canPlayAudioType(mimeType: string): boolean {
  const audio = document.createElement('audio')
  const support = audio.canPlayType(mimeType)
  return support === 'probably' || support === 'maybe'
}

/**
 * Extrait le type MIME audio depuis une URL
 */
function getAudioFormat(url: string): string | null {
  const urlLower = url.toLowerCase()

  // Détection par extension ou contenu de l'URL
  if (urlLower.includes('flac') || urlLower.endsWith('.flac')) {
    return 'audio/flac'
  }
  if (urlLower.includes('ogg') || urlLower.endsWith('.ogg') || urlLower.includes('vorbis')) {
    return 'audio/ogg'
  }
  if (urlLower.includes('mp3') || urlLower.endsWith('.mp3') || urlLower.includes('mpeg')) {
    return 'audio/mpeg'
  }
  if (urlLower.includes('wav') || urlLower.endsWith('.wav')) {
    return 'audio/wav'
  }
  if (urlLower.includes('opus')) {
    return 'audio/ogg' // Opus est généralement dans un container OGG
  }

  return null
}

/**
 * Sélectionne l'URI préférée en tenant compte du support navigateur
 */
function pickPreferredResourceUri(item: BrowseItem): string | null {
  const resources = item.resources || []
  if (resources.length === 0) return null

  // Ordre de préférence des formats (du meilleur au plus compatible)
  const preferredFormats = [
    { mime: 'audio/flac', keywords: ['flac'] },
    { mime: 'audio/ogg', keywords: ['ogg', 'vorbis', 'opus'] },
    { mime: 'audio/mpeg', keywords: ['mp3', 'mpeg'] },
    { mime: 'audio/wav', keywords: ['wav'] },
    { mime: 'audio/x-wav', keywords: ['wav'] },
  ]

  // Tester chaque format dans l'ordre de préférence
  for (const format of preferredFormats) {
    // Vérifier d'abord si le navigateur supporte ce format
    if (!canPlayAudioType(format.mime)) {
      console.log(`Format ${format.mime} non supporté par ce navigateur`)
      continue
    }

    // Chercher une ressource correspondant à ce format
    const matchingRes = resources.find((res) => {
      if (!res.url || !res.protocol_info) return false
      const protocolLower = res.protocol_info.toLowerCase()
      return format.keywords.some((keyword) => protocolLower.includes(keyword))
    })

    if (matchingRes?.url) {
      console.log(`Format sélectionné: ${format.mime}`, matchingRes.url)
      return matchingRes.url
    }
  }

  // Fallback: prendre n'importe quelle URL disponible
  const anyRes = resources.find((res) => !!res.url)
  if (anyRes?.url) {
    console.log('Fallback: utilisation de la première URL disponible', anyRes.url)
  }
  return anyRes?.url || null
}

/**
 * Ajoute un cache-buster pour forcer un nouveau stream côté navigateur
 */
function addCacheBuster(uri: string): string {
  const separator = uri.includes('?') ? '&' : '?'
  return `${uri}${separator}ts=${Date.now()}`
}

/**
 * Nettoie le timer de retry
 */
function clearRetryTimer() {
  if (retryTimer) {
    clearTimeout(retryTimer)
    retryTimer = null
  }
}

/**
 * Réinitialise les compteurs de retry
 */
function resetAudioRetries() {
  retryCount.value = 0
  clearRetryTimer()
}

/**
 * Lance un retry différé pour laisser le temps au backend de reproposer le flux
 */
function scheduleRetryPlayback() {
  clearRetryTimer()
  retryTimer = setTimeout(() => {
    retryTimer = null
    void retryPlayback()
  }, RETRY_DELAY_MS)
}

/**
 * (Ré)initialise la source audio et démarre la lecture
 */
async function startPlaybackFromUri(uri: string, waitForDom = false) {
  if (waitForDom) {
    await nextTick()
  }

  const player = audioPlayer.value
  if (!player) {
    throw new Error('Audio player not ready')
  }

  const finalUri = addCacheBuster(uri)
  currentUri.value = finalUri

  player.pause()
  player.src = finalUri
  player.load()

  // Attendre que le navigateur ait bufferisé suffisamment de données
  // pour éviter les gaps audio entre les morceaux
  await waitForSufficientBuffer(player)

  await player.play()

  isPlaying.value = true
  audioError.value = null
  resetAudioRetries()
  startMetadataRefresh()
}

/**
 * Attend que le player ait suffisamment bufferisé avant de lancer la lecture
 */
async function waitForSufficientBuffer(player: HTMLAudioElement): Promise<void> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      cleanup()
      console.warn('Buffer timeout - starting playback anyway')
      resolve()
    }, 5000) // Timeout de 5 secondes max

    const cleanup = () => {
      clearTimeout(timeout)
      player.removeEventListener('canplay', onCanPlay)
      player.removeEventListener('error', onError)
    }

    const onCanPlay = () => {
      // Vérifier si on a au moins 1-2 secondes de buffer
      if (player.buffered.length > 0) {
        const bufferedEnd = player.buffered.end(0)
        const currentTime = player.currentTime
        const bufferedSeconds = bufferedEnd - currentTime

        console.log(`Buffer disponible: ${bufferedSeconds.toFixed(2)}s`)

        // Attendre au moins 0.5 secondes de buffer pour réduire les gaps
        if (bufferedSeconds >= 0.5) {
          cleanup()
          resolve()
        }
      }
    }

    const onError = () => {
      cleanup()
      reject(new Error('Buffer error'))
    }

    player.addEventListener('canplay', onCanPlay)
    player.addEventListener('error', onError)

    // Check immédiatement au cas où c'est déjà prêt
    if (player.readyState >= 3) {
      onCanPlay()
    }
  })
}

/**
 * Tente de relancer la lecture après une erreur (nouvelle résolution d'URI)
 */
async function retryPlayback() {
  if (!selectedSource.value || !currentTrack.value) return

  try {
    const result = await resolveUri(selectedSource.value.id, currentTrack.value.id)
    await startPlaybackFromUri(result.uri)
  } catch (e: any) {
    audioError.value = `Erreur lors de la reprise: ${e.message}`
    isPlaying.value = false
  }
}
</script>

<style scoped>
.music-player {
  padding: 20px;
  max-width: 1400px;
  margin: 0 auto;
}

.player-header {
  text-align: center;
  margin-bottom: 40px;
}

.player-header h1 {
  margin: 0;
  color: #00d4ff;
  font-size: 2.5rem;
}

.subtitle {
  color: #9aa0a6;
  font-size: 1rem;
  margin-top: 8px;
}

.error-banner {
  background: rgba(255, 68, 68, 0.15);
  border: 1px solid rgba(255, 68, 68, 0.4);
  color: #ff6b6b;
  padding: 16px;
  border-radius: 8px;
  margin-bottom: 20px;
}

/* Sources Section */
.sources-section {
  background: #1a1a2e;
  border-radius: 12px;
  padding: 24px;
  margin-bottom: 30px;
  border: 1px solid rgba(0, 212, 255, 0.3);
}

.sources-section h2 {
  margin-top: 0;
  color: #00d4ff;
}

.loading-message {
  color: #9aa0a6;
  text-align: center;
  padding: 20px;
}

.empty-message {
  color: #9aa0a6;
  text-align: center;
  padding: 20px;
  border: 1px dashed #444;
  border-radius: 8px;
}

.sources-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 16px;
}

.source-card {
  background: #2a2a3e;
  border-radius: 8px;
  padding: 16px;
  cursor: pointer;
  transition: all 0.3s;
  border: 2px solid transparent;
  display: flex;
  align-items: center;
  gap: 16px;
}

.source-card:hover {
  background: #333350;
  transform: translateY(-2px);
}

.source-card.active {
  border-color: #00d4ff;
  background: rgba(0, 212, 255, 0.1);
}

.source-image img {
  width: 80px;
  height: 80px;
  object-fit: cover;
  border-radius: 8px;
}

.source-info {
  flex: 1;
}

.source-name {
  font-weight: bold;
  color: #fff;
  font-size: 1.1rem;
  margin-bottom: 8px;
}

.source-capabilities {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.capability-badge {
  background: rgba(0, 212, 255, 0.2);
  color: #00d4ff;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 600;
}

/* Browser Section */
.browser-section {
  background: #1a1a2e;
  border-radius: 12px;
  padding: 24px;
  margin-bottom: 30px;
  border: 1px solid rgba(0, 212, 255, 0.3);
}

.browser-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.browser-header h2 {
  margin: 0;
  color: #00d4ff;
}

.btn-refresh {
  background: rgba(0, 212, 255, 0.15);
  border: 1px solid rgba(0, 212, 255, 0.4);
  color: #00d4ff;
  padding: 8px 16px;
  border-radius: 6px;
  cursor: pointer;
  font-weight: 600;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  gap: 8px;
}

.btn-refresh:hover:not(:disabled) {
  background: rgba(0, 212, 255, 0.25);
}

.btn-refresh:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Breadcrumb */
.breadcrumb {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 20px;
  padding-bottom: 16px;
  border-bottom: 1px solid rgba(0, 212, 255, 0.2);
}

.breadcrumb-item {
  background: rgba(0, 212, 255, 0.1);
  border: 1px solid rgba(0, 212, 255, 0.3);
  color: #00d4ff;
  padding: 6px 12px;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.2s;
}

.breadcrumb-item:hover {
  background: rgba(0, 212, 255, 0.2);
}

.breadcrumb-item.active {
  background: rgba(0, 212, 255, 0.25);
  font-weight: bold;
}

.breadcrumb-item:not(:last-child)::after {
  content: '›';
  margin-left: 8px;
  color: #666;
}

/* Containers */
.containers-section {
  margin-bottom: 30px;
}

.containers-section h3 {
  color: #00d4ff;
  margin-bottom: 12px;
}

.containers-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 12px;
}

.container-item {
  background: rgba(0, 212, 255, 0.05);
  border: 1px solid rgba(0, 212, 255, 0.2);
  border-radius: 8px;
  padding: 14px;
  cursor: pointer;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  gap: 12px;
}

.container-item:hover {
  background: rgba(0, 212, 255, 0.1);
  border-color: rgba(0, 212, 255, 0.4);
}

.container-icon {
  font-size: 2rem;
}

.container-info {
  flex: 1;
}

.container-title {
  font-weight: 600;
  color: #fff;
  margin-bottom: 4px;
}

.container-meta {
  font-size: 0.85rem;
  color: #9aa0a6;
}

/* Items */
.items-section {
  margin-bottom: 30px;
}

.items-section h3 {
  color: #00d4ff;
  margin-bottom: 12px;
}

.items-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.item-card {
  background: #2a2a3e;
  border-radius: 8px;
  padding: 12px;
  display: flex;
  align-items: center;
  gap: 12px;
  transition: all 0.2s;
  border: 2px solid transparent;
}

.item-card:hover {
  background: #333350;
}

.item-card.active {
  border-color: rgba(0, 212, 255, 0.5);
}

.item-card.playing {
  border-color: #00d4ff;
  background: rgba(0, 212, 255, 0.1);
}

.item-number {
  font-weight: bold;
  color: #666;
  min-width: 30px;
  text-align: center;
}

.item-cover {
  width: 60px;
  height: 60px;
  flex-shrink: 0;
}

.item-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 6px;
}

.item-cover-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 212, 255, 0.1);
  border-radius: 6px;
  font-size: 1.5rem;
}

.item-info {
  flex: 1;
  min-width: 0;
}

.item-title {
  font-weight: 600;
  color: #fff;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-bottom: 4px;
}

.item-artist {
  color: #00d4ff;
  font-size: 0.9rem;
  margin-bottom: 2px;
}

.item-album {
  color: #9aa0a6;
  font-size: 0.85rem;
}

.item-actions {
  display: flex;
  gap: 8px;
}

.btn-play-item {
  background: rgba(46, 204, 113, 0.2);
  border: 1px solid rgba(46, 204, 113, 0.4);
  color: #2ecc71;
  padding: 8px 16px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 1.1rem;
  transition: all 0.2s;
  min-width: 50px;
}

.btn-play-item:hover {
  background: rgba(46, 204, 113, 0.3);
}

.btn-play-item.active {
  background: rgba(46, 204, 113, 0.4);
  border-color: #2ecc71;
}

/* Now Playing */
.now-playing-section {
  background: linear-gradient(135deg, #1a1a2e 0%, #0f0f1e 100%);
  border-radius: 12px;
  padding: 24px;
  margin-bottom: 30px;
  border: 2px solid rgba(0, 212, 255, 0.4);
  position: sticky;
  bottom: 20px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}

.now-playing-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.now-playing-header h2 {
  margin: 0;
  color: #00d4ff;
}

.btn-stop {
  background: rgba(231, 76, 60, 0.2);
  border: 1px solid rgba(231, 76, 60, 0.4);
  color: #e74c3c;
  padding: 8px 16px;
  border-radius: 6px;
  cursor: pointer;
  font-weight: 600;
  transition: all 0.2s;
}

.btn-stop:hover {
  background: rgba(231, 76, 60, 0.3);
}

.audio-error {
  background: rgba(255, 68, 68, 0.15);
  border: 1px solid rgba(255, 68, 68, 0.4);
  color: #ff6b6b;
  padding: 12px;
  border-radius: 6px;
  margin-bottom: 16px;
}

.now-playing-content {
  display: flex;
  gap: 20px;
  align-items: center;
  margin-bottom: 16px;
}

.now-playing-cover {
  width: 120px;
  height: 120px;
  flex-shrink: 0;
}

.now-playing-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 8px;
  border: 2px solid rgba(0, 212, 255, 0.3);
}

.now-playing-cover-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 212, 255, 0.1);
  border-radius: 8px;
  font-size: 3rem;
  border: 2px solid rgba(0, 212, 255, 0.3);
}

.now-playing-info {
  flex: 1;
}

.now-playing-title {
  font-size: 1.5rem;
  font-weight: bold;
  color: #fff;
  margin-bottom: 8px;
}

.now-playing-artist {
  font-size: 1.2rem;
  color: #00d4ff;
  margin-bottom: 4px;
}

.now-playing-album {
  font-size: 1rem;
  color: #9aa0a6;
  margin-bottom: 8px;
}

.now-playing-source {
  font-size: 0.85rem;
  color: #666;
}

/* Progress Bar */
.progress-container {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 0;
  width: 100%;
}

.progress-time {
  font-size: 0.85rem;
  color: #9aa0a6;
  font-variant-numeric: tabular-nums;
  min-width: 45px;
  text-align: center;
}

.progress-bar {
  flex: 1;
  height: 6px;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 3px;
  overflow: hidden;
  position: relative;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #00d4ff, #00a8cc);
  border-radius: 3px;
  transition: width 0.1s linear;
  box-shadow: 0 0 10px rgba(0, 212, 255, 0.5);
}

.audio-player-container {
  display: none;
}

/* Debug Info */
.debug-info {
  background: rgba(0, 0, 0, 0.3);
  border-radius: 8px;
  padding: 16px;
  border: 1px solid #333;
}

.debug-info summary {
  cursor: pointer;
  color: #9aa0a6;
  font-weight: 600;
  margin-bottom: 12px;
}

.debug-content {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  font-family: 'Courier New', monospace;
  font-size: 0.85rem;
  color: #9aa0a6;
}

.debug-content strong {
  color: #00d4ff;
}

/* Responsive */
@media (max-width: 768px) {
  .sources-grid {
    grid-template-columns: 1fr;
  }

  .containers-list {
    grid-template-columns: 1fr;
  }

  .now-playing-content {
    flex-direction: column;
    text-align: center;
  }

  .now-playing-cover {
    width: 150px;
    height: 150px;
  }
}
</style>
