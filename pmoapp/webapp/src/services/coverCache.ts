/**
 * Service API pour interagir avec le cache d'images de couvertures
 */

export interface CacheEntry {
  pk: string;
  source_url: string;
  hits: number;
  last_used: string | null;
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
 * Génère l'URL pour afficher une image
 */
export function getImageUrl(pk: string, size?: number): string {
  if (size) {
    return `/covers/images/${pk}/${size}`;
  }
  return `/covers/images/${pk}`;
}
