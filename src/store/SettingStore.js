import { defineStore } from "pinia";

export const useSettingStore = defineStore("Setting", {
  state: () => ({
    downloadPath: "",
    downloadCount: 4, // 下载数
    threadCount: 16, // 线程数
    autoRetryDownload: false, // 一轮下载后仍有分片缺失时，是否自动重试整轮下载直到完成
    physicalCores: 1, // 物理核心数
    logicalCores: 4, // 逻辑核心数
    isDeleteDownloadFile: false, // 是否删除下载原文件
    deepLinkAutoDownload: false, // 深链（m3u8dl://）是否直接开始下载，false 时弹出新建下载界面确认
    enableNotification: true, // 下载完成是否弹出系统通知
    minimizeOnClose: true, // false 退出程序  true 最小化
    logLevel: "Info", // 日志级别
    lastRoute: "dList", // 上次关闭时的路由，用于重建窗口时恢复
  }),
  actions: {},
  persist: true, // 启用持久化
});
