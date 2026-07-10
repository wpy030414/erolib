<template>
  <div v-if="error" class="error-boundary pa-6">
    <h2 class="md3-title-medium text-error">{{ t('common.errorTitle') }}</h2>
    <pre class="error-message">{{ error.message }}</pre>
    <pre v-if="error.stack" class="md3-body-small error-stack">{{ error.stack }}</pre>
    <md-outlined-button class="mt-4" @click="dismiss">
      {{ t('common.dismiss') }}
    </md-outlined-button>
  </div>
  <div v-else :key="resetKey">
    <slot />
  </div>
</template>

<script setup lang="ts">
import { ref, onErrorCaptured } from 'vue';
import { useI18n } from '@/i18n';

const { t } = useI18n();

const error = ref<Error | null>(null);
const resetKey = ref(0);

function dismiss() {
  error.value = null;
  resetKey.value++;
}

onErrorCaptured((err, instance, info) => {
  error.value = err instanceof Error ? err : new Error(String(err));
  // eslint-disable-next-line no-console
  console.error('[ErrorBoundary] captured error:', err, instance, info);
  return false;
});
</script>

<style scoped>
.error-boundary {
  font-family: ui-monospace, SFMono-Regular, Menlo, 'Roboto Mono', monospace;
}

.error-message {
  white-space: pre-wrap;
  word-break: break-word;
  padding: 12px;
  border-radius: 8px;
  border: 1px solid var(--md-sys-color-error);
  background: color-mix(in srgb, var(--md-sys-color-error) 8%, transparent);
  color: var(--md-sys-color-on-surface);
}

.error-stack {
  white-space: pre-wrap;
  word-break: break-word;
  padding: 12px;
  border-radius: 8px;
  max-height: 320px;
  overflow: auto;
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 5%, transparent);
  color: color-mix(in srgb, var(--md-sys-color-on-surface) 60%, transparent);
  margin-top: 12px;
}
</style>
