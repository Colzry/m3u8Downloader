import { createApp } from "vue";
import App from "./App.vue";
import router from "@/router";
import { createPinia } from "pinia";
import piniaPersistedState from "pinia-plugin-persistedstate";
import { createNotificationPlugin } from "@/plugins/notificationPlugin.js";
import { createTauriSettingsPersistPlugin } from "@/plugins/settingsPersistPlugin.js";
import "reset-css";

const pinia = createPinia();
pinia.use(createNotificationPlugin());
pinia.use(piniaPersistedState); // 使用持久化插件
pinia.use(createTauriSettingsPersistPlugin()); // 设置持久化到 Tauri 中

createApp(App).use(router).use(pinia).mount("#app");
