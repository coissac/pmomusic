import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";

// Service SSE (les composables se connectent automatiquement)
import { sse } from "./services/pmocontrol/sse";

// Store UI (garde UIStore pour les notifications et état UI global)
import { useUIStore } from "./stores/ui";

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

// Après montage, initialiser SSE
const uiStore = useUIStore();

// Les composables se connectent automatiquement à SSE
// Ils gèrent eux-mêmes le re-fetch lors des événements

sse.onConnectionChange((connected) => {
  uiStore.setSSEConnected(connected);
  if (connected) {
    console.log("[App] SSE connecté");
  }
});

// Démarrer la connexion SSE
sse.connect();

console.log("[App] PMOControl initialisé");
