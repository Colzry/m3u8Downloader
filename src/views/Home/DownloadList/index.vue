<script setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import HButton from "@/views/Home/components/HButton.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import DownloadItem from "@/views/Home/components/DownloadItem.vue";
import {useMessage} from "naive-ui";
import {openFolder} from "@/utils/fs.js";
import {ref} from "vue";

const message = useMessage();
const showModal = ref(false);
const formRef = ref(null);

const formValue = reactive({
  videoUrl: '',
  videoName: ''
})

const rules = {
  videoUrl: {
    required: true,
    message: "请输入合法的视频m3u8链接",
    trigger: "blur"
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
// if (downloadingStore.items.length < 5) {
//   downloadingStore.addItem({
//     id: '1',
//     title: '西瓜-演示下载',
//     progress: 80,
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

// 暂停选择的下载
const handlePauseSelected = async () => {
  if (handleSelectedNull()) return;
  for (const id of downloadingStore.selectedItems) {
    await downloadingStore.pauseItem(id);
  }
  downloadingStore.clearSelectedItems()
  message.success("暂停成功")
}

// 恢复暂停的下载
const handleResumeSelected = async () => {
  if (handleSelectedNull()) return;
  for (const id of downloadingStore.selectedItems) {
    await downloadingStore.resumeItem(id);
  }
  downloadingStore.clearSelectedItems()
  message.success("开始下载")
}

const cancelAddDownloadHandle = () => {
  Object.keys(formValue).forEach(key => {
    delete formValue[key];
  });
  message.success("已取消");
  showModal.value = false;
}
const addToListHandle = async () => {
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
    });

    message.success("添加成功");
    showModal.value = false;
    return id; // 成功返回 ID
  } catch (errors) {
    console.log(errors);
    return null; // 验证失败返回 null
  }
}
const clickNewDownload = () => {
  showModal.value = true
  Object.keys(formValue).forEach(key => {
    delete formValue[key];
  });
}
const nowDownloadHandle = async () => {
  const id = await addToListHandle()
  if (id) {
    await downloadingStore.startDownload(id)
  }
}

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
                negative-text="取消"
                @positive-click="handlePauseSelected"
            >
              <template #trigger>
                <n-button size="small" type="info" ghost>暂停</n-button>
              </template>
              你确认要暂停吗？
            </n-popconfirm>
            <n-popconfirm
                positive-text="确认"
                negative-text="取消"
                @positive-click="handleResumeSelected"
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
      <n-button size="small" type="primary" @click="nowDownloadHandle">立即下载</n-button>
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