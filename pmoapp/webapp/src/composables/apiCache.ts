/**
 * Cache API centralisé pour les requêtes HTTP
 * 
 * Fonctionnalités:
 * - Cache mémoire avec TTL configurable
 * - Dédupplication des requêtes en cours (une seule requête pour plusieurs callers)
 * - Invalidation par pattern (ex: invalidate('renderers/*'))
 * - Subscribe aux changements de données pour reactivity
 */

export interface CacheEntry<T> {
  data: T;
  timestamp: number;
  ttl?: number;
  etag?: string;
}

export interface ApiCacheOptions {
  ttl?: number;
  staleWhileRevalidate?: boolean;
}

interface PendingRequest {
  promise: Promise<unknown>;
  subscribers: Set<(data: unknown) => void>;
}

/**
 * Classe principale du cache API
 */
class ApiCacheService {
  private cache = new Map<string, CacheEntry<unknown>>();
  private pendingRequests = new Map<string, PendingRequest>();
  private subscriptions = new Map<string, Set<(data: unknown) => void>>();
  
  private options: Required<ApiCacheOptions> = {
    ttl: 2000,
    staleWhileRevalidate: true,
  };

  configure(options: Partial<ApiCacheOptions>) {
    this.options = { ...this.options, ...options };
  }

  private makeKey(endpoint: string, params?: Record<string, string | number | boolean>): string {
    if (!params) return endpoint;
    const sorted = Object.entries(params).sort(([a], [b]) => a.localeCompare(b));
    const query = sorted.map(([k, v]) => `${k}=${v}`).join('&');
    return `${endpoint}?${query}`;
  }

  private isFresh(key: string): boolean {
    const entry = this.cache.get(key);
    if (!entry) return false;
    // Lire le TTL de l'entrée, sinon utiliser le TTL global par défaut
    const ttl = entry.ttl ?? this.options.ttl;
    return Date.now() - entry.timestamp < ttl;
  }

  get<T>(endpoint: string, params?: Record<string, string | number | boolean>): T | null {
    const key = this.makeKey(endpoint, params);
    const entry = this.cache.get(key) as CacheEntry<T> | undefined;
    
    if (!entry) return null;
    if (!this.isFresh(key)) {
      return this.options.staleWhileRevalidate ? entry.data : null;
    }
    
    return entry.data;
  }

  set<T>(endpoint: string, data: T, params?: Record<string, string | number | boolean>, etag?: string, ttl?: number): void {
    const key = this.makeKey(endpoint, params);
    
    this.cache.set(key, {
      data,
      timestamp: Date.now(),
      ttl,
      etag,
    });

    this.notifySubscribers(key, data);
  }

  async fetch<T>(
    endpoint: string,
    params: Record<string, string | number | boolean> | undefined,
    fetcher: () => Promise<T>,
    options: { force?: boolean; ttl?: number } = {}
  ): Promise<T> {
    const key = this.makeKey(endpoint, params);
    const { force = false, ttl } = options;

    if (!force && this.isFresh(key)) {
      const cached = this.get<T>(endpoint, params);
      if (cached) return cached;
    }

    // Utiliser une clé unique pour éviter les problèmes de race condition
    // avec les requêtes en cours qui peuvent être supprimées avant résolution
    const requestKey = `request:${key}`;
    
    // Récupérer ou créer la requête
    let pendingRequest = this.pendingRequests.get(requestKey);
    
    // Si une requête est en cours et sa promesse n'a pas encore été resolved/rejected
    // on retourne directement cette promesse
    if (pendingRequest) {
      try {
        // Attendre la résolution pour s'assurer que c'est toujours valide
        return await pendingRequest.promise as T;
      } catch (e) {
        // La requête a échoué, on continue pour faire une nouvelle requête
        this.pendingRequests.delete(requestKey);
      }
    }

    let resolvePromise!: (value: T) => void;
    let rejectPromise!: (reason: unknown) => void;
    
    const promise = new Promise<T>((resolve, reject) => {
      resolvePromise = resolve;
      rejectPromise = reject;
    });

    this.pendingRequests.set(requestKey, {
      promise,
      subscribers: new Set(),
    });

    try {
      const data = await fetcher();
      
      // Passer le TTL directement à set() pour éviter les problèmes de race condition
      // avec la modification globale de this.options.ttl
      this.set(endpoint, data, params, undefined, ttl);

      resolvePromise(data);
      
      const pending = this.pendingRequests.get(requestKey);
      if (pending) {
        pending.subscribers.forEach(cb => cb(data));
      }
      
    } catch (error) {
      rejectPromise(error);
      throw error;
    } finally {
      this.pendingRequests.delete(requestKey);
    }

    return promise;
  }

