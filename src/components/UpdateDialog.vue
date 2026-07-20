<template>
  <md-dialog ref="dialogRef" @close="onClose">
    <div slot="headline">{{ t('settings.update.title') }}</div>

    <div slot="content" class="update-dialog__content">
      <!-- Checking -->
      <div v-if="store.checking" class="update-dialog__center">
        <md-circular-progress indeterminate />
        <p class="text-body-2 mt-2">{{ t('settings.update.checking') }}</p>
      </div>

      <!-- Error -->
      <p v-else-if="store.error" class="text-body-2 text-error">
        {{ t('settings.update.checkFailed', { error: store.error }) }}
      </p>

      <!-- Result -->
      <template v-else-if="store.info">
        <p class="text-body-2 mb-1">
          {{ t('settings.update.current') }} <b>v{{ store.info.current }}</b>
        </p>
        <p class="text-body-2 mb-3">
          {{ t('settings.update.latest') }} <b>v{{ store.info.latest }}</b>
        </p>

        <!-- Up to date -->
        <p v-if="!store.info.hasUpdate" class="text-body-2 text-success d-flex align-center">
          <MdiIcon :path="mdiCheckCircle" :size="18" class="mr-1" />
          {{ t('settings.update.upToDate') }}
        </p>

        <!-- Has update -->
        <template v-else>
          <div v-if="store.info.notes" class="update-dialog__notes text-body-2">
            {{ store.info.notes }}
          </div>

          <!-- Downloading progress -->
          <div v-if="store.downloading" class="mt-4">
            <md-linear-progress :value="store.progress.percent / 100" />
            <p class="text-body-2 text-medium-emphasis mt-1">
              {{ store.progress.percent }}% · {{ formatSpeed(store.progress.speed) }}
            </p>
          </div>

          <!-- Downloaded, ready to install -->
          <p v-else-if="store.downloadPath" class="mt-3 text-body-2 text-success d-flex align-center">
            <MdiIcon :path="mdiCheckCircle" :size="18" class="mr-1" />
            {{ t('settings.update.downloadComplete') }}
          </p>
        </template>
      </template>
    </div>

    <div slot="actions">
      <!-- No update / checking / error → a single dismiss button. -->
      <template v-if="!store.info?.hasUpdate">
        <md-text-button @click="closeDialog">
          {{ t('common.confirm') }}
        </md-text-button>
      </template>

      <!-- Has update: before download → 更新 + 取消 -->
      <template v-else-if="!store.downloadPath">
        <md-text-button @click="closeDialog">
          {{ t('common.cancel') }}
        </md-text-button>
        <md-filled-button :disabled="store.downloading" @click="store.download()">
          {{ store.downloading ? t('settings.update.downloading') : t('settings.update.download') }}
        </md-filled-button>
      </template>

      <!-- Downloaded: → 打开安装器 + 退出并安装 -->
      <template v-else>
        <md-outlined-button @click="store.install()">
          {{ t('settings.update.openInstaller') }}
        </md-outlined-button>
        <md-filled-button @click="store.quitAndInstall()">
          {{ t('settings.update.quitAndInstall') }}
        </md-filled-button>
      </template>
    </div>
  </md-dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { mdiCheckCircle } from '@mdi/js';
import { useI18n } from '@/i18n';
import { useUpdateStore } from '@/stores/update';
import MdiIcon from '@/components/MdiIcon.vue';

const { t } = useI18n();
const store = useUpdateStore();

type MdDialogEl = HTMLElement & { show: () => void; close: () => void };
const dialogRef = ref<MdDialogEl | null>(null);

function open() {
  dialogRef.value?.show();
}

function closeDialog() {
  dialogRef.value?.close();
}

function onClose() {
  // If a download finished but the user just closed the dialog, keep the file
  // path so reopening shows "install" again; only reset transient progress.
}

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec >= 1024 * 1024) return `${(bytesPerSec / 1024 / 1024).toFixed(1)} MB/s`;
  if (bytesPerSec >= 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${bytesPerSec} B/s`;
}

defineExpose({ open });
</script>

<style scoped>
.update-dialog__content {
  min-width: min(420px, 82vw);
  max-width: 82vw;
}

.update-dialog__center {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 16px 0;
}

.update-dialog__notes {
  max-height: 180px;
  overflow-y: auto;
  white-space: pre-wrap;
  padding: 10px 12px;
  border-radius: var(--md-sys-shape-corner-small);
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface-variant);
}
</style>
