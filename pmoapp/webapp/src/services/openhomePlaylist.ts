import type {
  OpenHomePlaylistAddRequest,
  OpenHomePlaylistSnapshot,
} from '@/services/pmocontrol/types'

const API_BASE = '/api/control'

export async function getOpenHomePlaylist(rendererId: string): Promise<OpenHomePlaylistSnapshot> {
  const resp = await fetch(
    `${API_BASE}/renderers/${encodeURIComponent(rendererId)}/oh/playlist`,
  )
  if (!resp.ok) {
    throw new Error(`Failed to fetch OpenHome playlist: ${resp.status} ${resp.statusText}`)
  }
  return resp.json()
}

export async function clearOpenHomePlaylist(rendererId: string): Promise<void> {
  const resp = await fetch(
    `${API_BASE}/renderers/${encodeURIComponent(rendererId)}/oh/playlist/clear`,
    { method: 'POST' },
  )
  if (!resp.ok) {
    throw new Error(`Failed to clear OpenHome playlist: ${resp.status} ${resp.statusText}`)
  }
}

export async function addOpenHomeTrack(
  rendererId: string,
  payload: OpenHomePlaylistAddRequest,
): Promise<void> {
  const resp = await fetch(
    `${API_BASE}/renderers/${encodeURIComponent(rendererId)}/oh/playlist/add`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    },
  )
  if (!resp.ok) {
    throw new Error(`Failed to add track to OpenHome playlist: ${resp.status} ${resp.statusText}`)
  }
}

export async function playOpenHomeTrack(rendererId: string, trackId: number): Promise<void> {
  const resp = await fetch(
    `${API_BASE}/renderers/${encodeURIComponent(
      rendererId,
    )}/oh/playlist/play/${encodeURIComponent(trackId.toString())}`,
    { method: 'POST' },
  )
  if (!resp.ok) {
    throw new Error(`Failed to play OpenHome track ${trackId}: ${resp.status} ${resp.statusText}`)
  }
}
