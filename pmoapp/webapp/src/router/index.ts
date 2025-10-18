import { createRouter, createWebHistory } from "vue-router";
import HelloWorld from "../components/HelloWorld.vue";
import LogView from "../components/LogView.vue";
import CoverCacheManager from "../components/CoverCacheManager.vue";
import AudioCacheManager from "../components/AudioCacheManager.vue";
import UpnpExplorer from "../components/UpnpExplorer.vue";
import APIDashboard from "../components/APIDashboard.vue";

const routes = [
  { path: "/", name: "home", component: HelloWorld },
  { path: "/logs", name: "logs", component: LogView },
  { path: "/covers-cache", name: "covers-cache", component: CoverCacheManager },
  { path: "/audio-cache", name: "audio-cache", component: AudioCacheManager },
  { path: "/upnp", name: "upnp", component: UpnpExplorer },
  { path: "/api-dashboard", name: "api-dashboard", component: APIDashboard },
];

const router = createRouter({
  // history avec base /app
  history: createWebHistory("/app"),
  routes,
});

export default router;
