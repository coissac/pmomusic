import { createRouter, createWebHistory } from "vue-router";
import HelloWorld from "../components/HelloWorld.vue";
import LogView from "../components/LogView.vue";

const routes = [
  { path: "/", name: "home", component: HelloWorld },
  { path: "/logs", name: "logs", component: LogView },
];

const router = createRouter({
  // history avec base /app
  history: createWebHistory("/app"),
  routes,
});

export default router;
