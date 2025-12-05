// Store Pinia pour le playback (métadonnées)
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { TrackMetadata, RendererEventPayload } from '../services/pmocontrol/types'

export const usePlaybackStore = defineStore('playback', () => {
  // État
  // Map de renderer_id → métadonnées de la piste courante
  const currentTracks = ref<Map<string, TrackMetadata>>(new Map())

  // Getters
  const getTrackMetadata = (rendererId: string) => {
    return currentTracks.value.get(rendererId)
  }

  // Actions
  function updateMetadata(rendererId: string, metadata: TrackMetadata) {
    currentTracks.value.set(rendererId, metadata)
  }

  function clearMetadata(rendererId: string) {
    currentTracks.value.delete(rendererId)
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

      case 'state_changed': {
        // Si stopped, on peut clear les métadonnées
        if (event.state === 'STOPPED' || event.state === 'NO_MEDIA') {
          clearMetadata(rendererId)
        }
        break
      }
    }
  }

  return {
    // État
    currentTracks,
    // Getters
    getTrackMetadata,
    // Actions
    updateMetadata,
    clearMetadata,
    updateFromSSE,
  }
})
