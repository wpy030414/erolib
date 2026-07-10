<template>
  <div class="pa-6">
    <h2 class="text-h5 mb-6">{{ t('nav.ehentai') }}</h2>

    <!-- Login status section -->
    <div class="md3-card md3-card--outlined mb-6">
      <div class="md3-card__header">
        <span class="md3-card__header-avatar">
          <MdiIcon :path="mdiAlphaE" :size="26" />
        </span>
        <div class="md3-card__header-titles">
          <span class="md3-card__title">{{ t('eh.login.tab') }}</span>
          <span class="md3-card__subtitle">
            <span v-if="loggedIn" class="text-success">
              {{ t('eh.login.loggedIn') }}
            </span>
            <span v-else-if="loggingIn" class="d-flex align-center">
              <md-circular-progress indeterminate class="eh-spin" />
              {{ t('eh.login.loggingIn') }}
            </span>
            <span v-else class="text-medium-emphasis">
              {{ t('eh.login.notLoggedIn') }}
            </span>
          </span>
        </div>
        <span class="md3-card__header-action">
          <md-outlined-button
            :disabled="loggingIn"
            @click="startLogin"
          >
            <MdiIcon slot="icon" :path="mdiLogin" :size="18" />
            {{ t('eh.login.openBrowser') }}
          </md-outlined-button>
        </span>
      </div>

      <div class="md3-card__content text-body-2 text-medium-emphasis">
        {{ t('eh.login.hint') }}
      </div>
    </div>

    <!-- Download section -->
    <div class="md3-card md3-card--outlined">
      <div class="md3-card__header">
        <span class="md3-card__header-avatar">
          <MdiIcon :path="mdiDownload" :size="24" />
        </span>
        <div class="md3-card__header-titles">
          <span class="md3-card__title">{{ t('eh.download.tab') }}</span>
        </div>
      </div>

      <div class="md3-card__content">
        <div class="d-flex align-start gap-4 flex-wrap mb-4">
          <md-outlined-text-field
            :value="galleryUrl"
            :label="t('eh.download.url.label')"
            :placeholder="t('eh.download.url.placeholder')"
            :disabled="running"
            style="flex: 1; min-width: 240px"
            @input="galleryUrl = $event.target.value"
          />

          <md-filled-button
            :disabled="!canStart"
            @click="start"
          >
            <MdiIcon slot="icon" :path="mdiDownload" :size="18" />
            {{ running ? t('eh.download.running') : t('eh.download.start') }}
          </md-filled-button>

          <md-outlined-button
            v-if="running"
            @click="cancel"
          >
            <MdiIcon slot="icon" :path="mdiCancel" :size="18" />
            {{ t('eh.download.cancel') }}
          </md-outlined-button>
        </div>

        <div v-if="running" class="mb-4">
          <p class="text-body-2 text-medium-emphasis mb-2">
            {{ current ? `${phase}: ${current.index}/${total} — ${current.title}` : `${phase}: ${message}` }}
          </p>
          <md-linear-progress :value="progressValue" />
        </div>

        <p v-if="result" class="text-body-2 mb-4">
          <MdiIcon :path="mdiAlphaE" :size="18" class="mr-2" />
          {{ t('eh.result', result) }}
        </p>

        <div v-if="logs.length > 0" class="rounded bg-surface-container">
          <md-list class="eh-logs">
            <md-list-item
              v-for="(line, i) in logs"
              :key="i"
            >
              <span slot="start" class="eh-log-icon">›</span>
              <span class="font-monospace text-body-2">{{ line }}</span>
            </md-list-item>
          </md-list>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import {
  mdiAlphaE,
  mdiCancel,
  mdiDownload,
  mdiLogin,
} from '@mdi/js';
import { listen } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import MdiIcon from '@/components/MdiIcon.vue';

const { t } = useI18n();

const loggedIn = ref(false);
const loggingIn = ref(false);
const galleryUrl = ref('');
const running = ref(false);
const phase = ref('idle');
const message = ref('');
const total = ref(0);
const current = ref<{ index: number; title: string } | null>(null);
const logs = ref<string[]>([]);
const result = ref<{ downloaded: number; skipped: number; failed: number } | null>(null);

let unlistenLogin: (() => void) | null = null;
let unlistenProgress: (() => void) | null = null;

