<script setup>
</script>

<template>
  <main class="content">
    <slot></slot> <!-- 插槽用于传递子组件内容 -->
  </main>
</template>

<style scoped>
.content {
  margin-top: 4rem; /* 让 main 留出足够空间，避免被 PageHeader 遮挡 */
  height: calc(100vh - 6rem); /* main 区域高度，占满剩余空间 */
  overflow-y: auto; /* 启用垂直滚动 */
  overflow-x: hidden;
}
</style>