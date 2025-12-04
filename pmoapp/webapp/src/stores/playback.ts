// Store Pinia pour le playback (métadonnées et positions)
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { TrackMetadata, PositionInfo, RendererEventPayload } from '../services/pmocontrol/types'

export const usePlaybackStore = defineStore('playback', () => {
  // État
  // Map de renderer_id → métadonnées de la piste courante
  const currentTracks = ref<Map<string, TrackMetadata>>(new Map())

  // Map de renderer_id → position actuelle
  const positions = ref<Map<string, PositionInfo>>(new Map())

  // Getters
  const getTrackMetadata = (rendererId: string) => {
    return currentTracks.value.get(rendererId)
  }

  const getPosition = (rendererId: string) => {
    return positions.value.get(rendererId)
  }

  // Actions
  function updateMetadata(rendererId: string, metadata: TrackMetadata) {
    currentTracks.value.set(rendererId, metadata)
  }

  function updatePosition(rendererId: string, position: PositionInfo) {
    positions.value.set(rendererId, position)
  }

  function clearMetadata(rendererId: string) {
    currentTracks.value.delete(rendererId)
  }

  function clearPosition(rendererId: string) {
    positions.value.delete(rendererId)
  }

  // SSE event handling
  function updateFromSSE(event: RendererEventPayload) {
    const rendererId = event.renderer_id

    switch (event.type) {
      case 'metadata_changed': {
        const metadata: TrackMetadata = {
          title: event.title,
          artist: event.artist,
          album: event.album,
          album_art_uri: event.album_art_uri,
          duration_ms: null,
        }
        updateMetadata(rendererId, metadata)
        break
      }

      case 'position_changed': {
        const position: PositionInfo = {
          track: event.track,
          rel_time: event.rel_time,
          track_duration: event.track_duration,
        }
        updatePosition(rendererId, position)
        break
      }

      case 'state_changed': {
        // Si stopped, on peut clear les métadonnées
        if (event.state === 'STOPPED' || event.state === 'NO_MEDIA') {
          clearMetadata(rendererId)
          clearPosition(rendererId)
        }
        break
      }
    }
  }

  return {
    // État
    currentTracks,
    positions,
    // Getters
    getTrackMetadata,
    getPosition,
    // Actions
    updateMetadata,
    updatePosition,
    clearMetadata,
    clearPosition,
    updateFromSSE,
  }
})
