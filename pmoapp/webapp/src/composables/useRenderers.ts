/**
 * Composable pour gérer les renderers.
 * Le ControlPoint est la seule source de vérité :
 * - Les snapshots complets proviennent de /renderers/{id}/full
 * - Les événements SSE ne servent qu'à déclencher un refetch.
 */
import { ref, reactive, computed, type Ref, onUnmounted } from "vue";
import { api } from "../services/pmocontrol/api";
import { useSSE } from "./useSSE";
import { apiCache } from "./apiCache";
import { parseTimeToMs } from "../utils/time";
import { useUIStore } from "@/stores/ui";
import type {
  RendererSummary,
  RendererState,
  QueueSnapshot,
  AttachedPlaylistInfo,
  FullRendererSnapshot,
} from "../services/pmocontrol/types";
import { isTransportState } from "../services/pmocontrol/types";

// État global des snapshots avec reactive pour une réactivité native Vue sur les Maps
const snapshots = reactive(new Map<string, FullRendererSnapshot>());
const lastSnapshotAt = reactive(new Map<string, number>());
const lastEventAt = reactive(new Map<string, number>());
const loadingIds = reactive(new Set<string>());
const queueRefreshingIds = reactive(new Set<string>());
const selectedRendererId = ref<string | null>(null);

// Debounce pour les refetches queue_updated
const queueUpdateDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();
const QUEUE_UPDATE_DEBOUNCE_MS = 300;

// Cache des renderers (summary)
const renderersCache = ref<Map<string, RendererSummary>>(new Map());
const RENDERERS_CACHE_MS = 2000;

// Supprimé : les helpers triggerXXX ne sont plus nécessaires avec reactive

const loading = ref(false);
const error = ref<string | null>(null);

// Utiliser le composable SSE centralisé
let sseInitialized = false;

/**
 * Réinitialise le flag SSE pour permettre une nouvelle connexion après reconnexion
 */
function resetSSE() {
  sseInitialized = false;
}

