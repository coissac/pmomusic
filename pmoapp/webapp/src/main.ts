import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";

// Service SSE (les composables se connectent automatiquement)
import { sse } from "./services/pmocontrol/sse";

// Store UI (garde UIStore pour les notifications et état UI global)
import { useUIStore } from "./stores/ui";

// Image cache (pour cleanup)
import { imageCache } from "./composables/imageCache";

// Styles
import "./style.css";
import "./assets/styles/variables.css";
import "./assets/styles/pmocontrol.css";
import "./assets/styles/glass-theme.css";
import "./assets/styles/drawers.css";

// Créer l'application Vue
const app = createApp(App);

// Créer et installer Pinia
const pinia = createPinia();
app.use(pinia);
app.use(router);

// Initialiser UIStore AVANT le montage pour éviter la race condition (P2)
const uiStore = useUIStore();

// Monter l'application
app.mount("#app");

// Après montage, initialiser SSE
// Les composables se connectent automatiquement à SSE
// Ils gèrent eux-mêmes le re-fetch lors des événements

sse.onConnectionChange((connected) => {
  uiStore.setSSEConnected(connected);
});

// Démarrer la connexion SSE
sse.connect();

// Cleanup global lors du unload de la page
window.addEventListener('beforeunload', () => {
  imageCache.destroy();
});
