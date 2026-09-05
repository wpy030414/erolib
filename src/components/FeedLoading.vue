<template>
  <div class="feed-loading text-center text-medium-emphasis">
    <md-circular-progress indeterminate />
    <span><slot>{{ t('nc.browse.loadingMore') }}</slot></span>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from '@/i18n';

const { t } = useI18n();
</script>

<style scoped>
.feed-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px 0;
}
</style>
