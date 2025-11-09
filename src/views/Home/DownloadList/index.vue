<script setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import HButton from "@/views/Home/components/HButton.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import DownloadItem from "@/views/Home/components/DownloadItem.vue";
import {validateM3u8Url} from "@/utils/m3u8Validator.js";
import {useMessage, useNotification } from "naive-ui";
 import {openFolder} from "@/utils/fs.js";
 import {throttle} from 'lodash';
 import {ref, reactive} from "vue";
 
 const message = useMessage();
const notification = useNotification()
const showModal = ref(false);
const formRef = ref(null);

const formValue = reactive({
  videoUrl: '',
  videoName: '',
  headers: {}
})

// 处理自定义 Headers 的响应式数据
const headerEntries = ref([{ key: '', value: '' }]);
const userAgentOptions = [
  { label: 'Chrome(Win)', value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36' },
  { label: 'Firefox', value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:143.0) Gecko/20100101 Firefox/143.0' },
  { label: 'Safari', value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15' },
  { label: 'Chrome(Linux)', value: 'Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36' },
  { label: 'Safari', value: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1' },
  { label: 'Edge', value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0' },
];
const resetHeaders = () => {
  headerEntries.value = [
    { key: 'User-Agent', value: userAgentOptions[0]?.value || '' },
  ];
};

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
            reject(result.message)
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

// 初始化一些数据
// for (let id = 1; id < 15; id++) {
//   downloadingStore.addItem({
//     id: id,
//     title: '西瓜-演示下载' + id,
//     progress: 80,
//     speed: '999.99 KB/s',
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
    updateHeadersObject()

    downloadingStore.addItem({
      id,
      title: formValue.videoName.trim(),
      progress: 0,
      status: 10,
      url: formValue.videoUrl.trim(),
      downloadPath: settingStore.downloadPath,
      headers: formValue.headers,
    });

    message.success("添加成功");
    showModal.value = false;
    return id; // 成功返回 ID
  } catch (errors) {
    return null; // 验证失败返回 null
  }
}, 2000, { leading: true, trailing: false})

const clickNewDownload = ()=> {
  showModal.value = true
  resetHeaders(); // 统一重置headers
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

// 添加 Header 输入框
const addHeader = () => {
  headerEntries.value.push({ key: '', value: '' });
};

// 移除 Header 输入框
const removeHeader = (index) => {
  if (headerEntries.value.length > 1) {
    headerEntries.value.splice(index, 1);
  } else {
      message.warning("默认配置，不允许删除");
  }
};

// 将 headerEntries 转换为 headers 对象
const updateHeadersObject = () => {
  const headers = {};
  headerEntries.value.forEach(entry => {
    if (entry.key.trim()) {
      headers[entry.key] = entry.value;
    }
  });
  formValue.headers = headers;
};

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
            :is-merged="item.isMerged"
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
      
        <!-- 高级选项折叠面板 -->
        <n-collapse>
          <n-collapse-item title="高级选项" name="advanced">
            <div>
              <p style="margin-bottom: 10px; color: #666;">自定义请求头（可选）</p>
              <div
                v-for="(header, index) in headerEntries" 
                :key="index" 
                style="margin-bottom: 10px; display: flex; align-items: center;"
              >
            <n-input 
                v-model:value="header.key" 
                placeholder="Key" 
                style="width: 37%; margin-right: 3%;"
                />
                <template v-if="header.key.trim().toLowerCase() === 'user-agent'">
                <n-select 
                    v-model:value="header.value" 
                    :options="userAgentOptions" 
                    style="width: 50%;"
                    placeholder="选择User-Agent"
                    allow-input
                />
                </template>
                <template v-else>
                <n-input 
                    v-model:value="header.value" 
                    placeholder="Value" 
                    style="width: 50%;"
                />
                </template>
                <n-button 
                  @click="removeHeader(index)" 
                  text 
                  type="error"
                  style="width: 10%;"
                >
                  ×
                </n-button>
              </div>
              <n-button @click="addHeader" size="small" text type="primary">
                + 添加 Header
              </n-button>
            </div>
          </n-collapse-item>
        </n-collapse>
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