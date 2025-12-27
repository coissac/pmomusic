import { createRouter, createWebHistory } from "vue-router";

// PMOControl Unified View (nouvelle interface unifiée)
import UnifiedControlView from "../views/UnifiedControlView.vue";

// PMOControl Views (anciennes vues - conservées pour fallback)
import DashboardView from "../views/DashboardView.vue";
import RendererView from "../views/RendererView.vue";
import MediaServerView from "../views/MediaServerView.vue";

// Debug Components (anciennes routes)
import GenericMusicPlayer from "../components/GenericMusicPlayer.vue";
import LogView from "../components/LogView.vue";
import CoverCacheManager from "../components/CoverCacheManager.vue";
import AudioCacheManager from "../components/AudioCacheManager.vue";
import PlayListManager from "../components/PlayListManager.vue";
import UpnpExplorer from "../components/UpnpExplorer.vue";
import APIDashboard from "../components/APIDashboard.vue";
import RadioParadiseExplorer from "../components/RadioParadiseExplorer.vue";

const routes = [
  // PMOControl Unified Interface (nouvelle interface unifiée avec onglets)
  {
    path: "/",
    name: "Control",
    component: UnifiedControlView,
  },

  // Anciennes routes (conservées sous /legacy pour fallback)
  {
    path: "/legacy",
    name: "Dashboard",
    component: DashboardView,
  },
  {
    path: "/legacy/renderer/:id",
    name: "Renderer",
    component: RendererView,
  },
  {
    path: "/legacy/server/:serverId",
    name: "MediaServer",
    component: MediaServerView,
  },

  // Debug menu (anciennes routes déplacées sous /debug)
  {
    path: "/debug/generic-player",
    name: "GenericPlayer",
    component: GenericMusicPlayer,
  },
  {
    path: "/debug/logs",
    name: "Logs",
    component: LogView,
  },
  {
    path: "/debug/covers-cache",
    name: "CoversCache",
    component: CoverCacheManager,
  },
  {
    path: "/debug/audio-cache",
    name: "AudioCache",
    component: AudioCacheManager,
  },
  {
    path: "/debug/playlists",
    name: "PlaylistsManager",
    component: PlayListManager,
  },
  {
    path: "/debug/upnp",
    name: "UpnpExplorer",
    component: UpnpExplorer,
  },
  {
    path: "/debug/api-dashboard",
    name: "APIDashboard",
    component: APIDashboard,
  },
  {
    path: "/debug/radio-paradise",
    name: "RadioParadise",
    component: RadioParadiseExplorer,
  },
];

const router = createRouter({
  // history avec base /app
  history: createWebHistory("/app"),
  routes,
});

export default router;
