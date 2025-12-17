import { createRouter, createWebHistory } from "vue-router";

// PMOControl Views (nouvelle home)
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
  // PMOControl (nouvelle home)
  {
    path: "/",
    name: "Dashboard",
    component: DashboardView,
  },
  {
    path: "/renderer/:id",
    name: "Renderer",
    component: RendererView,
  },
  {
    path: "/server/:serverId",
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
