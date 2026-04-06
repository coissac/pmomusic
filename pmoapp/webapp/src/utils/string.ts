/**
 * Utilitaires pour les chaînes de caractères
 */

/**
 * Génère un hash simple et rapide pour une chaîne
 * @param str - Chaîne à hasher
 * @returns Hash sous forme de chaîne hexadécimale positive
 */
export function simpleHash(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = (hash << 5) - hash + char;
    hash = hash & hash; // Convert to 32bit integer
  }
  return Math.abs(hash).toString(36);
}

/**
 * Ajoute un paramètre cache-busting à une URL
 * @param url - URL originale
 * @param cacheKey - Clé de cache (hash ou timestamp)
 * @returns URL avec le paramètre _cb ajouté
 */
export function addCacheBust(url: string, cacheKey: string): string {
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}_cb=${cacheKey}`;
}

/**
 * Nettoie une URL en supprimant les paramètres de cache-busting
 * @param url - URL avec possibly _cb params
 * @returns URL nettoyée
 */
export function normalizeUrl(url: string): string {
  return url.replace(/[?&]_cb=[^&]*/, '');
}

/**
 * Tronque une chaîne à une longueur maximale
 * @param str - Chaîne à tronquer
 * @param maxLength - Longueur maximale
 * @returns Chaîne tronquée avec suffix si nécessaire
 */
export function truncate(str: string, maxLength: number, suffix = '...'): string {
  if (str.length <= maxLength) return str;
  // Guard: si suffix est plus long que maxLength, retourner juste le suffixe
  if (suffix.length >= maxLength) {
    return str.slice(0, maxLength);
  }
  return str.slice(0, maxLength - suffix.length) + suffix;
}