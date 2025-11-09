<script setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import HButton from "@/views/Home/components/HButton.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import { useDownloadedStore } from '@/store/DownloadedStore';
import { useSettingStore } from "@/store/SettingStore.js";
import DownloadItem from "@/views/Home/components/DownloadItem.vue";
import {openFolder} from "@/utils/fs.js";
import {useMessage} from "naive-ui";

const downloadedStore = useDownloadedStore();
const settingStore = useSettingStore()

// 计算属性
const items = computed(() => downloadedStore.paginatedItems);
const isAllSelected = computed(() => downloadedStore.selectedItems.length === downloadedStore.items.length && downloadedStore.selectedItems.length !== 0);
const isItemSelected = (id) => downloadedStore.selectedItems.includes(id);

const handleSelectAll = (checked) => {
  if (checked) {
    downloadedStore.selectAll();
  } else {
    downloadedStore.unselectAll();
  }
};

const handleSelectItem = (id, selected) => {
  downloadedStore.toggleItemSelection(id);
};

const message = useMessage();
const handleDeleteSelected = () => {
  if (downloadedStore.selectedItems.length === 0) {
    message.warning('请先选择需要操作的选项')
    return;
  }
  downloadedStore.selectedItems.forEach((id) => {
    downloadedStore.removeItem(id);
  });
  downloadedStore.clearSelectedItems()
  message.success("删除成功")
};


import { getCurrentWindow } from '@tauri-apps/api/window';
let unlisten = null; // 用于清理监听器

// 窗口 resize 事件回调
const handleWindowResized = async () => {
  const appWindow = getCurrentWindow();
  const isMaximized = await appWindow.isMaximized();

  if (isMaximized) {
    // 最大化时，增加分页大小以显示更多项
    downloadedStore.pagination.pageSize = 12;
  } else {
    // 还原时，恢复默认大小
    downloadedStore.pagination.pageSize = 5;
  }

  // 重置当前页
  downloadedStore.pagination.currentPage = 1;
};

onMounted(async () => {
  const appWindow = getCurrentWindow();
  // 监听 resize 事件
  unlisten = await appWindow.onResized(handleWindowResized);
});

onUnmounted(() => {
  if (unlisten) {
    unlisten(); // 清理监听器
  }
});
</script>

<template>
  <page-header title="下载完成">
    <template #extra>
      <h-button
          label="下载目录"
          @click="openFolder(settingStore.downloadPath)"
      />
    </template>
  </page-header>

  <main-wrapper>
    <div class="list-ctr">
      <div class="empty-ctr" v-if="downloadedStore.items.length === 0">
        <n-empty size="large" description="暂无数据"/>
      </div>

      <div class="list-wrap" v-else>
        <div class="multi-choice-ctr">
          <div class="check-wrap">
            <n-checkbox
                :checked="isAllSelected"
                @update-checked="handleSelectAll"
                label="全选"
            />
          </div>
          <div class="opera-ctr">
            <n-popconfirm
                positive-text="确认"
                negative-text="取消"
                @positive-click="handleDeleteSelected"
            >
              <template #trigger>
                <n-button size="small" type="error" ghost>删除</n-button>
              </template>
              你确认要删除吗？
            </n-popconfirm>
          </div>
        </div>
        <download-item
            v-for="item in items"
            :key="item.id"
            :id="item.id"
            :title="item.title"
            :progress="item.progress"
            :status="item.status"
            :is-merged="item.isMerged"
            :url="item.url"
            :selected="isItemSelected(item.id)"
            @select="handleSelectItem"
        />
      </div>
      <!-- 分页控件 -->
      <n-pagination
          v-if="downloadedStore.totalPages > 1"
          v-model:page="downloadedStore.pagination.currentPage"
          :page-count="downloadedStore.totalPages"
          @update:page="downloadedStore.setCurrentPage"
          style="position: absolute; bottom: 10px; right: 15px"
      />
    </div>
  </main-wrapper>
</template>

<style scoped lang="less">
.list-ctr {
  margin-top: 1rem;
  min-height: calc(100% - 1rem);
  border-radius: 5px;
  background-color: #fff;
  position: relative;

  .empty-ctr {
    display: flex;
    justify-content: center; /* 控制垂直方向上的对齐 */
    align-items: center; /* 控制水平方向上的对齐 */
    height: 84vh;
  }

  .list-wrap {
    display: flex;
    flex-direction: column; /* 设置主轴为垂直方向 */
    //justify-content: center; /* 控制垂直方向上的对齐 */
    align-items: center; /* 控制水平方向上的对齐 */
    width: 100%;
    .multi-choice-ctr {
      width: 95%;
      display: flex;
      justify-content: space-between;
      align-items: center;
      //background-color: #e2e2e2;
      padding: 1rem 0;

      .opera-ctr {
        display: flex;
        flex-direction: row;
        gap: 10px;
      }
    }
  }
}
</style>