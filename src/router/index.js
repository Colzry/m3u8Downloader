import { createRouter, createWebHashHistory } from 'vue-router'
import homeRoutes from "@/router/home/index.js";

const routes = [
  {
    path: '/',
    redirect: '/dList'
  },
  ...homeRoutes
];

const router = createRouter({
  history: createWebHashHistory(),
  routes
});

export default router;
