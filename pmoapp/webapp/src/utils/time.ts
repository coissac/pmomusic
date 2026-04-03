/**
 * Utilitaires pour les dates et durées
 */

/**
 * Convertit une durée au format HH:MM:SS en millisecondes
 * @param time - Durée au format "HH:MM:SS" ou "MM:SS"
 * @returns Durée en millisecondes, ou null si invalide
 */
export function parseTimeToMs(time: string | null | undefined): number | null {
  if (!time) return null;

  const parts = time.split(':').map(Number);
  
  if (parts.length === 3) {
    // HH:MM:SS
    const hours = parts[0] ?? 0;
    const minutes = parts[1] ?? 0;
    const seconds = parts[2] ?? 0;
    if (isNaN(hours) || isNaN(minutes) || isNaN(seconds)) return null;
    return (hours * 3600 + minutes * 60 + seconds) * 1000;
  } else if (parts.length === 2) {
    // MM:SS
    const minutes = parts[0] ?? 0;
    const seconds = parts[1] ?? 0;
    if (isNaN(minutes) || isNaN(seconds)) return null;
    return (minutes * 60 + seconds) * 1000;
  }

  return null;
}

/**
 * Convertit des millisecondes en format HH:MM:SS
 * @param ms - Durée en millisecondes
 * @returns Durée au format "HH:MM:SS" ou "MM:SS"
 */
export function formatMsToTime(ms: number | null): string {
  if (ms === null || ms === undefined || ms < 0) return '--:--';

  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  const h = hours > 0 ? `${hours}:` : '';
  const m = `${minutes.toString().padStart(2, '0')}:`;
  const s = seconds.toString().padStart(2, '0');
  
  return `${h}${m}${s}`;
}

/**
 * Convertit des millisecondes en format court (pour l'affichage progress)
 * @param ms - Durée en millisecondes
 * @returns Durée au format "X:XX" ou "X:XX:XX"
 */
export function formatMsToShortTime(ms: number | null): string {
  return formatMsToTime(ms);
}