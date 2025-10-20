/**
 * Service API pour interagir avec le cache de pistes audio
 */

export interface AudioMetadata {
  title?: string;
  artist?: string;
  album?: string;
  year?: number;
  genre?: string;
  track_number?: number;
  disc_number?: number;
  duration_ms?: number;
  sample_rate?: number;
  bitrate?: number;
  channels?: number;
}

export interface AudioCacheEntry {
  pk: string;
  source_url: string;
  hits: number;
  last_used: string | null;
  collection?: string;
  metadata?: AudioMetadata;
}

export interface AddTrackRequest {
  url: string;
  collection?: string;
}

export interface AddTrackResponse {
  pk: string;
  url: string;
  message: string;
}

export interface DownloadStatus {
  pk: string;
  status: "pending" | "downloading" | "completed" | "failed";
  progress?: number;
  error?: string;
}

export interface ApiError {
  error: string;
  message: string;
}

/**
 * Liste toutes les pistes en cache
 */
export async function listTracks(): Promise<AudioCacheEntry[]> {
  const response = await fetch("/api/audio");
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch tracks");
  }
  return response.json();
}

/**
 * Récupère les informations d'une piste spécifique
 */
export async function getTrackInfo(pk: string): Promise<AudioCacheEntry> {
  const response = await fetch(`/api/audio/${pk}`);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch track info");
  }
  return response.json();
}

/**
 * Récupère le statut de téléchargement d'une piste
 */
export async function getDownloadStatus(pk: string): Promise<DownloadStatus> {
  const response = await fetch(`/api/audio/${pk}/status`);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch download status");
  }
  return response.json();
}

/**
 * Ajoute une nouvelle piste au cache depuis une URL
 */
export async function addTrack(url: string, collection?: string): Promise<AddTrackResponse> {
  const body: AddTrackRequest = { url };
  if (collection) {
    body.collection = collection;
  }

  const response = await fetch("/api/audio", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to add track");
  }
  return response.json();
}

/**
 * Supprime une piste du cache
 */
export async function deleteTrack(pk: string): Promise<void> {
  const response = await fetch(`/api/audio/${pk}`, {
    method: "DELETE",
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to delete track");
  }
}

/**
 * Purge complètement le cache
 */
export async function purgeCache(): Promise<void> {
  const response = await fetch("/api/audio", {
    method: "DELETE",
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to purge cache");
  }
}

/**
 * Consolide le cache (re-télécharge les pistes manquantes)
 */
export async function consolidateCache(): Promise<void> {
  const response = await fetch("/api/audio/consolidate", {
    method: "POST",
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to consolidate cache");
  }
}

/**
 * Génère l'URL pour streamer une piste
 */
export function getTrackUrl(pk: string): string {
  return `/audio/flac/${pk}`;
}

/**
 * Génère l'URL pour télécharger la piste originale
 */
export function getOriginalTrackUrl(pk: string): string {
  return `/audio/flac/${pk}/orig`;
}

/**
 * Formatte la durée en millisecondes au format MM:SS
 */
export function formatDuration(ms?: number): string {
  if (!ms) return "Unknown";
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
}

/**
 * Formatte le bitrate en kbps
 */
export function formatBitrate(bitrate?: number): string {
  if (!bitrate) return "Unknown";
  return `${Math.round(bitrate / 1000)} kbps`;
}

/**
 * Formatte le sample rate en kHz
 */
export function formatSampleRate(sampleRate?: number): string {
  if (!sampleRate) return "Unknown";
  return `${(sampleRate / 1000).toFixed(1)} kHz`;
}
