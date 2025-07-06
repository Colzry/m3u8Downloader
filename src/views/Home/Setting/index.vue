<script async setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import {useSettingStore} from "@/store/SettingStore.js";
import { open } from '@tauri-apps/plugin-dialog';
import {invoke} from "@tauri-apps/api/core";

const version = import.meta.env.VITE_APP_VERSION;
const settingStore = useSettingStore()

const selectFolder = async () => {
  const selectDirectory = await open({
    directory: true,
    multiple: false,
    title: '选择下载目录'
  });
  if (selectDirectory) {
    settingStore.downloadPath = selectDirectory
  }
}
watch(
    () => settingStore.minimizeOnClose,
    (newVal, _oldVal) => {
      invoke("set_minimize_on_close", {minimizeOnClose: newVal})
    }
)
</script>

<template>
  <page-header title="软件设置"/>
  
  <main-wrapper>
    <div class="base-setting set-wrap">
      <div class="b-title title">基本设置</div>
      <div class="set-items-wrap">
        <div class="set-item">
          <div class="set-label">
            <div class="select-dir" @click="selectFolder">选择下载文件夹</div>
          </div>
          <div class="set-value">
            <n-input
                type="text"
                size="small"
                style="max-width: 350px"
                :value="settingStore.downloadPath"
                :disabled="true"
            />
          </div>
        </div>

        <div class="set-item">
          <div class="set-label">
            <div>删除已下载同时删除原文件</div>
          </div>
          <div class="set-value">
            <n-switch size="small" v-model:value="settingStore.isDeleteDownloadFile" />
          </div>
        </div>

        <div class="set-item">
          <div class="set-label">
            <div>关闭主窗口</div>
          </div>
          <div class="set-value">
            <n-radio-group v-model:value="settingStore.minimizeOnClose" name="closeTheWindow">
              <n-space>
                <n-radio :value="false">退出程序</n-radio>
                <n-radio :value="true">最小化</n-radio>
              </n-space>
            </n-radio-group>
          </div>
        </div>
      </div>
    </div>

    <div class="download-setting set-wrap">
      <div class="d-title title">下载设置</div>
      <div class="set-items-wrap">
        <div class="set-item">
          <div class="set-label">最大同时下载数</div>
          <div class="set-value">
            <n-input-number
                size="small"
                style="max-width: 100px"
                v-model:value="settingStore.downloadCount"
                placeholder="下载数"
                :min="1"
                :max="12"
            />
          </div>
        </div>
        <div class="set-item">
          <div class="set-label">下载线程数</div>
          <div class="set-value">
            <n-input-number
                size="small"
                style="max-width: 100px"
                v-model:value="settingStore.threadCount"
                placeholder="下载数"
                :min="2"
                :max="32"
            />
          </div>
        </div>
      </div>
    </div>

    <div class="other-setting set-wrap">
      <div class="o-title title">其他</div>
      <div class="set-items-wrap">
        <div class="set-item">
          <div class="set-label">当前版本</div>
          <div class="set-value">{{ version }}</div>
        </div>
        <div class="set-item">
          <div class="set-label">发布地址</div>
          <div class="set-value">https://github.com/Colzry/m3u8Downloader</div>
        </div>
      </div>
    </div>

  </main-wrapper>


</template>

<style scoped lang="less">
.set-wrap {
  width: 100%;
  padding: 10px;
  font-size: 0.9rem;
  border-radius: 5px;
  background-color: #fff;
  &:not(:last-child) {
    margin-bottom: 1rem;
  }
  .title {
    position: relative; /* 使伪元素的定位相对于父元素 */
    padding-left: 10px; /* 为标题内容增加左边距，避免和长方形重叠 */
    line-height: 1.1rem; /* 确保垂直居中 */
    &::before {
      content: ""; /* 创建一个空内容伪元素 */
      position: absolute; /* 绝对定位 */
      left: 0; /* 靠左对齐 */
      top: 0; /* 从顶部开始 */
      width: 3px; /* 宽度为 2px */
      height: 100%; /* 高度为父元素的 100% */
      background-color: #1ba059; /* 背景颜色 */
    }
  }

  .set-items-wrap {
    .set-item {
      margin-top: 20px;
      display: flex;
      align-items: center;
      .set-label {
        margin-left: 10px;
        flex: 3 1 0; /* 比例3，允许收缩，基准宽度0% */
        color: #1f1f1f;
        .select-dir {
          display: inline-block;
          padding: 8px;
          border: 1px solid #e2e2e2;
          cursor: pointer;
          border-radius: 5px;
          transition: all .4s;
          &:hover {
            color: #18a058;
            border-color: #18a058;
          }
        }
      }
      .set-value {
        flex: 7 1 0; /* 比例7，允许收缩，基准宽度0% */
      }
    }
  }
}
</style>