function ensureSSEInitialized() {
  if (sseInitialized) return;

  const { onRendererEvent, connect } = useSSE();
  
  // Démarrer la connexion SSE
  connect();

  onRendererEvent((event) => {
    const rendererId = event.renderer_id;
    const timestamp = Date.parse(event.timestamp ?? "") || Date.now();

    // Gérer les événements Online/Offline différemment
    if (event.type === "online") {
      // Ajouter au cache avec les infos disponibles
      // Note: on n'a pas toutes les infos (capabilities, protocol) donc on fetch ensuite
      const renderer: RendererSummary = {
        id: rendererId,
        friendly_name: event.friendly_name,
        model_name: event.model_name,
        manufacturer: event.manufacturer,
        protocol: "upnp", // Valeur par défaut, sera mise à jour par le fetch
        capabilities: {
          has_avtransport: false,
          has_avtransport_set_next: false,
          has_rendering_control: false,
          has_connection_manager: false,
          has_linkplay_http: false,
          has_arylic_tcp: false,
          has_oh_playlist: false,
          has_oh_volume: false,
          has_oh_info: false,
          has_oh_time: false,
          has_oh_radio: false,
        },
        online: true,
      };
      renderersCache.value.set(rendererId, renderer);

      // Fetch la liste complète pour avoir les bonnes infos
      void fetchRenderers(true);

      // Fetch le snapshot complet pour ce renderer
      void fetchRendererSnapshot(rendererId, { force: true });
      return;
    }

    if (event.type === "offline") {
      // Marquer comme offline dans le cache
      const renderer = renderersCache.value.get(rendererId);
      if (renderer) {
        renderer.online = false;
        renderersCache.value.set(rendererId, renderer);
      }

      // Supprimer le snapshot (il n'est plus valide)
      snapshots.delete(rendererId);
      lastSnapshotAt.delete(rendererId);
      lastEventAt.delete(rendererId);
      return;
    }

    // Pour les autres événements, mettre à jour le snapshot local directement
    lastEventAt.set(rendererId, timestamp);

    const snapshot = snapshots.get(rendererId);

    // Si pas de snapshot, on doit fetch
    if (!snapshot) {
      void fetchRendererSnapshot(rendererId, { force: true });
      return;
    }

    // Sinon, mettre à jour le snapshot localement selon le type d'événement
    switch (event.type) {
      case "state_changed":
        // Créer un nouvel objet pour déclencher la réactivité
        if (isTransportState(event.state)) {
          snapshots.set(rendererId, {
            ...snapshot,
            state: { ...snapshot.state, transport_state: event.state },
          });
          
        } else {
          console.warn(`[useRenderers] transport_state inconnu: ${event.state}`);
        }
        break;

      case "position_changed":
        // Mettre à jour position et durée de manière atomique pour garantir la cohérence
        // Le backend envoie TOUJOURS les deux valeurs (même si null)

        // Convertir rel_time (HH:MM:SS) en millisecondes
        const positionMs = parseTimeToMs(event.rel_time ?? null);

        // Convertir track_duration (HH:MM:SS) en millisecondes
        const durationMs = parseTimeToMs(event.track_duration ?? null);
        
        // Créer un nouvel objet pour déclencher la réactivité
        snapshots.set(rendererId, {
          ...snapshot,
          state: {
            ...snapshot.state,
            position_ms: positionMs ?? 0,
            duration_ms: durationMs,
          },
        });
        
        break;

      case "volume_changed":
        // Créer un nouvel objet pour déclencher la réactivité
        snapshots.set(rendererId, {
          ...snapshot,
          state: { ...snapshot.state, volume: event.volume },
        });
        break;

      case "mute_changed":
        // Créer un nouvel objet pour déclencher la réactivité
        snapshots.set(rendererId, {
          ...snapshot,
          state: { ...snapshot.state, mute: event.mute },
        });
        break;

      case "metadata_changed":
        if (!snapshot.state.current_track) {
          snapshot.state.current_track = {
            title: null,
            artist: null,
            album: null,
            album_art_uri: null,
          };
        }
        snapshot.state.current_track.title = event.title;
        snapshot.state.current_track.artist = event.artist;
        snapshot.state.current_track.album = event.album;
        snapshot.state.current_track.album_art_uri = event.album_art_uri;
        // Important: Trigger reactivity en réassignant l'objet complet avec deep copy
        snapshots.set(rendererId, {
          ...snapshot,
          state: { ...snapshot.state },
        });
        break;

      case "queue_refreshing":
        queueRefreshingIds.add(rendererId);
        
        break;

      case "queue_updated":
        snapshot.state.queue_len = event.queue_length;
        queueRefreshingIds.delete(rendererId);
        
        // Annuler le timer précédent pour ce renderer
        const existingTimer = queueUpdateDebounceTimers.get(rendererId);
        if (existingTimer) clearTimeout(existingTimer);

        // Programmer un seul fetch après stabilisation
        queueUpdateDebounceTimers.set(rendererId, setTimeout(() => {
            queueUpdateDebounceTimers.delete(rendererId);
            void fetchRendererSnapshot(rendererId, { force: true });
        }, QUEUE_UPDATE_DEBOUNCE_MS));
        break;

      case "binding_changed":
        if (event.server_id && event.container_id) {
          snapshot.binding = {
            server_id: event.server_id,
            container_id: event.container_id,
            has_seen_update: false,
          };
          snapshot.state.attached_playlist = snapshot.binding;
        } else {
          snapshot.binding = null;
          snapshot.state.attached_playlist = null;
        }
        // Créer un nouvel objet pour déclencher la réactivité
        snapshots.set(rendererId, {
          ...snapshot,
          state: { ...snapshot.state },
        });
        break;

      case "stream_state_changed":
        snapshot.is_stream = event.is_stream;
        // Créer un nouvel objet pour déclencher la réactivité
        snapshots.set(rendererId, { ...snapshot });
        break;

      case "timer_started":
      case "timer_updated":
      case "timer_tick":
      case "timer_expired":
      case "timer_cancelled":
        // Les événements de timer ne modifient pas le snapshot renderer
        // (le timer state est géré séparément)
        break;
    }

    // Note: chaque case est maintenant responsable de stocker le snapshot dans la Map
    // Plus de réassignation finale après le switch
  });

  sseInitialized = true;
}

const allRenderers = computed(() => Array.from(renderersCache.value.values()));
const onlineRenderers = computed(() =>
  allRenderers.value.filter((r) => r.online),
);
const allSnapshots = computed(() =>
  Array.from(snapshots.values()),
);
const playingRenderers = computed(() =>
  allSnapshots.value
    .filter((snapshot) => snapshot.state.transport_state === "PLAYING")
    .map((snapshot) => snapshot.state),
);

function getRendererById(id: string) {
  return renderersCache.value.get(id);
}

function getSnapshotById(id: string) {
  return snapshots.get(id) ?? null;
}

function getStateById(id: string): RendererState | null {
  return snapshots.get(id)?.state ?? null;
}

function getQueueById(id: string): QueueSnapshot | null {
  return snapshots.get(id)?.queue ?? null;
}

function getBindingById(id: string): AttachedPlaylistInfo | null {
  return snapshots.get(id)?.binding ?? null;
}

