/**
 * Service API pour interagir avec le cache d'images de couvertures
 */

export interface CacheMetadata {
  origin_url?: string;
  [key: string]: unknown;
}

export interface CacheEntry {
  pk: string;
  id: string | null;
  collection?: string | null;
  hits: number;
  last_used: string | null;
  metadata?: CacheMetadata | null;
}

export interface AddImageRequest {
  url: string;
}

export interface AddImageResponse {
  pk: string;
  url: string;
  message: string;
}

export interface ApiError {
  error: string;
  message: string;
}

export interface DownloadStatus {
  pk: string;
  finished: boolean;
  current_size?: number;
  expected_size?: number;
  transformed_size?: number;
}

/**
 * Liste toutes les images en cache
 */
export async function listImages(): Promise<CacheEntry[]> {
  const response = await fetch("/api/covers");
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch images");
  }
  return response.json();
}

/**
 * Récupère les informations d'une image spécifique
 */
export async function getImageInfo(pk: string): Promise<CacheEntry> {
  const response = await fetch(`/api/covers/${pk}`);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to fetch image info");
  }
  return response.json();
}

/**
 * Ajoute une nouvelle image au cache depuis une URL
 */
export async function addImage(url: string): Promise<AddImageResponse> {
  const response = await fetch("/api/covers", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ url }),
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to add image");
  }
  return response.json();
}

/**
 * Supprime une image du cache
 */
export async function deleteImage(pk: string): Promise<void> {
  const response = await fetch(`/api/covers/${pk}`, {
    method: "DELETE",
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to delete image");
  }
}

/**
 * Purge complètement le cache
 */
export async function purgeCache(): Promise<void> {
  const response = await fetch("/api/covers", {
    method: "DELETE",
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to purge cache");
  }
}

/**
 * Consolide le cache (re-télécharge les images manquantes)
 */
export async function consolidateCache(): Promise<void> {
  const response = await fetch("/api/covers/consolidate", {
    method: "POST",
  });

  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to consolidate cache");
  }
}

/**
 * Récupère le statut du téléchargement d'une image
 */
export async function getDownloadStatus(pk: string): Promise<DownloadStatus> {
  const response = await fetch(`/api/covers/${pk}/status`);
  if (!response.ok) {
    const error: ApiError = await response.json();
    throw new Error(error.message || "Failed to get download status");
  }
  return response.json();
}

/**
 * Attend que le téléchargement d'une image soit terminé
 *
 * @param pk - Clé primaire de l'image
 * @param maxWaitMs - Temps maximum d'attente en millisecondes (défaut: 30000)
 * @param pollIntervalMs - Intervalle entre les vérifications en millisecondes (défaut: 500)
 */
export async function waitForDownload(
  pk: string,
  maxWaitMs: number = 30000,
  pollIntervalMs: number = 500
): Promise<void> {
  const startTime = Date.now();

  while (Date.now() - startTime < maxWaitMs) {
    try {
      const status = await getDownloadStatus(pk);
      if (status.finished) {
        return; // Téléchargement terminé
      }
    } catch (error) {
      // Si l'API retourne une erreur, on continue d'attendre
      console.warn(`Error checking download status for ${pk}:`, error);
    }

    // Attendre avant la prochaine vérification
    await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
  }

  // Timeout atteint, on lance une dernière vérification
  const finalStatus = await getDownloadStatus(pk);
  if (!finalStatus.finished) {
    console.warn(`Download timeout for ${pk}, but continuing anyway`);
  }
}

/**
 * Génère l'URL pour afficher une image
 */
export function getOriginUrl(entry: CacheEntry): string | undefined {
  const metadata = entry.metadata;
  if (metadata && typeof metadata === "object") {
    const origin = (metadata as { origin_url?: unknown }).origin_url;
    if (typeof origin === "string" && origin.trim().length > 0) {
      return origin;
    }
  }
  return undefined;
}

export function getImageUrl(pk: string, size?: number): string {
  if (size) {
    return `/covers/image/${pk}/${size}`;
  }
  return `/covers/image/${pk}`;
}

export function getJpegUrl(pk: string, size?: number): string {
  if (size) {
    return `/covers/jpeg/${pk}/${size}`;
  }
  return `/covers/jpeg/${pk}`;
}

/**
 * SVG par défaut pour les images qui ne se chargent pas
 */
const DEFAULT_COVER_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 400">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#667eea;stop-opacity:1" />
      <stop offset="100%" style="stop-color:#764ba2;stop-opacity:1" />
    </linearGradient>
  </defs>
  <rect width="400" height="400" fill="url(#bgGrad)"/>
  <g transform="translate(200, 200)">
    <rect x="-60" y="-80" width="120" height="100" rx="8" fill="white" opacity="0.9"/>
    <circle cx="0" cy="-30" r="18" fill="rgba(102, 126, 234, 0.3)"/>
    <rect x="-40" y="10" width="80" height="8" rx="4" fill="rgba(102, 126, 234, 0.3)"/>
    <rect x="-30" y="30" width="60" height="6" rx="3" fill="rgba(102, 126, 234, 0.2)"/>
    <rect x="-35" y="50" width="70" height="6" rx="3" fill="rgba(102, 126, 234, 0.2)"/>
  </g>
  <text x="200" y="360" text-anchor="middle"
        font-family="system-ui, -apple-system, sans-serif"
        font-size="20" fill="white" opacity="0.6">
    No Image Available
  </text>
</svg>`;

/**
 * Retourne l'URL de l'image par défaut comme data URL
 */
export function getDefaultImageUrl(): string {
  return `data:image/svg+xml;utf8,${encodeURIComponent(DEFAULT_COVER_SVG)}`;
}
