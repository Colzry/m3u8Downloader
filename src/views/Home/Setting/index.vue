<script setup>
import PageHeader from "@/views/Home/components/PageHeader.vue";
import MainWrapper from "@/views/Home/components/MainWrapper.vue";
import { useSettingStore } from "@/store/SettingStore.js";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { appLogDir } from "@tauri-apps/api/path";
import { HelpCircleOutline } from "@vicons/ionicons5";
import { ref } from "vue";

// 引入官方的 updater 和 process 插件 API
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

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

const updateModalVisible = ref(false);
const updateProgress = ref(0);
const updateMessage = ref("");
// 增加 "confirm" 状态
const updateModalStatus = ref("checking"); // "checking" | "confirm" | "downloading" | "success" | "failed" | "latest"

// 用于保存检查到的更新对象
const currentUpdate = ref(null);

// 点击按钮触发检查更新
const onCheckUpdateClick = async () => {
    updateModalVisible.value = true;
    updateProgress.value = 0;
    updateMessage.value = "正在检查更新...";
    updateModalStatus.value = "checking";
    currentUpdate.value = null;

    try {
        // 使用 Tauri 官方前端 API 检查更新
        const update = await check();

        if (update) {
            // 发现更新，进入等待用户确认状态
            currentUpdate.value = update;
            updateMessage.value = `发现新版本 v${update.version}，是否立即更新？`;
            updateModalStatus.value = "confirm";
        } else {
            // 没有更新
            updateMessage.value = "当前已是最新版本";
            updateModalStatus.value = "latest";
            setTimeout(() => {
                updateModalVisible.value = false;
            }, 3000);
        }
    } catch (e) {
        updateMessage.value = "检查更新失败：" + e;
        updateModalStatus.value = "failed";
        setTimeout(() => {
            updateModalVisible.value = false;
        }, 3000);
    }
};

// 用户点击确认更新，开始下载
const confirmUpdate = async () => {
    if (!currentUpdate.value) return;

    updateModalStatus.value = "downloading";
    updateMessage.value = "正在下载更新...";
    updateProgress.value = 0;

    let downloaded = 0;
    let contentLength = 0;

    try {
        // 执行下载并安装，监听进度
        await currentUpdate.value.downloadAndInstall((event) => {
            switch (event.event) {
                case "Started":
                    contentLength = event.data.contentLength;
                    break;
                case "Progress":
                    downloaded += event.data.chunkLength;
                    if (contentLength > 0) {
                        updateProgress.value = Math.round(
                            (downloaded / contentLength) * 100,
                        );
                    }
                    break;
                case "Finished":
                    updateProgress.value = 100;
                    break;
            }
        });

        // 安装完成
        updateMessage.value = "更新已准备就绪，正在重启...";
        updateModalStatus.value = "success";

        setTimeout(async () => {
            await relaunch(); // 重启应用
        }, 1500);
    } catch (e) {
        updateMessage.value = "更新下载失败：" + e;
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
                    <div class="set-label">单个下载线程数</div>
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
                            openUrl(
                                'https://github.com/Colzry/m3u8-downloader/releases',
                            )
                        "
                    >
                        https://github.com/Colzry/m3u8-downloader/releases
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

            <div
                v-if="updateModalStatus === 'confirm'"
                style="margin-top: 20px; display: flex; gap: 15px"
            >
                <n-button @click="updateModalVisible = false">取消</n-button>
                <n-button type="primary" @click="confirmUpdate"
                    >立即更新</n-button
                >
            </div>

            <n-progress
                v-if="updateModalStatus === 'downloading'"
                :percentage="updateProgress"
                :show-indicator="false"
                type="line"
                processing
                style="
                    width: 100%;
                    height: 18px;
                    border-radius: 9px;
                    margin-top: 10px;
                "
                :status="updateModalStatus === 'failed' ? 'error' : 'success'"
            />
        </div>
    </n-modal>
</template>

<style scoped lang="less">
/* 原有样式保持不变 */
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
        position: relative;
        padding-left: 10px;
        line-height: 1.1rem;
        &::before {
            content: "";
            position: absolute;
            left: 0;
            top: 0;
            width: 3px;
            height: 100%;
            background-color: #1ba059;
        }
    }

    .set-items-wrap {
        .set-item {
            margin-top: 20px;
            display: flex;
            align-items: center;
            .set-label {
                margin-left: 10px;
                flex: 3 1 0;
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
                flex: 7 1 0;
            }
            .url {
                cursor: pointer;
                transition: all 0.2s;
                &:hover {
                    color: #18a058;
                    text-decoration: underline;
                }
            }
        }
    }
}
</style>