function isSnapshotLoading(id: string) {
  return loadingIds.has(id);
}

function isQueueRefreshing(id: string) {
  return queueRefreshingIds.has(id);
}

function selectRenderer(id: string | null) {
  selectedRendererId.value = id;
}

async function fetchRenderers(force = false, retries = 2) {
  ensureSSEInitialized();
  
  // Créer le store UI pour les notifications (lazy import pour éviter les effets de bord)
  const uiStore = useUIStore();

  let lastError: Error | null = null;
  
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      loading.value = true;
      error.value = null;

      // Utiliser le cache API centralisé
      const data = await apiCache.fetch(
        '/renderers',
        undefined,
        () => api.getRenderers(),
        { force, ttl: RENDERERS_CACHE_MS }
      );
      
      renderersCache.value = new Map(
        data.map((renderer) => [renderer.id, renderer]),
      );
      return;
    } catch (err) {
      lastError = err instanceof Error ? err : new Error("Erreur fetch renderers");
      console.error("[useRenderers] Erreur fetch (attempt " + (attempt + 1) + "):", lastError);
      if (attempt < retries) {
        await new Promise(r => setTimeout(r, 500 * (attempt + 1)));
      }
    } finally {
      loading.value = false;
    }
  }
  
  error.value = lastError?.message ?? "Erreur fetch renderers";
  
  // Notifier l'utilisateur en cas d'erreur finale
  uiStore.notifyError("Impossible de rafraîchir la liste des renderers");
}

async function fetchRendererSnapshot(
  rendererId: string,
  opts?: { force?: boolean },
) {
  ensureSSEInitialized();
  const force = opts?.force ?? false;
  const hasSnapshot = snapshots.has(rendererId);

  if (!force && hasSnapshot) {
    const lastSnapshot = lastSnapshotAt.get(rendererId) ?? 0;
    const lastEvent = lastEventAt.get(rendererId) ?? 0;
    if (lastEvent <= lastSnapshot) {
      return;
    }
  }

  // Éviter les requêtes multiples simultanées pour le même renderer
  if (loadingIds.has(rendererId)) {
    return;
  }

  loadingIds.add(rendererId);
  
  
  // Lazy load UI store pour les notifications
  const uiStore = useUIStore();
  
  try {
    const snapshot = await api.getRendererFullSnapshot(rendererId);
    snapshots.set(rendererId, snapshot);
    lastSnapshotAt.set(rendererId, Date.now());
    
  } catch (err) {
    console.error(`[useRenderers] Erreur snapshot ${rendererId}:`, err);
    // En cas d'erreur, on supprime le snapshot pour permettre une nouvelle tentative
    snapshots.delete(rendererId);
    
    // Notifier l'utilisateur
    uiStore.notifyError(`Impossible de récupérer l'état du renderer`);
  } finally {
    // Toujours nettoyer le flag de chargement
    loadingIds.delete(rendererId);
    
  }
}

/**
 * Fetch les snapshots de plusieurs renderers en parallèle controlée.
 * - Limite le nombre de requêtes simultanées (concurrency)
 * - Ajoute un délai entre chaque batch pour ne pas saturer le réseau
 * - Continue même si certaines requêtes échouent
 */
async function fetchBatchSnapshots(
  rendererIds: string[],
  options: {
    concurrency?: number;  // Nombre max de requêtes parallèles (défaut: 3)
    batchDelay?: number;   // Délai entre les batches en ms (défaut: 100ms)
    force?: boolean;       // Forcer le refetch même en cache
  } = {}
): Promise<void> {
  const { concurrency = 3, batchDelay = 100, force = false } = options;
  
  // Filtrer les rendererIds valides
  const validIds = rendererIds.filter(id => id && typeof id === 'string');
  
  if (validIds.length === 0) return;

  // Fonction pour traiter un batch
  const processBatch = async (batch: string[]): Promise<void> => {
    await Promise.allSettled(
      batch.map(id => fetchRendererSnapshot(id, { force }))
    );
  };

  // Exécuter par batches avec controlled concurrency
  for (let i = 0; i < validIds.length; i += concurrency) {
    const batch = validIds.slice(i, i + concurrency);
    await processBatch(batch);
    
    // Délai entre les batches (sauf pour le dernier)
    if (i + concurrency < validIds.length) {
      await new Promise(resolve => setTimeout(resolve, batchDelay));
    }
  }
}

// Transport controls
async function play(id: string) {
  await api.play(id);
}

