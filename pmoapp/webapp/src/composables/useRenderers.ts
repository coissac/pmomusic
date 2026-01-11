/**
 * Composable pour gérer les renderers.
 * Le ControlPoint est la seule source de vérité :
 * - Les snapshots complets proviennent de /renderers/{id}/full
 * - Les événements SSE ne servent qu'à déclencher un refetch.
 */
import { ref, reactive, computed, type Ref } from "vue";
import { api } from "../services/pmocontrol/api";
import { sse } from "../services/pmocontrol/sse";
import type {
  RendererSummary,
  RendererState,
  QueueSnapshot,
  AttachedPlaylistInfo,
  FullRendererSnapshot,
} from "../services/pmocontrol/types";

interface RendererSnapshotState {
  snapshots: Map<string, FullRendererSnapshot>;
  lastSnapshotAt: Map<string, number>;
  lastEventAt: Map<string, number>;
  loadingIds: Set<string>;
  selectedRendererId: string | null;
}

const renderersCache = ref<Map<string, RendererSummary>>(new Map());
const RENDERERS_CACHE_MS = 2000;
const lastRenderersFetch = ref(0);

const snapshotState = reactive<RendererSnapshotState>({
  snapshots: reactive(new Map<string, FullRendererSnapshot>()),
  lastSnapshotAt: reactive(new Map<string, number>()),
  lastEventAt: reactive(new Map<string, number>()),
  loadingIds: reactive(new Set<string>()),
  selectedRendererId: null,
});

const loading = ref(false);
const error = ref<string | null>(null);

