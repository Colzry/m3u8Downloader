<script setup>
import {useMessage} from "naive-ui";

const message = useMessage()
const props = defineProps({
  id: {
    type: String,
    required: true,
  },
  title: {
    type: String,
    required: true,
  },
  progress: {
    type: Number,
    required: true,
  },
  isDownloaded: {
    type: Boolean,
    required: true,
  },
  isMerge: {
    type: Boolean,
    default: false,
  },
  status: {
    type: Number,
    default: 0,
  },
  url: {
    type: String,
    default: '',
  },
  speed: {
    type: String,
    default: '0kB/s',
  },
  selected: {
    type: Boolean,
    default: false,
  },
});
import { useDownloadingStore } from '@/store/DownloadingStore';
import { useDownloadedStore } from '@/store/DownloadedStore';
const downloadingStore = useDownloadingStore();
const downloadedStore = useDownloadedStore();

const emit = defineEmits(['select']);

const handleCheckboxChange = (checked) => {
  emit('select', props.id, checked);
};

const pauseTask = async () => {
  await downloadingStore.pauseItem(props.id)
  message.success("暂停成功")
}

const resumeTask = async () => {
  await downloadingStore.resumeItem(props.id)
  message.success("开始下载")
}

const startTask = async () => {
  await downloadingStore.startDownload(props.id)
}

const deleteTask = async () => {
  if (props.isMerge) {
    await downloadedStore.removeItem(props.id)
  } else {
    await downloadingStore.removeItem(props.id)
  }
  message.success("删除成功")
}
</script>

<template>
  <div class="item-ctr">
    <div class="check-wrap">
      <n-checkbox :checked="selected" @update-checked="handleCheckboxChange"/>
    </div>
    <div class="info-wrap">
      <div class="item-top">
        <div class="title">{{ title }}</div>
        <div class="operation-wrap" v-if="status === 2">
          <span class="opera-btn" @click="pauseTask">暂停</span>
        </div>
        <div class="operation-wrap" v-else>
          <n-popconfirm
              positive-text="确认"
              negative-text="取消"
              @positive-click="deleteTask"
          >
            <template #trigger>
              <span class="opera-btn">删除</span>
            </template>
            你确认要删除吗？
          </n-popconfirm>
          <span class="opera-btn" v-if="!isMerge&&isDownloaded&&status!==4" @click="resumeTask">恢复下载</span>
          <span class="opera-btn" v-if="!isMerge&&!isDownloaded&&status!==4" @click="startTask">开始下载</span>
          <span class="opera-btn" v-if="status===4" @click="pauseTask">取消等待</span>
        </div>
      </div>
      <div class="progress-wrap" v-if="!isMerge&&isDownloaded">
          <n-progress
              style="flex-grow: 1; min-width: 84%"
              type="line"
              :show-indicator="false"
              status="success"
              :percentage="progress"
              processing
          />
        <div class="progress-value">{{ progress }}%</div>
        <div class="speed tail" v-if="status === 1">下载暂停</div>
        <div class="speed tail" v-else-if="status === 2">{{ speed }}</div>
        <div class="speed tail" v-else-if="status === 3">正在合并</div>
        <div class="speed tail" v-else-if="status === 4">等待下载</div>
        <div class="status tail" v-else>{{isMerge ? '正在合并' : '下载暂停'}}</div>
      </div>
      <div class="url-warp" v-else>
        {{url}}
      </div>
    </div>

  </div>
</template>

<style scoped lang="less">
.item-ctr {
  display: flex;
  padding: 18px 10px;
  background-color: #faf9f8;
  border-radius: 5px;
  width: 95%;
  height: 2.5rem;
  margin: 5px 0;
  .check-wrap {
    margin-right: 8px;
  }
  .info-wrap {
    width: 100%;
    .item-top {
      display: flex;
      padding-bottom: 10px;
      justify-content: space-between;
      .operation-wrap {
        display: flex;
        flex-direction: row;
        gap: 10px;
        margin-right: 20px;
        .opera-btn {
          display: inline-block;
          font-size: 0.8rem;
          cursor: pointer;
          padding: 5px;
          transition: color .4s;
          color: #666;
          &:hover {
            color: #1ba059;
          }
        }
      }
    }
    .progress-wrap {
      width: 100%;
      display: flex;
      align-items: center;
      justify-items: center;
      font-size: 0.8rem;
      .progress-value {
        padding: 0 10px;
      }
      .tail {
        min-width: 70px;
        white-space: nowrap;
      }
    }
    .url-warp {
      font-size: 0.8rem;
      line-height: 1rem;
      white-space: nowrap; /* 强制文本不换行 */
      overflow: hidden; /* 超出部分隐藏 */
      text-overflow: ellipsis; /* 超出部分用省略号显示 */
    }
  }
}
</style>