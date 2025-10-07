import { createRouter, createWebHistory } from "vue-router";
import HelloWorld from "../components/HelloWorld.vue";
import LogView from "../components/LogView.vue";
import CoverCacheManager from "../components/CoverCacheManager.vue";

const routes = [
  { path: "/", name: "home", component: HelloWorld },
  { path: "/logs", name: "logs", component: LogView },
  { path: "/covers-cache", name: "covers-cache", component: CoverCacheManager },
];

const router = createRouter({
  // history avec base /app
  history: createWebHistory("/app"),
  routes,
});

export default router;
