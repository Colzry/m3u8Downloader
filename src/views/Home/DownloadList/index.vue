<script setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import HButton from "@/views/Home/components/HButton.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import DownloadItem from "@/views/Home/components/DownloadItem.vue";
import {validateM3u8Url} from "@/utils/m3u8Validator.js";
import {useMessage, useNotification } from "naive-ui";
import {openFolder} from "@/utils/fs.js";
import {throttle} from 'lodash';
import {ref} from "vue";

const message = useMessage();
const notification = useNotification()
const showModal = ref(false);
const formRef = ref(null);

const formValue = reactive({
  videoUrl: '',
  videoName: ''
})
const rules = {
  videoUrl: {
    required: true,
    trigger: ['blur'],
    validator: (rule, value) => {
      return new Promise(async (resolve, reject) => {
        // 必填校验
        if (!value?.trim()) {
          reject(new Error('请输入视频m3u8链接'))
        }
        // URL格式校验
        if (!/^https?:\/\//i.test(value)) {
          reject(new Error('请输入正确格式的m3u8链接'))
        }
        try {
          const result = await validateM3u8Url(value, {
            checkContent: true,
            timeout: 6000
          });

          if (!result.valid) {
            reject(new Error('此m3u8地址无效'))
          }
          resolve()
        } catch (e) {
          reject(new Error(`验证失败：${e.message}`))
        }
      })
    },
  },
  videoName: {
    required: true,
    message: "请输入视频名称",
    trigger: "blur"
  }
}

import { useDownloadingStore } from '@/store/DownloadingStore';
import { useSettingStore } from "@/store/SettingStore.js";
const downloadingStore = useDownloadingStore();
const settingStore = useSettingStore();

// 初始化一些数据（可选）
// for (let id = 1; id < 15; id++) {
//   downloadingStore.addItem({
//     id: id,
//     title: '西瓜-演示下载' + id,
//     progress: 80,
//     speed: '999.99 KB/s',
//     isDownloaded: false,
//     status: 1,
//     url: 'https://sf1-cdn-tos.huoshanstatic.com/obj/media-fe/xgplayer_doc_video/hls/xgplayer-demo.m3u8',
//   });
// }
// 计算属性
const items = computed(() => downloadingStore.paginatedItems);
const isAllSelected = computed(() => downloadingStore.selectedItems.length === downloadingStore.items.length && downloadingStore.selectedItems.length !== 0);
const isItemSelected = (id) => downloadingStore.selectedItems.includes(id);

const handleSelectAll = (checked) => {
  if (checked) {
    downloadingStore.selectAll();
  } else {
    downloadingStore.unselectAll();
  }
};

const handleSelectItem = (id, selected) => {
  downloadingStore.toggleItemSelection(id);
};

const handleSelectedNull = () => {
  if (downloadingStore.selectedItems.length === 0) {
    message.warning('请先选择需要操作的选项')
    return true
  }
  return false
}

// 删除选择的下载
const handleDeleteSelected = async () => {
  if (handleSelectedNull()) return;
  for (const id of downloadingStore.selectedItems) {
    await downloadingStore.removeItem(id);
  }
  downloadingStore.clearSelectedItems()
  message.success("删除成功")
};

// 取消选择的下载
const handleCancelSelected = async () => {
  if (handleSelectedNull()) return;
  for (const id of downloadingStore.selectedItems) {
    downloadingStore.cancelDownload(id).then(() => {})
  }
  downloadingStore.clearSelectedItems()
  message.success("已取消")
}

// 下载选中的项
const handleDownloadSelected = async () => {
  if (handleSelectedNull()) return;
  // 将下载项加入等待任务中
  for (const id of downloadingStore.selectedItems) {
    if (downloadingStore.getItemById(id)?.status === 2) continue;
    downloadingStore.updateItem(id, { status: 1 }); // 1 表示等待中
  }
  // 清除选中列表
  downloadingStore.clearSelectedItems()
  // 开始下载等待中的任务
  downloadingStore.tryStartNextDownloads().then(() => {})
  message.success("开始下载")
}

const cancelAddDownloadHandle = () => {
  Object.keys(formValue).forEach(key => {
    delete formValue[key];
  });
  message.success("已取消");
  showModal.value = false;
}

