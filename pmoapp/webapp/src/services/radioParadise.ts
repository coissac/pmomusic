/**
 * Service API pour interagir avec Radio Paradise
 */

export interface ChannelInfo {
  id: number;
  name: string;
  description: string;
}

export interface SongInfo {
  index: number;
  artist: string;
  title: string;
  album: string;
  year?: number;
  elapsed_ms: number;
  duration_ms: number;
  cover_url?: string;
  rating?: number;
}

export interface BlockResponse {
  event: number;
  end_event: number;
  url: string;
  length_ms: number;
  songs: SongInfo[];
}

export interface NowPlayingResponse {
  event: number;
  end_event: number;
  stream_url: string;
  block_length_ms: number;
  current_song_index?: number;
  current_song?: SongInfo;
  songs: SongInfo[];
}

export interface StreamUrlResponse {
  event: number;
  stream_url: string;
  length_ms: number;
}

export interface CoverUrlResponse {
  event: number;
  song_index: number;
  cover_url?: string;
  cover_type: string;
}

export interface ApiError {
  error: string;
  message: string;
}

/**
 * Liste tous les canaux disponibles
 */
export async function listChannels(): Promise<ChannelInfo[]> {
  const response = await fetch("/api/radioparadise/channels");
  if (!response.ok) {
    throw new Error("Failed to fetch channels");
  }
  return response.json();
}

/**
 * Récupère le morceau en cours de lecture
 */
export async function getNowPlaying(channel?: number): Promise<NowPlayingResponse> {
  const url = channel !== undefined
    ? `/api/radioparadise/now-playing?channel=${channel}`
    : "/api/radioparadise/now-playing";

  const response = await fetch(url);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch now playing");
  }
  return response.json();
}

/**
 * Récupère le block actuel
 */
export async function getCurrentBlock(channel?: number): Promise<BlockResponse> {
  const url = channel !== undefined
    ? `/api/radioparadise/block/current?channel=${channel}`
    : "/api/radioparadise/block/current";

  const response = await fetch(url);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch current block");
  }
  return response.json();
}

/**
 * Récupère un block spécifique par son event ID
 */
export async function getBlockById(eventId: number, channel?: number): Promise<BlockResponse> {
  const url = channel !== undefined
    ? `/api/radioparadise/block/${eventId}?channel=${channel}`
    : `/api/radioparadise/block/${eventId}`;

  const response = await fetch(url);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch block");
  }
  return response.json();
}

/**
 * Récupère un morceau spécifique d'un block
 */
export async function getSongByIndex(
  eventId: number,
  index: number,
  channel?: number
): Promise<SongInfo> {
  const url = channel !== undefined
    ? `/api/radioparadise/block/${eventId}/song/${index}?channel=${channel}`
    : `/api/radioparadise/block/${eventId}/song/${index}`;

  const response = await fetch(url);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch song");
  }
  return response.json();
}

/**
 * Récupère l'URL de la pochette d'un morceau
 */
export async function getCoverUrl(
  eventId: number,
  songIndex: number,
  channel?: number
): Promise<CoverUrlResponse> {
  const url = channel !== undefined
    ? `/api/radioparadise/cover-url/${eventId}/${songIndex}?channel=${channel}`
    : `/api/radioparadise/cover-url/${eventId}/${songIndex}`;

  const response = await fetch(url);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch cover URL");
  }
  return response.json();
}

/**
 * Récupère l'URL de streaming d'un block
 */
export async function getStreamUrl(
  eventId: number,
  channel?: number
): Promise<StreamUrlResponse> {
  const url = channel !== undefined
    ? `/api/radioparadise/stream-url/${eventId}?channel=${channel}`
    : `/api/radioparadise/stream-url/${eventId}`;

  const response = await fetch(url);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch stream URL");
  }
  return response.json();
}

/**
 * Formate une durée en millisecondes en format MM:SS
 */
export function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/**
 * Formate une durée en millisecondes en format H:MM:SS si >= 1h, sinon MM:SS
 */
export function formatDurationLong(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/**
 * Récupère l'URL de la pochette d'un morceau, avec fallback vers l'image par défaut
 */
export function getSongCoverUrl(song: SongInfo): string | undefined {
  return song.cover_url;
}

/**
 * Retourne le nom complet d'un canal
 */
export function getChannelName(channelId: number): string {
  const channelNames: Record<number, string> = {
    0: "Main Mix",
    1: "Mellow Mix",
    2: "Rock Mix",
    3: "Eclectic Mix",
  };
  return channelNames[channelId] || "Unknown";
}

/**
 * Retourne la description d'un canal
 */
export function getChannelDescription(channelId: number): string {
  const descriptions: Record<number, string> = {
    0: "Eclectic mix of rock, world, electronica, and more",
    1: "Mellower, less aggressive music",
    2: "Heavier, more guitar-driven music",
    3: "Curated worldwide selection",
  };
  return descriptions[channelId] || "";
}