  subscribe<T>(endpoint: string, params: Record<string, string | number | boolean>, callback: (data: T) => void): () => void {
    const key = this.makeKey(endpoint, params);
    
    if (!this.subscriptions.has(key)) {
      this.subscriptions.set(key, new Set());
    }
    
    this.subscriptions.get(key)!.add(callback as (data: unknown) => void);
    
    const cached = this.get<T>(endpoint, params);
    if (cached) {
      callback(cached);
    }

    return () => {
      const subs = this.subscriptions.get(key);
      if (subs) {
        subs.delete(callback as (data: unknown) => void);
        if (subs.size === 0) {
          this.subscriptions.delete(key);
        }
      }
    };
  }

  invalidate(pattern: string): void {
    const keysToDelete: string[] = [];
    
    if (pattern.includes('*')) {
      const prefix = pattern.replace('*', '');
      this.cache.forEach((_, key) => {
        if (key.startsWith(prefix)) {
          keysToDelete.push(key);
        }
      });
    } else {
      if (this.cache.has(pattern)) {
        keysToDelete.push(pattern);
      }
    }

    keysToDelete.forEach(key => {
      this.cache.delete(key);
      // NE PAS supprimer this.subscriptions.get(key)
      // Les abonnés seront notifiés lors du prochain set() après un refetch
    });
  }

  clear(): void {
    this.cache.clear();
    this.subscriptions.clear();
  }

  async invalidateAndFetch<T>(
    endpoint: string,
    params: Record<string, string | number | boolean>,
    fetcher: () => Promise<T>
  ): Promise<T> {
    this.invalidate(this.makeKey(endpoint, params));
    return this.fetch(endpoint, params, fetcher, { force: true });
  }

  getStats() {
    let fresh = 0;
    let stale = 0;
    const now = Date.now();
    
    this.cache.forEach((entry) => {
      if (now - entry.timestamp < this.options.ttl) {
        fresh++;
      } else {
        stale++;
      }
    });

    return {
      total: this.cache.size,
      fresh,
      stale,
      pending: this.pendingRequests.size,
      subscriptions: this.subscriptions.size,
    };
  }

  private notifySubscribers(key: string, data: unknown) {
    const subs = this.subscriptions.get(key);
    if (subs) {
      subs.forEach(cb => {
        try {
          cb(data);
        } catch (e) {
          console.error('[ApiCache] Error in subscriber:', e);
        }
      });
    }
  }
}

export const apiCache = new ApiCacheService();

export function useApiCache() {
  return {
    fetch<T>(
      endpoint: string,
      params: Record<string, string | number | boolean> | undefined,
      fetcher: () => Promise<T>,
      options?: { force?: boolean; ttl?: number }
    ): Promise<T> {
      return apiCache.fetch(endpoint, params, fetcher, options);
    },

    subscribe<T>(
      endpoint: string,
      params: Record<string, string | number | boolean>,
      callback: (data: T) => void
    ): () => void {
      return apiCache.subscribe(endpoint, params, callback);
    },

    invalidate(pattern: string): void {
      apiCache.invalidate(pattern);
    },

    clear(): void {
      apiCache.clear();
    },

    getStats() {
      return apiCache.getStats();
    },
  };
}