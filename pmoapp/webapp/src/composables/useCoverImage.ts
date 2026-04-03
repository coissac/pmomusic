/**
 * Composable pour gérer les images de couvertures
 * 
 * Version optimisée avec:
 * - Cache centralisé pour partager l'état entre composants
 * - Intégration optionnelle avec le cache serveur
 * - Retry automatique avec backoff exponentiel
 * - Cache-busting pour éviter les problèmes de cache navigateur
 */
import { ref, watch, computed, type Ref } from "vue";
import { imageCache, useImageCache } from "./imageCache";
import { simpleHash } from "../utils/string";

export interface CoverImageOptions {
  maxRetries?: number;
  retryDelay?: number;
  forceReload?: boolean;
  useServerCache?: boolean; // Passer par /api/covers pour le caching serveur
}

export function useCoverImage(
  imageUrl: Ref<string | null | undefined>,
  options: CoverImageOptions = {},
) {
  const { 
    maxRetries = 5, 
    retryDelay = 500, 
    forceReload = true,
    useServerCache = true 
  } = options;

  // Configurer le cache global
  imageCache.configure({ maxRetries, retryDelay });

  // État local
  const imageLoaded = ref(false);
  const imageError = ref(false);
  const coverImageRef = ref<HTMLImageElement | null>(null);
  const cacheBustedUrl = ref<string | null>(null);
  const isLoadingNewImage = ref(false);
  
  // Utiliser le cache centralisé pour l'état de chargement
  const cacheEntry = useImageCache(imageUrl);
  
  // Computed: synchroniser avec le cache centralisé
  // Note: on garde le controle local du loaded/error pour éviter les effets de bord
  
  // Génère une URL avec cache-busting
  function getCacheBustedUrl(url: string, retry: number): string {
    if (!forceReload && retry === 0) {
      return url;
    }
    
    // Si on utilise le cache serveur, transformer l'URL
    if (useServerCache && url.startsWith('http')) {
      // L'URL sera transformée côté serveur via le cache
      const separator = url.includes("?") ? "&" : "?";
      const cacheBuster = retry > 0 
        ? `${simpleHash(url)}_r${retry}_${Date.now()}`
        : simpleHash(url);
      return `${url}${separator}_cb=${cacheBuster}`;
    }
    
    // Pour les URLs locales (data: ou /api/), juste ajouter un paramètre de cache-busting
    const separator = url.includes("?") ? "&" : "?";
    const cacheBuster = retry > 0 
      ? `${simpleHash(url)}_r${retry}_${Date.now()}`
      : simpleHash(url);
    return `${url}${separator}_cb=${cacheBuster}`;
  }

  // Gère le chargement réussi
  function handleImageLoad() {
    const url = imageUrl.value;
    if (url) {
      imageCache.markLoaded(url);
    }
    imageLoaded.value = true;
    imageError.value = false;
    isLoadingNewImage.value = false;
  }

  // Gère l'erreur de chargement
  function handleImageError(event: Event) {
    const url = imageUrl.value;
    const img = event.target as HTMLImageElement;
    
    console.warn(`[useCoverImage] Image load error: ${img.src}`);
    
    imageLoaded.value = false;

    if (url) {
      // Demander au cache si on doit réessayer
      if (imageCache.shouldRetry(url)) {
        const delay = imageCache.getRetryDelay(cacheEntry.retryCount.value);
        console.log(`[useCoverImage] Retrying in ${delay}ms...`);
        
        setTimeout(() => {
          // Générer une nouvelle URL avec retry count
          const retry = cacheEntry.retryCount.value;
          cacheBustedUrl.value = getCacheBustedUrl(url, retry);
          
          // Forcer le rechargement de l'image
          if (coverImageRef.value) {
            coverImageRef.value.src = cacheBustedUrl.value;
          }
        }, delay);
      } else {
        imageError.value = true;
        imageCache.markError(url, "Max retries reached");
      }
    } else {
      imageError.value = true;
    }
  }

  // Watch sur l'URL pour générer la cache-busted URL
  watch(
    imageUrl,
    (newUri, oldUri) => {
      // Reset de l'état d'erreur
      imageError.value = false;
      
      // Gestion des transitions
      if (oldUri && newUri && oldUri !== newUri) {
        isLoadingNewImage.value = true;
        // Garder l'image précédente visible pendant le chargement
      } else if (!newUri) {
        imageLoaded.value = false;
        isLoadingNewImage.value = false;
        cacheBustedUrl.value = null;
      } else if (!oldUri && newUri) {
        imageLoaded.value = false;
        isLoadingNewImage.value = true;
      }

      if (newUri) {
        // Indiquer au cache qu'on commence à charger
        imageCache.startLoading(newUri);
        
        // Générer l'URL avec cache-busting
        cacheBustedUrl.value = getCacheBustedUrl(newUri, 0);
      } else {
        cacheBustedUrl.value = null;
      }
    },
    { immediate: true }
  );

  // Callback pour le ref de l'image
  function setImageRef(el: HTMLImageElement | null) {
    coverImageRef.value = el;
    
    // Si on a une URL et une référence, initiate le chargement
    if (el && cacheBustedUrl.value && !imageLoaded.value) {
      // L'image va commencer à charger naturellement via le src
      // Le handler handleImageLoad sera appelé quand terminé
    }
  }

  return {
    // État
    imageLoaded: computed(() => imageLoaded.value || cacheEntry.loaded.value),
    imageError: computed(() => imageError.value || cacheEntry.error.value),
    coverImageRef: ref(coverImageRef),
    cacheBustedUrl,
    isLoadingNewImage,
    
    // Méthodes
    handleImageLoad,
    handleImageError,
    setImageRef,
  };
}

/**
 * Version simplifiée de useCoverImage pour les cas où on n'a pas besoin
 * de tous les options. Utilise le cache centralisé par défaut.
 */
export function useCover(url: Ref<string | null | undefined>) {
  return useCoverImage(url, { forceReload: false, useServerCache: true });
}