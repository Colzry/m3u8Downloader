<script setup>
import {useMessage} from "naive-ui";
import { throttle } from 'lodash';

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
  isMerged: {
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

// 取消下载（保留临时目录）
const cancelTask = throttle(async () => {
  await downloadingStore.cancelDownload(props.id)
  message.success("已取消")
}, 1500, { leading: true, trailing: false})

// 继续下载（断点续传）
const continueTask = throttle(() => {
  message.success("开始下载")
  downloadingStore.continueDownload(props.id)
}, 1500, { leading: true, trailing: false})

// 开始下载
const startTask = throttle(() => {
  message.success("开始下载")
  downloadingStore.startDownload(props.id)
}, 1500, { leading: true, trailing: false})

// 删除任务（清理临时目录）
const deleteTask = throttle(async () => {
  if (props.isMerged) {
    await downloadedStore.removeItem(props.id)
  } else {
    await downloadingStore.removeItem(props.id)
  }
  message.success("删除成功")
}, 1500, { leading: true, trailing: false})
</script>

<template>
  <div class="item-ctr">
    <div class="check-wrap">
      <n-checkbox :checked="selected" @update-checked="handleCheckboxChange"/>
    </div>
    <div class="info-wrap">
      <div class="item-top">
        <div class="title text-ellipsis">{{ title }}</div>
        <!-- 下载中：显示取消按钮 -->
        <div class="operation-wrap" v-if="status === 2">
          <span class="opera-btn" @click="cancelTask">取消</span>
        </div>
        <!-- 其他状态：显示删除、继续下载等按钮 -->
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
          <!-- 未下载过：显示开始下载 -->
          <span class="opera-btn" v-if="!isMerged && status === 10" @click="startTask">开始下载</span>
          <!-- 已取消或已下载：显示继续下载 -->
          <span class="opera-btn" v-if="!isMerged && status === 0" @click="continueTask">继续下载</span>
          <!-- 等待中：显示取消等待 -->
          <span class="opera-btn" v-if="!isMerged && status === 1" @click="cancelTask">取消等待</span>
        </div>
      </div>
      <div class="progress-wrap" v-if="!isMerged && status === 2">
          <n-progress
              style="flex-grow: 1; min-width: 84%"
              type="line"
              :show-indicator="false"
              status="success"
              :percentage="progress"
              processing
          />
        <div class="progress-value">{{ progress }}%</div>
        <div class="speed tail" v-if="status === 0">已取消</div>
        <div class="speed tail" v-else-if="status === 2">{{ speed }}</div>
      </div>
      <div class="completed-warp" v-else>
        <div class="url-warp text-ellipsis">{{url}}</div>
        <div class="merge-status tail" v-if="status === 3">
          <span>下载完成</span>
        </div>
        <div class="merge-status tail" v-if="status === 4">
          <span>正在合并</span>
          <span class="slash-container">.</span>
          <span class="slash-rotating">/</span>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped lang="less">
.text-ellipsis {
  white-space: nowrap; /* 文字不换行 */
  overflow: hidden; /* 隐藏超出部分 */
  text-overflow: ellipsis; /* 超出部分显示省略号 */
}

.item-ctr {
  display: flex;
  padding: 15px 10px;
  background-color: #faf9f8;
  border-radius: 5px;
  width: 95%;
  height: 3rem;
  margin: 5px 0;
  .check-wrap {
    margin-right: 8px;
  }
  .info-wrap {
    width: 100%;
    overflow: hidden;
    box-sizing: border-box;
    .item-top {
      display: flex;
      padding-bottom: 10px;
      justify-content: space-between;
      .title {
        width: 80%;
      }
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
        min-width: 80px;
        white-space: nowrap;
      }
    }
    .completed-warp {
      width: 100%;
      display: flex;
      align-items: center;
      justify-items: center;
      justify-content: space-between;
      .url-warp {
        font-size: 0.8rem;
        line-height: 1rem;
        max-width: 90%;
        font-family: Arial, Helvetica, sans-serif !important;
      }
      .merge-status {
        font-family: sans-serif;
        display: inline-flex;
        align-items: center;
        gap: 2px;
        font-size: 0.8rem;
        margin-right: 5px;
        .slash-container {
          display: inline-block;
          width: 10px;
        }
        .slash-rotating {
          display: inline-block;
          animation: rotateSlash 1s infinite linear;
        }
        @keyframes rotateSlash {
          0% {
            transform: rotate(0deg);
          }
          100% {
            transform: rotate(360deg);
          }
        }
      }
      .tail {
        min-width: 70px;
        white-space: nowrap;
      }
    }

  }
}
</style>