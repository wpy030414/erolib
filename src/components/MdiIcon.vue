<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    aria-hidden="true"
    focusable="false"
    :fill="color ?? 'currentColor'"
  >
    <path :d="path" />
  </svg>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    path: string;
    size?: number;
    color?: string;
  }>(),
  {
    size: 24,
  },
);
</script>