function pushLog(line: string) {
  logs.value = [line, ...logs.value].slice(0, 200);
}

onMounted(async () => {
  const cookie = await api.getEHentaiLogin();
  if (cookie) {
    loggedIn.value = true;
  }

  unlistenLogin = await listen<{ cookie: string }>('ehentai://login', (evt) => {
    if (evt.payload.cookie) {
      loggedIn.value = true;
      loggingIn.value = false;
    }
  });

  unlistenProgress = await listen<ProgressEvent>('ehentai://progress', (evt) => {
    const p = evt.payload as any;
    if ('phase' in p) {
      phase.value = p.phase;
      message.value = p.message;
      pushLog(`[${p.phase}] ${p.message}`);
    } else if ('total_bookmarks' in p) {
      total.value = p.total_bookmarks;
    } else if ('index' in p) {
      current.value = { index: p.index, title: p.title };
    } else if ('pages' in p) {
      pushLog(`✓ ${p.title} (${p.pages} pages)`);
    } else if ('reason' in p) {
      pushLog(`⊘ ${p.title} — ${p.reason}`);
    } else if ('error' in p) {
      pushLog(`✗ ${p.title} — ${p.error}`);
    } else if ('downloaded' in p) {
      result.value = p;
      running.value = false;
      current.value = null;
    }
  });
});

onUnmounted(() => {
  unlistenLogin?.();
  unlistenProgress?.();
});

async function startLogin() {
  loggingIn.value = true;
  try {
    await api.openEHentaiLoginWindow();
  } catch (e) {
    loggingIn.value = false;
    pushLog(t('common.error', { message: `opening login window: ${e}` }));
  }
}

async function start() {
  result.value = null;
  logs.value = [];
  running.value = true;
  try {
    await api.downloadEHentaiGallery(galleryUrl.value.trim());
  } catch (e) {
    pushLog(t('common.error', { message: String(e) }));
    running.value = false;
  }
}

async function cancel() {
  await api.cancelEHentaiDownload();
  pushLog(t('common.cancelledByUser'));
}

const progressValue = computed(() =>
  total.value > 0 && current.value
    ? Math.min(100, (current.value.index / total.value) * 100)
    : 0,
);

const canStart = computed(
  () => !running.value && loggedIn.value && galleryUrl.value.trim().length > 0,
);

type ProgressEvent =
  | { phase: string; message: string }
  | { total_bookmarks: number }
  | { index: number; total: number; illust_id: string; title: string }
  | { illust_id: string; title: string; pages: number }
  | { illust_id: string; title: string; reason: string }
  | { illust_id: string; title: string; error: string }
  | { downloaded: number; skipped: number; failed: number };
</script>

<style scoped>
.md3-card {
  display: flex;
  flex-direction: column;
  background: var(--md-sys-color-surface);
  border-radius: var(--md-sys-shape-corner-medium);
}

.md3-card--outlined {
  border: 1px solid var(--md-sys-color-outline-variant);
}

.md3-card__header {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px;
}

.md3-card__header-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
  flex-shrink: 0;
}

.md3-card__header-titles {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}

.md3-card__title {
  font: 500 var(--md-sys-typescale-title-large-size) / var(--md-sys-typescale-title-large-line-height) var(--md-sys-typescale-font);
  letter-spacing: var(--md-sys-typescale-title-large-tracking);
  color: var(--md-sys-color-on-surface);
}

.md3-card__subtitle {
  font: 400 var(--md-sys-typescale-body-medium-size) / var(--md-sys-typescale-body-medium-line-height) var(--md-sys-typescale-font);
  letter-spacing: var(--md-sys-typescale-body-medium-tracking);
  color: var(--md-sys-color-on-surface-variant);
}

.md3-card__header-action {
  display: inline-flex;
  flex-shrink: 0;
  margin-left: auto;
}

.md3-card__content {
  padding: 0 16px 16px;
}

.eh-spin {
  --md-circular-progress-size: 18px;
  --md-circular-progress-active-indicator-width: 6;
  margin-right: 8px;
}

.eh-logs {
  max-height: 320px;
  overflow-y: auto;
}

.eh-log-icon {
  color: var(--md-sys-color-on-surface-variant);
  margin-right: 8px;
  font-size: 12px;
}
</style>
