export interface PlaylistSummary {
  id: string;
  title: string;
  role: string;
  persistent: boolean;
  track_count: number;
  max_size?: number | null;
  default_ttl_secs?: number | null;
  last_change: string;
}

export interface PlaylistTrack {
  cache_pk: string;
  added_at: string;
  ttl_secs?: number | null;
}

export interface PlaylistDetail {
  summary: PlaylistSummary;
  tracks: PlaylistTrack[];
}

export interface ApiErrorPayload {
  error?: string;
  message?: string;
}

export interface CreatePlaylistPayload {
  id: string;
  title?: string;
  role?: string;
  persistent?: boolean;
  max_size?: number;
  default_ttl_secs?: number;
}

export interface UpdatePlaylistPayload {
  title?: string;
  role?: string;
  max_size?: number | null;
  default_ttl_secs?: number | null;
}

export interface AddTracksPayload {
  cache_pks: string[];
  ttl_secs?: number;
  lazy?: boolean;
}

async function parseJsonOrThrow<T>(response: Response): Promise<T> {
  if (!response.ok) {
    let message = `HTTP ${response.status}`;
    try {
      const error: ApiErrorPayload = await response.json();
      if (error?.message) {
        message = error.message;
      }
    } catch {
      // Ignore JSON parsing errors and keep default message
    }
    throw new Error(message);
  }

  return response.json() as Promise<T>;
}

async function ensureSuccess(response: Response): Promise<void> {
  if (!response.ok) {
    let message = `HTTP ${response.status}`;
    try {
      const error: ApiErrorPayload = await response.json();
      if (error?.message) {
        message = error.message;
      }
    } catch {
      // ignore
    }
    throw new Error(message);
  }
}

export async function listPlaylists(): Promise<PlaylistSummary[]> {
  const response = await fetch("/api/playlists");
  return parseJsonOrThrow(response);
}

export async function getPlaylistDetail(id: string): Promise<PlaylistDetail> {
  const response = await fetch(`/api/playlists/${encodeURIComponent(id)}`);
  return parseJsonOrThrow(response);
}

export async function createPlaylist(body: CreatePlaylistPayload): Promise<PlaylistDetail> {
  const response = await fetch("/api/playlists", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return parseJsonOrThrow(response);
}

export async function updatePlaylist(
  id: string,
  body: UpdatePlaylistPayload
): Promise<PlaylistDetail> {
  const response = await fetch(`/api/playlists/${encodeURIComponent(id)}`, {
    method: "PATCH",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return parseJsonOrThrow(response);
}

export async function deletePlaylist(id: string): Promise<void> {
  const response = await fetch(`/api/playlists/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
  await ensureSuccess(response);
}

export async function addTracksToPlaylist(
  id: string,
  payload: AddTracksPayload
): Promise<PlaylistDetail> {
  const response = await fetch(`/api/playlists/${encodeURIComponent(id)}/tracks`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  });
  return parseJsonOrThrow(response);
}

export async function flushPlaylist(id: string): Promise<PlaylistDetail> {
  const response = await fetch(`/api/playlists/${encodeURIComponent(id)}/tracks`, {
    method: "DELETE",
  });
  return parseJsonOrThrow(response);
}

export async function removeTrackFromPlaylist(id: string, cachePk: string): Promise<PlaylistDetail> {
  const response = await fetch(
    `/api/playlists/${encodeURIComponent(id)}/tracks/${encodeURIComponent(cachePk)}`,
    {
      method: "DELETE",
    }
  );
  return parseJsonOrThrow(response);
}