let sseConnected = false;
function ensureSSEConnected() {
  if (sseConnected) return;

  sse.onRendererEvent((event) => {
    const rendererId = event.renderer_id;
    const timestamp = Date.parse(event.timestamp ?? "") || Date.now();

    // Gérer les événements Online/Offline différemment
    if (event.type === "online") {
      // Nouveau renderer découvert
      console.log(
        `[useRenderers] Renderer ${rendererId} (${event.friendly_name}) est maintenant en ligne`,
      );

      // Ajouter au cache avec les infos disponibles
      // Note: on n'a pas toutes les infos (capabilities, protocol) donc on fetch ensuite
      const renderer: RendererSummary = {
        id: rendererId,
        friendly_name: event.friendly_name,
        model_name: event.model_name,
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
      // Renderer déconnecté
      console.log(
        `[useRenderers] Renderer ${rendererId} est maintenant hors ligne`,
      );

      // Marquer comme offline dans le cache
      const renderer = renderersCache.value.get(rendererId);
      if (renderer) {
        renderer.online = false;
        renderersCache.value.set(rendererId, renderer);
      }

      // Supprimer le snapshot (il n'est plus valide)
      snapshotState.snapshots.delete(rendererId);
      snapshotState.lastSnapshotAt.delete(rendererId);
      snapshotState.lastEventAt.delete(rendererId);
      return;
    }

    // Pour les autres événements, mettre à jour le snapshot local directement
    snapshotState.lastEventAt.set(rendererId, timestamp);

    const snapshot = snapshotState.snapshots.get(rendererId);

    // Si pas de snapshot, on doit fetch
    if (!snapshot) {
      void fetchRendererSnapshot(rendererId, { force: true });
      return;
    }

    // Sinon, mettre à jour le snapshot localement selon le type d'événement
    switch (event.type) {
      case "state_changed":
        snapshot.state.transport_state = event.state as any;
        break;

      case "position_changed":
        // Convertir rel_time (HH:MM:SS) en millisecondes
        if (event.rel_time) {
          const parts = event.rel_time.split(":").map(Number);
          if (parts.length === 3) {
            snapshot.state.position_ms =
              ((parts[0] ?? 0) * 3600 +
                (parts[1] ?? 0) * 60 +
                (parts[2] ?? 0)) *
              1000;
          }
        }
        // Convertir track_duration (HH:MM:SS) en millisecondes
        if (event.track_duration) {
          const parts = event.track_duration.split(":").map(Number);
          if (parts.length === 3) {
            snapshot.state.duration_ms =
              ((parts[0] ?? 0) * 3600 +
                (parts[1] ?? 0) * 60 +
                (parts[2] ?? 0)) *
              1000;
          }
        }
        break;

      case "volume_changed":
        snapshot.state.volume = event.volume;
        break;

      case "mute_changed":
        snapshot.state.mute = event.mute;
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
        break;

      case "queue_updated":
        snapshot.state.queue_len = event.queue_length;
        // Pour la queue complète, on doit refetch
        void fetchRendererSnapshot(rendererId, { force: true });
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

    // Trigger reactivity
    snapshotState.snapshots.set(rendererId, snapshot);
  });

  sseConnected = true;
}

const allRenderers = computed(() => Array.from(renderersCache.value.values()));
const onlineRenderers = computed(() =>
  allRenderers.value.filter((r) => r.online),
);
const allSnapshots = computed(() =>
  Array.from(snapshotState.snapshots.values()),
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
  return snapshotState.snapshots.get(id) ?? null;
}

function getStateById(id: string): RendererState | null {
  return snapshotState.snapshots.get(id)?.state ?? null;
}

function getQueueById(id: string): QueueSnapshot | null {
  return snapshotState.snapshots.get(id)?.queue ?? null;
}

function getBindingById(id: string): AttachedPlaylistInfo | null {
  return snapshotState.snapshots.get(id)?.binding ?? null;
}

function isSnapshotLoading(id: string) {
  return snapshotState.loadingIds.has(id);
}

function selectRenderer(id: string | null) {
  snapshotState.selectedRendererId = id;
}

async function fetchRenderers(force = false) {
  ensureSSEConnected();

  const now = Date.now();
  if (!force && now - lastRenderersFetch.value < RENDERERS_CACHE_MS) {
    return;
  }

  try {
    loading.value = true;
    error.value = null;
    const data = await api.getRenderers();
    renderersCache.value = new Map(
      data.map((renderer) => [renderer.id, renderer]),
    );
    lastRenderersFetch.value = now;
  } catch (err) {
    error.value = err instanceof Error ? err.message : "Erreur fetch renderers";
    console.error("[useRenderers] Erreur fetch:", err);
  } finally {
    loading.value = false;
  }
}

async function fetchRendererSnapshot(
  rendererId: string,
  opts?: { force?: boolean },
) {
  ensureSSEConnected();
  const force = opts?.force ?? false;
  const hasSnapshot = snapshotState.snapshots.has(rendererId);

  if (!force && hasSnapshot) {
    const lastSnapshot = snapshotState.lastSnapshotAt.get(rendererId) ?? 0;
    const lastEvent = snapshotState.lastEventAt.get(rendererId) ?? 0;
    if (lastEvent <= lastSnapshot) {
      return;
    }
  }

  if (snapshotState.loadingIds.has(rendererId)) {
    return;
  }

  snapshotState.loadingIds.add(rendererId);
  try {
    const snapshot = await api.getRendererFullSnapshot(rendererId);
    snapshotState.snapshots.set(rendererId, snapshot);
    snapshotState.lastSnapshotAt.set(rendererId, Date.now());
  } catch (err) {
    console.error(`[useRenderers] Erreur snapshot ${rendererId}:`, err);
  } finally {
    snapshotState.loadingIds.delete(rendererId);
  }
}

// Transport controls
async function play(id: string) {
  await api.play(id);
}

async function resumeOrPlayFromQueue(id: string) {
  const snapshot = snapshotState.snapshots.get(id);
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
  ensureSSEConnected();

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
    selectRenderer,
    snapshotState,
    // Fetchers
    fetchRenderers,
    fetchRendererSnapshot,
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
  };
}

export function useRenderer(rendererId: Ref<string>) {
  ensureSSEConnected();

  const renderer = computed(() => renderersCache.value.get(rendererId.value));
  const snapshot = computed(
    () => snapshotState.snapshots.get(rendererId.value) ?? null,
  );
  const state = computed(() => snapshot.value?.state ?? null);
  const queue = computed(() => snapshot.value?.queue ?? null);
  const binding = computed(() => snapshot.value?.binding ?? null);

  async function refresh(force = true) {
    await Promise.all([
      fetchRenderers(force),
      fetchRendererSnapshot(rendererId.value, { force: true }),
    ]);
  }

  return {
    renderer,
    snapshot,
    state,
    queue,
    binding,
    refresh,
  };
}
