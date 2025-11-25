/**
 * Service API pour interagir avec le cache de pistes audio
 */

export interface AudioCacheMetadata {
  origin_url?: string;
  title?: string;
  artist?: string;
  album?: string;
  year?: number;
  genre?: string;
  track_number?: number;
  track_total?: number;
  disc_number?: number;
  disc_total?: number;
  duration_ms?: number;
  duration_secs?: number;
  sample_rate?: number;
  bitrate?: number;
  channels?: number;
  conversion?: ConversionInfo;
  cover_pk?: string;
  cover_url?: string;
  [key: string]: unknown;
}

export interface AudioCacheEntry {
  pk: string;
  id: string | null;
  hits: number;
  last_used: string | null;
  collection?: string | null;
  metadata?: AudioCacheMetadata | null;
}

export interface ConversionInfo {
  mode: string;
  input_codec?: string;
  details?: string;
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
  in_progress: boolean;
  finished: boolean;
  current_size?: number;
  transformed_size?: number;
  expected_size?: number;
  error?: string;
  conversion?: ConversionInfo;
}

export interface ApiError {
  error: string;
  message: string;
}

export function getOriginUrl(entry: AudioCacheEntry): string | undefined {
  const metadata = entry.metadata;
  if (metadata && typeof metadata === "object") {
    const origin = (metadata as { origin_url?: unknown }).origin_url;
    if (typeof origin === "string" && origin.trim().length > 0) {
      return origin;
    }
  }
  return undefined;
}

export function getDurationMs(metadata?: AudioCacheMetadata | null): number | undefined {
  if (!metadata) return undefined;
  if (typeof metadata.duration_ms === "number" && !Number.isNaN(metadata.duration_ms)) {
    return metadata.duration_ms;
  }
  if (typeof metadata.duration_secs === "number" && !Number.isNaN(metadata.duration_secs)) {
    return metadata.duration_secs * 1000;
  }
  return undefined;
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

/**
 * Génère l'URL de la cover d'une piste
 * Priorité : cover_pk (cache) > cover_url (externe) > undefined
 */
export function getCoverUrl(metadata?: AudioCacheMetadata | null, size?: number): string | undefined {
  if (!metadata) return undefined;

  // Priorité 1 : cover en cache via cover_pk
  if (metadata.cover_pk) {
    if (size) {
      return `/covers/image/${metadata.cover_pk}/${size}`;
    }
    return `/covers/image/${metadata.cover_pk}`;
  }

  // Priorité 2 : cover externe via cover_url
  if (metadata.cover_url) {
    return metadata.cover_url;
  }

  return undefined;
}
