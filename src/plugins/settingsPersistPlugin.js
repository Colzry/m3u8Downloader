import { invoke } from "@tauri-apps/api/core";
import { debounce } from "lodash";

/**
 * 将整个设置对象保存到 Tauri 的 settings.dat 文件中。
 * @param {Object} settingsObject - 整个设置 Store 的状态对象。
 */
async function saveSettings(settingsObject) {
    try {
        // 🚀 只需要调用一次命令，传入整个对象
        await invoke("save_settings", { settingsObject });
        console.log("✅ 所有设置已保存到 Tauri Store。");
    } catch (error) {
        console.error("❌ 保存所有设置失败:", error);
    }
}

// 确保在最后一次状态变化后 500ms 才会执行保存。
const debouncedSaveSettings = debounce(saveSettings, 500);

/**
 * Pinia 插件，用于持久化 useSettingStore 的设置到 Tauri Store。
 * @returns {Function} Pinia 插件函数
 */
export function createTauriSettingsPersistPlugin() {
    return ({ store }) => {
        // 只持久化ID为Setting的Store
        if (store.$id !== "Setting") {
            return;
        }

        // 订阅 Store 的状态变化
        store.$subscribe(
            (mutation, state) => {
                // 🚀 传递整个状态对象给防抖函数
                debouncedSaveSettings(state);
            },
            { detached: true },
        ); // detached: true 确保在组件卸载后继续监听
    };
}
