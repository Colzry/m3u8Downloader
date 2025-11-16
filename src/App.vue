<script setup>
// 关闭编译后的应用右键菜单
if (import.meta.env.PROD) {
    document.addEventListener("contextmenu", (e) => e.preventDefault());
}
</script>

<template>
    <n-message-provider>
        <n-notification-provider>
            <n-modal-provider>
                <RouterView />
            </n-modal-provider>
        </n-notification-provider>
    </n-message-provider>
</template>
<style scoped></style>
