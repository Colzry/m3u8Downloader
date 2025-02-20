import { createApp } from 'vue'
import App from './App.vue'
import router from "@/router";
import {createPinia} from 'pinia';
import piniaPersistedState from 'pinia-plugin-persistedstate';
import 'reset-css';

const pinia = createPinia();
pinia.use(piniaPersistedState); // 使用持久化插件

createApp(App)
  .use(router)
  .use(pinia)
  .mount('#app');
