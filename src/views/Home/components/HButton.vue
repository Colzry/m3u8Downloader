<script setup>

// 定义 Props
defineProps({
  label: {
    type: String,
    default: "按钮", // 默认按钮文本
  },
  customClass: {
    type: String,
    default: "", // 允许传入自定义类名
  },
});

// 定义 Emits
const emit = defineEmits(["click"]);

// 点击事件处理
function handleClick(event) {
  emit("click", event);
}
</script>

<template>
  <button class="h-button" :class="customClass" @click="handleClick">
    <slot>{{ label }}</slot>
  </button>
</template>

<style scoped>
.h-button {
  appearance: none;
  border: none;
  outline: none;
  padding: 10px 20px;
  font-size: 14px;
  font-weight: bold;
  color: #fff;
  background: linear-gradient(135deg,#00c9ff,#92fe9d);
  border-radius: 10px;
  cursor: pointer;
}
</style>