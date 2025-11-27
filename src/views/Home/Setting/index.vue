<script async setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import { useSettingStore } from "@/store/SettingStore.js";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { appLogDir } from "@tauri-apps/api/path";
import { HelpCircleOutline } from "@vicons/ionicons5";
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const version = import.meta.env.VITE_APP_VERSION;
const settingStore = useSettingStore();

const selectFolder = async () => {
    const selectDirectory = await open({
        directory: true,
        multiple: false,
        title: "选择下载目录",
    });
    if (selectDirectory) {
        settingStore.downloadPath = selectDirectory;
    }
};

// 日志级别选项（与 Rust 的 LevelFilter 对应）
const LOG_LEVEL_OPTIONS = [
    { label: "Trace", value: "Trace" },
    { label: "Debug", value: "Debug" },
    { label: "Info", value: "Info" },
    { label: "Warn", value: "Warn" },
    { label: "Error", value: "Error" },
    { label: "Off", value: "Off" },
];

const openAppLogDirectory = async () => {
    try {
        const logDirPath = await appLogDir();
        await openPath(logDirPath);
    } catch (e) {
        console.error("无法打开日志目录:", e);
    }
};

const updateModalVisible = ref(false); // 控制模态框显示
const updateProgress = ref(0); // 下载进度百分比
const updateMessage = ref(""); // 下载状态文字
const updateModalStatus = ref("checking"); // "checking" | "downloading" | "success" | "failed" | "latest"

onMounted(() => {
    // 监听 Rust 后端发过来的更新状态
    listen("update_status", (event) => {
        const { status, progress, message } = event.payload;

        updateProgress.value = progress;
        updateMessage.value = message;

        if (status === "downloading") {
            updateModalStatus.value = "downloading";
        } else if (status === "latest") {
            updateModalStatus.value = "latest";
            setTimeout(() => {
                updateModalVisible.value = false;
            }, 3000);
        } else if (status === "installed") {
            updateModalStatus.value = "success";
            setTimeout(() => {
                updateModalVisible.value = false;
            }, 3000);
        } else if (status === "failed") {
            updateModalStatus.value = "failed";
            setTimeout(() => {
                updateModalVisible.value = false;
            }, 3000);
        }
    });
});

// 点击按钮触发更新
const onCheckUpdateClick = async () => {
    updateModalVisible.value = true;
    updateProgress.value = 0;
    updateMessage.value = "正在检查更新...";
    updateModalStatus.value = "checking";
    try {
        await invoke("check_update");
    } catch (e) {
        updateMessage.value = "检查更新失败：" + e;
        updateProgress.value = 0;
        updateModalStatus.value = "failed";
        setTimeout(() => {
            updateModalVisible.value = false;
        }, 3000);
    }
};
</script>