const addToListHandle = throttle(async () => {
  try {
    await formRef.value?.validate();
    const id = crypto.randomUUID();

    downloadingStore.addItem({
      id,
      title: formValue.videoName,
      progress: 0,
      isDownloaded: false,
      status: 0,
      url: formValue.videoUrl,
      downloadPath: settingStore.downloadPath,
    });

    message.success("添加成功");
    showModal.value = false;
    return id; // 成功返回 ID
  } catch (errors) {
    return null; // 验证失败返回 null
  }
}, 1000)

const clickNewDownload = ()=> {
  showModal.value = true
  Object.keys(formValue).forEach(key => {
    delete formValue[key];
  });
}

const d_loading = ref(false);
const nowDownloadHandle = async () => {
  d_loading.value = true
  const id = await addToListHandle()
  if (id) {
    downloadingStore.startDownload(id).catch(
        err => {
          downloadingStore.cancelDownload(id);
          notification.error({
            content: downloadingStore.getItemById(id).title + '下载失败',
            meta: err,
            // duration: 5000,
            keepAliveOnHover: true
          })
        }
    )
  }
  d_loading.value = false
}


import { getCurrentWindow } from '@tauri-apps/api/window';
let unlisten = null; // 用于清理监听器

// 窗口 resize 事件回调
const handleWindowResized = async () => {
  const appWindow = getCurrentWindow();
  const isMaximized = await appWindow.isMaximized();

  if (isMaximized) {
    // 最大化时，增加分页大小以显示更多项
    downloadingStore.pagination.pageSize = 12;
  } else {
    // 还原时，恢复默认大小
    downloadingStore.pagination.pageSize = 5;
  }

  // 重置当前页
  downloadingStore.pagination.currentPage = 1;
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
  <page-header title="下载列表">
    <template #extra>
      <h-button
          label="下载目录"
          @click="openFolder(settingStore.downloadPath)"
      />
      <h-button
          label="新建下载"
          @click="clickNewDownload"
      />
    </template>
  </page-header>

  <main-wrapper>
    <div class="list-ctr">
      <div class="empty-ctr" v-if="downloadingStore.items.length === 0">
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
            <n-popconfirm
                positive-text="确认"
                negative-text="算了"
                @positive-click="handleCancelSelected"
            >
              <template #trigger>
                <n-button size="small" type="warning" ghost>取消</n-button>
              </template>
              你确认要取消选中的下载吗？
            </n-popconfirm>
            <n-popconfirm
                positive-text="确认"
                negative-text="取消"
                @positive-click="handleDownloadSelected"
            >
              <template #trigger>
                <n-button size="small" type="primary" ghost>下载</n-button>
              </template>
              你确认要下载吗？
            </n-popconfirm>
          </div>
        </div>
        <download-item
            v-for="item in items"
            :key="item.id"
            :id="item.id"
            :title="item.title"
            :progress="item.progress"
            :is-downloaded="item.isDownloaded"
            :is-merge="item.isMerge"
            :status="item.status"
            :url="item.url"
            :speed="item.speed"
            :selected="isItemSelected(item.id)"
            @select="handleSelectItem"
        />
      </div>
      <!-- 分页控件 -->
      <n-pagination
          v-if="downloadingStore.totalPages > 1"
          v-model:page="downloadingStore.pagination.currentPage"
          :page-count="downloadingStore.totalPages"
          @update:page="downloadingStore.setCurrentPage"
          style="position: absolute; bottom: 10px; right: 15px"
      />
    </div>
  </main-wrapper>


  <n-modal
      v-model:show="showModal"
      :mask-closable="false"
      :show-icon="false"
      preset="dialog"
  >
    <template #header>
      <div>新建下载</div>
    </template>
    <n-form
        ref="formRef"
        label-placement="left"
        label-width="auto"
        :model="formValue"
        :rules="rules"
    >
      <n-form-item label="视频链接" path="videoUrl">
        <n-input v-model:value="formValue.videoUrl" placeholder="请输入视频m3u8链接"/>
      </n-form-item>
      <n-form-item label="视频名称" path="videoName">
        <n-input v-model:value="formValue.videoName" placeholder="请输入视频名称"/>
      </n-form-item>
    </n-form>
    <template #action>
      <n-button size="small" ghost @click="cancelAddDownloadHandle">取消</n-button>
      <n-button size="small" type="info" ghost @click="addToListHandle">加入列表</n-button>
      <n-button :loading="d_loading" size="small" type="primary" @click="nowDownloadHandle">立即下载</n-button>
    </template>
  </n-modal>
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