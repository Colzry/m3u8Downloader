import { defineStore } from "pinia";

export const useUIStore = defineStore("UI", {
    state: () => ({
        collapsed: false, // collapsed 状态，控制侧边栏是否收起
    }),
    actions: {
        toggleCollapsed() {
            this.collapsed = !this.collapsed; // 切换 collapsed 状态
        },
    },
});
