/**
 * Cache centralisé pour les images de couvertures
 * 
 * Ce service fournit:
 * - Cache mémoire pour les états de chargement (évite les requêtes doubles)
 * - Intégration avec le cache serveur (/api/covers)
 * - Gestion centralisée des retries
 * - Subscription aux changements d'état (plusieurs composants partagent le même état)
 */

import { ref, computed, onUnmounted, watch, type Ref } from 'vue';

// Types pour le cache
export interface ImageCacheEntry {
  url: string;
  loaded: boolean;
  error: boolean;
  loading: boolean;
  retryCount: number;
  lastError: string | null;
}

export interface ImageCacheOptions {
  maxRetries?: number;
  retryDelay?: number;
  useServerCache?: boolean;
}

// Singleton - état du cache global
class ImageCacheService {
  private cache = new Map<string, ImageCacheEntry>();
  private subscriptions = new Map<string, Set<(entry: ImageCacheEntry) => void>>();
  private options: ImageCacheOptions = {
    maxRetries: 5,
    retryDelay: 500,
    useServerCache: true,
  };

  private readonly CACHE_CLEANUP_MS = 5 * 60 * 1000;

  constructor() {
    setInterval(() => this.cleanup(), this.CACHE_CLEANUP_MS);
  }

  configure(options: Partial<ImageCacheOptions>) {
    this.options = { ...this.options, ...options };
  }

  getOrCreate(url: string | null | undefined): ImageCacheEntry | null {
    if (!url) return null;

    const normalizedUrl = url.replace(/[?&]_cb=[^&]*/, '');
    const cacheKey = normalizedUrl;

    if (!this.cache.has(cacheKey)) {
      this.cache.set(cacheKey, {
        url: normalizedUrl,
        loaded: false,
        error: false,
        loading: false,
        retryCount: 0,
        lastError: null,
      });
    }

    return this.cache.get(cacheKey)!;
  }

  subscribe(url: string | null | undefined, callback: (entry: ImageCacheEntry) => void): () => void {
    const entry = this.getOrCreate(url);
    if (!entry) return () => {};

    const normalizedUrl = entry.url;
    
    if (!this.subscriptions.has(normalizedUrl)) {
      this.subscriptions.set(normalizedUrl, new Set());
    }
    
    this.subscriptions.get(normalizedUrl)!.add(callback);
    callback(entry);

    return () => {
      const subs = this.subscriptions.get(normalizedUrl);
      if (subs) {
        subs.delete(callback);
        if (subs.size === 0) {
          this.subscriptions.delete(normalizedUrl);
        }
      }
    };
  }

  startLoading(url: string | null | undefined) {
    const entry = this.getOrCreate(url);
    if (!entry) return;

    entry.loading = true;
    this.notifySubscribers(entry.url);
  }

  markLoaded(url: string | null | undefined) {
    const entry = this.getOrCreate(url);
    if (!entry) return;

    entry.loaded = true;
    entry.error = false;
    entry.loading = false;
    entry.retryCount = 0;
    entry.lastError = null;
    this.notifySubscribers(entry.url);
  }

  markError(url: string | null | undefined, error: string) {
    const entry = this.getOrCreate(url);
    if (!entry) return;

    entry.error = true;
    entry.loading = false;
    entry.lastError = error;
    this.notifySubscribers(entry.url);
  }

  shouldRetry(url: string | null | undefined): boolean {
    const entry = this.getOrCreate(url);
    if (!entry) return false;

    if (entry.retryCount >= (this.options.maxRetries ?? 5)) {
      return false;
    }

    entry.retryCount++;
    entry.loading = true;
    this.notifySubscribers(entry.url);
    return true;
  }

  getRetryDelay(retryCount: number): number {
    const baseDelay = this.options.retryDelay ?? 500;
    return baseDelay * Math.pow(2, retryCount - 1);
  }

  private notifySubscribers(url: string) {
    const entry = this.cache.get(url);
    if (!entry) return;

    const subs = this.subscriptions.get(url);
    if (subs) {
      subs.forEach(callback => {
        try {
          callback(entry);
        } catch (e) {
          console.error('[ImageCache] Error in subscriber:', e);
        }
      });
    }
  }

  private cleanup() {
    const toDelete: string[] = [];
    
    this.cache.forEach((entry, url) => {
      const hasSubs = this.subscriptions.has(url);
      if (!hasSubs && entry.loaded) {
        toDelete.push(url);
      }
    });

    toDelete.forEach(url => this.cache.delete(url));
  }

  getStats() {
    let loaded = 0;
    let loading = 0;
    let error = 0;
    let pending = 0;

    this.cache.forEach(entry => {
      if (entry.loaded) loaded++;
      else if (entry.loading) loading++;
      else if (entry.error) error++;
      else pending++;
    });

    return {
      total: this.cache.size,
      loaded,
      loading,
      error,
      pending,
      subscribers: this.subscriptions.size,
    };
  }
}

// Export singleton
export const imageCache = new ImageCacheService();

/**
 * Hook pour utiliser le cache d'images de manière reactive
 */
export function useImageCache(imageUrl: Ref<string | null | undefined>) {
  const entry = ref<ImageCacheEntry | null>(null);
  const cleanup = ref<(() => void) | null>(null);

  const loading = computed(() => entry.value?.loading ?? false);
  const loaded = computed(() => entry.value?.loaded ?? false);
  const error = computed(() => entry.value?.error ?? false);
  const lastError = computed(() => entry.value?.lastError ?? null);
  const retryCount = computed(() => entry.value?.retryCount ?? 0);

  watch(
    imageUrl,
    (newUrl) => {
      if (cleanup.value) {
        cleanup.value();
        cleanup.value = null;
      }

      if (newUrl) {
        cleanup.value = imageCache.subscribe(newUrl, (newEntry) => {
          entry.value = newEntry;
        });
      } else {
        entry.value = null;
      }
    },
    { immediate: true }
  );

  onUnmounted(() => {
    if (cleanup.value) {
      cleanup.value();
      cleanup.value = null;
    }
  });

  function reload() {
    const url = imageUrl.value;
    if (url) {
      imageCache.startLoading(url);
    }
  }

  return {
    entry,
    loading,
    loaded,
    error,
    lastError,
    retryCount,
    reload,
  };
}