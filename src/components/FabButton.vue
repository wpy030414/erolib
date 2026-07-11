<template>
  <button
    class="fab-button"
    :aria-label="ariaLabel"
    :disabled="disabled"
    @click="$emit('click')"
  >
    <MdiIcon :path="icon" :size="24" />
  </button>
</template>

<script setup lang="ts">
import MdiIcon from '@/components/MdiIcon.vue';

/**
 * Floating action button (bottom-right). Replaces the per-view `fab-refresh`
 * (Pixiv / EHentai) and `fab-clear` (Tasks) copies, which were byte-identical
 * apart from the class name. */
defineProps<{
  /** mdi icon path. */
  icon: string;
  ariaLabel?: string;
  disabled?: boolean;
}>();

defineEmits<{ (e: 'click'): void }>();
</script>

<style scoped>
.fab-button {
  position: fixed;
  right: 24px;
  bottom: 24px;
  z-index: 50;
  width: 56px;
  height: 56px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
  box-shadow: var(--md-sys-elevation-level3);
  cursor: pointer;
  transition:
    box-shadow 0.15s ease,
    transform 0.15s ease;
}

.fab-button:hover:not(:disabled) {
  box-shadow: var(--md-sys-elevation-level4);
  transform: scale(1.05);
}

.fab-button:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
