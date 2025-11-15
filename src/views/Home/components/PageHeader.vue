<script setup>
import { useUIStore } from "@/store/UIStore";
const UIStore = useUIStore();

defineProps({
    title: {
        type: String,
        required: true,
    },
});
</script>

<template>
    <div
        :class="['common-header', { 'no-extra': !$slots.extra }]"
        :style="{
            width: UIStore.collapsed
                ? 'calc(100% - 64px - 1.6rem)'
                : 'calc(100% - 200px - 1.6rem)',
        }"
    >
        <div class="title">{{ title }}</div>
        <div class="extra" v-if="$slots.extra">
            <slot name="extra"></slot>
        </div>
    </div>
</template>

<style scoped>
.common-header {
    margin-top: 1rem;
    position: fixed; /* 固定定位 */
    top: 0;
    z-index: 1000;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px;
    background-color: #fff;
    border-radius: 10px;
    height: 3.6rem;
    box-sizing: border-box; /* 包含 padding 在内 */
}

/* 当没有 extra 插槽时移除 padding */
.common-header.no-extra {
    height: 3rem;
}

.title {
    font-size: 1rem;
    font-weight: bold;
}

.extra {
    display: flex;
    align-items: center;
    gap: 10px; /* 按钮间距 */
}
</style>