<template>
    <page-header title="软件设置" />

    <main-wrapper>
        <div class="base-setting set-wrap">
            <div class="b-title title">基本设置</div>
            <div class="set-items-wrap">
                <div class="set-item">
                    <div class="set-label">
                        <div class="select-dir" @click="selectFolder">
                            选择下载文件夹
                        </div>
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
                        <n-switch
                            size="small"
                            v-model:value="settingStore.isDeleteDownloadFile"
                        />
                    </div>
                </div>

                <div class="set-item">
                    <div class="set-label">
                        <div>关闭主窗口</div>
                    </div>
                    <div class="set-value">
                        <n-radio-group
                            v-model:value="settingStore.minimizeOnClose"
                            name="closeTheWindow"
                        >
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
                            :max="settingStore.physicalCores * 2"
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
                            placeholder="线程数"
                            :min="1"
                            :max="settingStore.logicalCores * 8"
                        />
                    </div>
                </div>
            </div>
        </div>

        <div class="version set-wrap">
            <div class="o-title title">版本</div>
            <div class="set-items-wrap">
                <div class="set-item">
                    <div class="set-label">当前版本</div>
                    <div class="set-value">
                        <div class="version">{{ version }}</div>
                        <div class="check-update" style="margin-left: 10px">
                            <n-button
                                ghost
                                size="small"
                                @click="onCheckUpdateClick"
                            >
                                检查更新
                            </n-button>
                        </div>
                        <n-tooltip trigger="hover">
                            <template #trigger>
                                <n-icon
                                    size="1.2rem"
                                    style="cursor: pointer; margin-left: 5px"
                                >
                                    <HelpCircleOutline />
                                </n-icon>
                            </template>
                            <span>若更新失败可点击下面发布地址去下载安装</span>
                        </n-tooltip>
                    </div>
                </div>
                <div class="set-item">
                    <div class="set-label">发布地址</div>
                    <div
                        class="set-value url"
                        @click="
                            openUrl('https://github.com/Colzry/m3u8Downloader/releases')
                        "
                    >
                        https://github.com/Colzry/m3u8Downloader/releases
                    </div>
                </div>
            </div>
        </div>

        <div class="other-setting set-wrap">
            <div class="o-title title">其他</div>
            <div class="set-items-wrap">
                <div class="set-item">
                    <div class="set-label">
                        <div class="select-dir" @click="openAppLogDirectory">
                            打开日志目录
                        </div>
                    </div>
                    <div class="set-value">
                        <div style="margin-right: 5px; font: 1rem weight">
                            日志级别
                        </div>
                        <n-select
                            size="small"
                            style="max-width: 100px; margin-left: 5px"
                            v-model:value="settingStore.logLevel"
                            :options="LOG_LEVEL_OPTIONS"
                            placeholder="日志级别"
                        />
                        <n-tooltip trigger="hover">
                            <template #trigger>
                                <n-icon
                                    size="1.2rem"
                                    style="cursor: pointer; margin-left: 5px"
                                >
                                    <HelpCircleOutline />
                                </n-icon>
                            </template>
                            <span>该设置需要重启程序后生效</span>
                        </n-tooltip>
                    </div>
                </div>
            </div>
        </div>
    </main-wrapper>

    <n-modal
        v-model:show="updateModalVisible"
        title="检查更新"
        :mask-closable="false"
        :show-close="true"
        :show-footer="false"
        :style="{
            width: '400px',
            borderRadius: '8px',
        }"
        :mask-style="{ backgroundColor: 'rgba(0,0,0,0.35)' }"
    >
        <div
            style="
                margin-top: 10px;
                display: flex;
                padding: 2rem;
                line-height: 1.5rem;
                flex-direction: column;
                align-items: center;
                text-align: center;
                background-color: #fff;
                border-radius: 8px;
            "
        >
            <p
                :style="{
                    color:
                        updateModalStatus === 'failed'
                            ? 'red'
                            : updateModalStatus === 'success'
                              ? '#1ba059'
                              : updateModalStatus === 'latest'
                                ? '#1ba059'
                                : '#333',
                    fontWeight: 500,
                }"
            >
                {{ updateMessage }}
            </p>

            <n-progress
                v-if="
                    updateModalStatus === 'downloading' ||
                    updateModalStatus === 'checking'
                "
                :value="updateProgress"
                :show-value="true"
                style="
                    width: 100%;
                    height: 18px;
                    border-radius: 9px;
                    margin-top: 10px;
                "
                :status="updateModalStatus === 'failed' ? 'error' : 'active'"
            />
        </div>
    </n-modal>
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
                    transition: all 0.4s;
                    &:hover {
                        color: #18a058;
                        border-color: #18a058;
                    }
                }
            }
            .set-value {
                display: flex;
                align-items: center;
                flex: 7 1 0; /* 比例7，允许收缩，基准宽度0% */
            }
            .url {
                cursor: pointer;
                transition: all 0.4s;
                &:hover {
                    color: #18a058;
                    text-decoration: underline;
                }
            }
        }
    }
}
</style>
