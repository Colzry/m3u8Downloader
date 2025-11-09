import { defineStore } from "pinia";

export const useSettingStore = defineStore("Setting", {
    state: () => ({
        downloadPath: "",
        downloadCount: 1, // 下载数
        threadCount: 1, // 线程数
        physicalCores: 1, // 物理核心数
        logicalCores: 1, // 逻辑核心数
        isDeleteDownloadFile: false,
        minimizeOnClose: true, // false 退出程序  true 最小化
    }),
    actions: {},
    persist: true, // 启用持久化
});
