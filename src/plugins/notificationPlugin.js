import { useNotification, useMessage } from 'naive-ui';

/**
 * 创建 Pinia 插件，用于在所有 Store 中注入 Naive UI 的服务实例。
 * * @returns {Function} Pinia Plugin Function
 */
export function createNotificationPlugin() {
    return ({ store }) => {
        // 确保在 Vue 上下文环境（即 <n-provider> 内部）中调用 use... 钩子
                
        // 1. 注入通知服务
        const notification = useNotification(); 
        store.$notify = notification; // 在 Store 中可通过 this.$notify 访问

        // 2. 注入消息服务 (轻量提醒)
        const message = useMessage();
        store.$message = message; // 在 Store 中可通过 this.$message 访问
    }
}