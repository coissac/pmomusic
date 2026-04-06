import { createRouter, createWebHistory } from "vue-router";
import type { RouteRecordRaw } from "vue-router";

// PMOControl Unified View (nouvelle interface unifiée)
import UnifiedControlView from "../views/UnifiedControlView.vue";

// PMOControl Views (anciennes vues - conservées pour fallback)
import DashboardView from "../views/DashboardView.vue";
import RendererView from "../views/RendererView.vue";
import MediaServerView from "../views/MediaServerView.vue";

// Debug Components - lazy loaded uniquement en mode développement (P8)
const isDev = import.meta.env.DEV;

const routes: RouteRecordRaw[] = [
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
];

// Ajouter les routes de debug uniquement en développement
if (isDev) {
  routes.push(
    {
      path: "/debug",
      name: "Debug",
      component: () => import("../views/DebugView.vue"),
    },
    {
      path: "/debug/generic-player",
      name: "GenericPlayer",
      component: () => import("../components/GenericMusicPlayer.vue"),
    },
    {
      path: "/debug/logs",
      name: "Logs",
      component: () => import("../components/LogView.vue"),
    },
    {
      path: "/debug/covers-cache",
      name: "CoversCache",
      component: () => import("../components/CoverCacheManager.vue"),
    },
    {
      path: "/debug/audio-cache",
      name: "AudioCache",
      component: () => import("../components/AudioCacheManager.vue"),
    },
    {
      path: "/debug/playlists",
      name: "PlaylistsManager",
      component: () => import("../components/PlayListManager.vue"),
    },
    {
      path: "/debug/upnp",
      name: "UpnpExplorer",
      component: () => import("../components/UpnpExplorer.vue"),
    },
    {
      path: "/debug/api-dashboard",
      name: "APIDashboard",
      component: () => import("../components/APIDashboard.vue"),
    },
    {
      path: "/debug/radio-paradise",
      name: "RadioParadise",
      component: () => import("../components/RadioParadiseExplorer.vue"),
    }
  );
}

// Wildcard redirect pour les routes inconnues
routes.push({
  path: "/:pathMatch(.*)*",
  redirect: "/",
});

const router = createRouter({
  // history avec base /app
  history: createWebHistory("/app"),
  routes,
});

export default router;
