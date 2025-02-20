import { defineStore } from 'pinia';
import {useSettingStore} from "@/store/SettingStore.js";
import {invoke} from "@tauri-apps/api/core";

const settingStore = useSettingStore()

export const useDownloadedStore = defineStore('Downloaded', {
  state: () => ({
    items: [], // 下载完成项列表
    selectedItems: [], // 选中的下载完成项ID列表
    
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
      this.items.unshift(item);
      this.updatePaginationTotal()
    },
    
    // 通过 ID 获取下载项
    getItemById(id) {
      return this.items.find((item) => item.id === id) || null;
    },
    
    // 移除下载项
    async removeItem(id) {
      const item = this.getItemById(id)
      this.items = this.items.filter((item) => item.id !== id);
      if (settingStore.isDeleteDownloadFile) {
        await invoke("delete_file", {filePath: item.file})
      }
      this.selectedItems = this.selectedItems.filter(i => i !== id);
      this.updatePaginationTotal()
      this.adjustCurrentPageAfterRemove()
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
  },
  persist: true // 启用持久化
});