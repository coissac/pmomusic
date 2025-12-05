import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";

// Stores
import { useRenderersStore } from "./stores/renderers";
import { useMediaServersStore } from "./stores/mediaServers";
import { usePlaybackStore } from "./stores/playback";
import { useUIStore } from "./stores/ui";

// Service SSE
import { sse } from "./services/pmocontrol/sse";

// Styles
import "./style.css";
import "./assets/styles/variables.css";
import "./assets/styles/pmocontrol.css";

// Créer l'application Vue
const app = createApp(App);

// Créer et installer Pinia
const pinia = createPinia();
app.use(pinia);
app.use(router);

// Monter l'application
app.mount("#app");

// Après montage, initialiser SSE et connecter aux stores
const renderersStore = useRenderersStore();
const mediaServersStore = useMediaServersStore();
const playbackStore = usePlaybackStore();
const uiStore = useUIStore();

// Connecter SSE aux stores
sse.onRendererEvent((event) => {
  renderersStore.updateFromSSE(event);
  playbackStore.updateFromSSE(event);
});

sse.onMediaServerEvent((event) => {
  mediaServersStore.updateFromSSE(event);
});

sse.onConnectionChange((connected) => {
  uiStore.setSSEConnected(connected);
  if (connected) {
    console.log("[App] SSE connecté - Chargement des données initiales");
    // Charger les données initiales
    renderersStore.fetchRenderers();
    mediaServersStore.fetchServers();

    // Recharger la queue du renderer sélectionné s'il y en a un
    if (uiStore.selectedRendererId) {
      console.log(`[App] Rechargement de la queue du renderer ${uiStore.selectedRendererId}`);
      renderersStore.fetchQueue(uiStore.selectedRendererId);
    }
  }
});

// Démarrer la connexion SSE
sse.connect();

console.log("[App] PMOControl initialisé");
