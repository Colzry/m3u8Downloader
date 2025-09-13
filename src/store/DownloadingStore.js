import { defineStore } from 'pinia';
import {invoke} from "@tauri-apps/api/core";
import {useDownloadedStore} from "@/store/DownloadedStore.js";
import {useSettingStore} from "@/store/SettingStore.js";
import {listen} from "@tauri-apps/api/event";

const downloadedStore = useDownloadedStore()
const settingStore = useSettingStore()

export const useDownloadingStore = defineStore('Downloading', {
  
  /**
   status 0-已取消 1-已暂停 2-下载中 3-合并中 4-等待中
   **/
  
  state: () => ({
    items: [], // 下载项列表
    selectedItems: [], // 选中的下载项ID列表
    
    // 存储所有任务的监听器 { [taskId]: [listenFn1, listenFn2] }
    taskListeners: {},
    
    pagination: {
      currentPage: 1,
      pageSize: 5,
      total: 0
    }
  }),
  
  
  getters: {
    // 分页后的数据
    paginatedItems: (state) => {
      const start = (state.pagination.currentPage - 1) * state.pagination.pageSize
      const end = start + state.pagination.pageSize
      return state.items.slice(start, end)
    },
    
    // 总页数
    totalPages: (state) => {
      return Math.ceil(state.items.length / state.pagination.pageSize)
    }
  },
  
  
  actions: {
    setCurrentPage(page) {
      this.pagination.currentPage = Math.max(1,
        Math.min(page, this.totalPages)
      )
    },
    
    setPageSize(size) {
      this.pagination.pageSize = size
      this.pagination.currentPage = 1
      this.updatePaginationTotal()
    },
    
    updatePaginationTotal() {
      this.pagination.total = this.items.length
    },
    
    // 删除元素后调整页码
    adjustCurrentPageAfterRemove() {
      if (this.paginatedItems.length === 0 && this.pagination.currentPage > 1) {
        this.setCurrentPage(this.pagination.currentPage - 1)
      }
    },
    
    // 添加下载项
    addItem(item) {
      this.items.push(item);
      this.updatePaginationTotal()
    },
    
    // 通过 ID 获取下载项
    getItemById(id) {
      return this.items.find((item) => item.id === id) || null;
    },
    
    // 检查最大下载数，如果达到最大并发数，设置为等待状态
    checkMaxDownloads(id) {
      // 获取当前活跃任务数
      const activeCount = this.items.filter(
        item => item.status === 2 // 2 表示下载中
      ).length;
      
      // 如果达到最大并发数，设置为等待状态
      if (activeCount >= settingStore.downloadCount) {
        this.updateItem(id, { status: 4 }); // 4 表示等待中
        return true;
      }
      return false;
    },
    
    
    async pauseItem(id) {
      const item = this.getItemById(id)
      if (item?.status === 2) {
        await invoke('pause_download', { id });
        this.updateItem(id, { status: 1 }); // 1 表示暂停
        await this.tryStartNextDownloads(); // 触发队列检查
      }
      if (item?.status === 4) {
        this.updateItem(id, { status: 1 }); // 1 表示暂停
        await this.tryStartNextDownloads(); // 触发队列检查
      }
    },
    
    async resumeItem(id) {
      // 如果达到最大并发数，设置为等待状态
      if(this.checkMaxDownloads(id)) return;
      
      const item = this.getItemById(id)
      if (item?.isDownloaded) {
        await invoke('resume_download', {id})
      } else {
        await this.startDownload(id)
      }
      this.updateItem(id, {status: 2})
    },
    
    // 移除下载项
    async removeItem(id) {
      const item = this.getItemById(id)
      const wasActive = item?.status === 2;
      if (item.isCreatedTempDir) { // 创建了临时目录
        invoke("delete_download", {id}).catch()
      }
      this.cleanupTaskListeners(id)
      this.items = this.items.filter(i => i.id !== id);
      this.selectedItems = this.selectedItems.filter(i => i !== id);
      this.updatePaginationTotal()
      this.adjustCurrentPageAfterRemove()
      if (wasActive) {
        await this.tryStartNextDownloads();
      }
    },
    
    // 全选
    selectAll() {
      this.selectedItems = this.items.map((item) => item.id);
    },
    
    // 取消全选
    unselectAll() {
      this.selectedItems = [];
    },
    
    // 添加或移除单个选项
    toggleItemSelection(id) {
      const index = this.selectedItems.indexOf(id);
      if (index === -1) {
        this.selectedItems.push(id); // 如果未选中，则添加
      } else {
        this.selectedItems.splice(index, 1); // 如果已选中，则移除
      }
    },
    
    // 更新下载项
    updateItem(id, updates) {
      const item = this.items.find((item) => item.id === id);
      if (item) {
        // 将 updates 中的属性添加或覆盖到 item 中
        Object.keys(updates).forEach((key) => {
          item[key] = updates[key]; // 直接设置属性值，添加或覆盖
        });
      }
    },
    
    clearSelectedItems() {
      this.selectedItems = [];
    },
    
    // 清理指定任务的监听器
    cleanupTaskListeners(taskId) {
      if (this.taskListeners[taskId]) {
        this.taskListeners[taskId].forEach(listen => listen?.());
        delete this.taskListeners[taskId];
      }
    },
    
    // 启动新下载任务
    async startDownload(taskId) {
      // 如果达到最大并发数，设置为等待状态
      if(this.checkMaxDownloads(taskId)) return;
      
      // 清理已有监听器
      this.cleanupTaskListeners(taskId);
      
      // 创建事件监听器
      const [listenCreateDir, listenProgress, listenMerge] = await Promise.all([
        listen('create_temp_directory', (event) => {
          const data = event.payload;
          if (data.id === taskId && data.isCreatedTempDir) {
            // 更新状态示例（需要你实际的状态管理逻辑）
            const item = this.items.find(item => item.id === taskId);
            if (item) {
              item.isCreatedTempDir = data.isCreatedTempDir;
            }
          }
        }),
        listen('download_progress', (event) => {
          const data = event.payload;
          if (data.id === taskId) {
            this.updateItem(data.id, {...data})
          }
        }),
        listen('merge_video', (event) => {
          // 开始合并，触发队列检查，继续下载下一个
          this.tryStartNextDownloads();
          
          const data = event.payload;
          if (data.id === taskId && data.isMerge) {
            // 迁移数据示例
            const item = this.items.find(i => i.id === taskId);
            if (item) {
              Object.keys(data).forEach((key) => {
                item[key] = data[key]; // 直接设置属性值，添加或覆盖
              });
              downloadedStore.addItem(item);
              this.items = this.items.filter(
                item => item.id !== taskId
              );
              this.cleanupTaskListeners(taskId);
            }
          }
        })
      ]);
      
      // 存储监听器
      this.taskListeners[taskId] = [listenCreateDir, listenProgress, listenMerge];
      
      const item = this.getItemById(taskId)
      if (item) {
        this.updateItem(item.id, { isDownloaded: true, status: 2 });
        await invoke("start_download", {
          id: item.id,
          url: item.url,
          name: item.title,
          outputDir: settingStore.downloadPath,
          threadCount: settingStore.threadCount,
        });
      }
    },
    
    // 尝试启动等待中的任务
    async tryStartNextDownloads() {
      const settingStore = useSettingStore();
      
      // 计算可用槽位
      const activeCount = this.items.filter(
        item => item.status === 2
      ).length;
      const availableSlots = settingStore.downloadCount - activeCount;
      
      if (availableSlots <= 0) return;
      
      // 找到前 N 个等待中的任务
      const waitingTasks = this.items
        .filter(item => item.status === 4)
        .slice(0, availableSlots);
      
      // 启动这些任务
      for (const task of waitingTasks) {
        const item = this.getItemById(task.id)
        if (item?.isDownloaded) {
          await invoke('resume_download', {id: task.id})
        } else {
          await this.startDownload(task.id)
        }
        this.updateItem(task.id, {isDownloaded: true, status: 2})
      }
    },
  },
  persist: true // 启用持久化
});