import { defineStore } from 'pinia';

export const useSettingStore = defineStore('Setting', {
  state: () => ({
    downloadPath: '',
    threadCount: 4,
    downloadCount: 4,
    isDeleteDownloadFile: false,
    minimizeOnClose: true, // false 退出程序  true 最小化
  }),
  actions: {
  },
  persist: true // 启用持久化
});