async function resumeOrPlayFromQueue(id: string) {
  const snapshot = snapshots.get(id);
  if (!snapshot) {
    throw new Error(`Renderer ${id} non trouvé`);
  }

  const state = snapshot.state;
  if (state.transport_state === "PAUSED") {
    return play(id);
  }

  if (
    ["STOPPED", "NO_MEDIA"].includes(state.transport_state) &&
    snapshot.queue.items.length > 0
  ) {
    return api.resume(id);
  }

  throw new Error(
    "La file d'attente est vide. Ajoutez des morceaux avant de démarrer la lecture.",
  );
}

async function pause(id: string) {
  await api.pause(id);
}

async function stop(id: string) {
  await api.stop(id);
}

async function next(id: string) {
  await api.next(id);
}

// Volume controls
async function setVolume(id: string, volume: number) {
  await api.setVolume(id, volume);
}

async function volumeUp(id: string) {
  await api.volumeUp(id);
}

async function volumeDown(id: string) {
  await api.volumeDown(id);
}

async function toggleMute(id: string) {
  await api.toggleMute(id);
}

// Playlist binding
async function attachPlaylist(
  rendererId: string,
  serverId: string,
  containerId: string,
  options?: { autoPlay?: boolean },
) {
  await api.attachPlaylist(
    rendererId,
    serverId,
    containerId,
    options?.autoPlay ?? false,
  );
}

async function detachPlaylist(rendererId: string) {
  await api.detachPlaylist(rendererId);
}

async function attachAndPlayPlaylist(
  rendererId: string,
  serverId: string,
  containerId: string,
) {
  await attachPlaylist(rendererId, serverId, containerId, { autoPlay: true });
}

// Queue content
async function playContent(
  rendererId: string,
  serverId: string,
  objectId: string,
) {
  await api.playContent(rendererId, serverId, objectId);
}

async function addToQueue(
  rendererId: string,
  serverId: string,
  objectId: string,
) {
  await api.addToQueue(rendererId, serverId, objectId);
}

async function addAfterCurrent(
  rendererId: string,
  serverId: string,
  objectId: string,
) {
  await api.addAfterCurrent(rendererId, serverId, objectId);
}

export function useRenderers() {
  ensureSSEInitialized();

  return {
    loading,
    error,
    // Collections
    allRenderers,
    onlineRenderers,
    playingRenderers,
    // Accessors
    getRendererById,
    getSnapshotById,
    getStateById,
    getQueueById,
    getBindingById,
    isSnapshotLoading,
    isQueueRefreshing,
    selectRenderer,
    // Fetchers
    fetchRenderers,
    fetchRendererSnapshot,
    fetchBatchSnapshots,
    // Transport controls
    play,
    resumeOrPlayFromQueue,
    pause,
    stop,
    next,
    // Volume controls
    setVolume,
    volumeUp,
    volumeDown,
    toggleMute,
    // Playlist binding
    attachPlaylist,
    detachPlaylist,
    attachAndPlayPlaylist,
    // Queue content
    playContent,
    addToQueue,
    addAfterCurrent,
    // SSE
    resetSSE,
  };
}

export function useRenderer(rendererId: Ref<string>) {
  ensureSSEInitialized();

  // Delegates to useRenderers functions - no duplication
  const renderer = computed(() => getRendererById(rendererId.value));
  const snapshot = computed(() => getSnapshotById(rendererId.value));
  const state = computed(() => getStateById(rendererId.value));
  const queue = computed(() => getQueueById(rendererId.value));
  const binding = computed(() => getBindingById(rendererId.value));
  const isStream = computed(() => snapshot.value?.is_stream ?? false);
  const queueRefreshing = computed(() => isQueueRefreshing(rendererId.value));

  // Debounce pour éviter les refreshs multiples trop fréquents
  let refreshDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  const REFRESH_DEBOUNCE_MS = 500;

  // Nettoyer le timer debounce si le composant est démonté
  onUnmounted(() => {
    if (refreshDebounceTimer !== null) {
      clearTimeout(refreshDebounceTimer);
      refreshDebounceTimer = null;
    }
  });

  async function refresh(force = true) {
    const currentRendererId = rendererId.value;
    
    // Debounce: ignorer si un refresh est en cours pour ce renderer
    if (refreshDebounceTimer !== null) {
      return;
    }
    
    refreshDebounceTimer = setTimeout(() => {
      refreshDebounceTimer = null;
    }, REFRESH_DEBOUNCE_MS);

    await Promise.all([
      fetchRenderers(force),
      fetchRendererSnapshot(currentRendererId, { force: true }),
    ]);
  }

  return {
    renderer,
    snapshot,
    state,
    queue,
    binding,
    isStream,
    queueRefreshing,
    refresh,
  };
}
