<template>
  <div class="pa-6">
    <h2 class="text-h5 mb-6">{{ t('nav.pixiv') }}</h2>

    <!-- Login status section -->
    <div class="md3-card md3-card--outlined mb-6">
      <div class="md3-card__header">
        <span class="md3-card__header-avatar">
          <MdiIcon :path="mdiAlphaP" :size="26" />
        </span>
        <div class="md3-card__header-titles">
          <span class="md3-card__title">Pixiv</span>
          <span class="md3-card__subtitle">
            <span v-if="login" class="d-flex align-center">
              {{ t('pixiv.login.loggedIn') }}
              <span v-if="!login.user_id" class="text-warning ml-1">
                ({{ t('pixiv.login.noUserId') }})
              </span>
            </span>
            <span v-else-if="loggingIn" class="d-flex align-center">
              <md-circular-progress indeterminate class="pixiv-spin" />
              {{ t('pixiv.login.loggingIn') }}
            </span>
            <span v-else class="text-medium-emphasis">
              {{ t('pixiv.login.notLoggedIn') }}
            </span>
          </span>
        </div>
        <span class="md3-card__header-action">
          <md-outlined-button
            :disabled="loggingIn"
            @click="startLogin"
          >
            <MdiIcon slot="icon" :path="login ? mdiRefresh : mdiLogin" :size="18" />
            {{ t('pixiv.login') }}
          </md-outlined-button>
        </span>
      </div>

      <div class="md3-card__content">
        <p v-if="login?.user_id" class="text-body-2 text-medium-emphasis">
          {{ t('pixiv.login.userId') }}: {{ login.user_id }}
        </p>
        <p v-else class="text-body-2 text-medium-emphasis">
          {{ t('pixiv.login.notLoggedIn') }}
        </p>
      </div>
    </div>

    <!-- Download section -->
    <div class="md3-card md3-card--outlined">
      <div class="md3-card__header">
        <span class="md3-card__header-avatar">
          <MdiIcon :path="mdiDownload" :size="24" />
        </span>
        <div class="md3-card__header-titles">
          <span class="md3-card__title">{{ t('pixiv.download.tab') }}</span>
        </div>
      </div>

      <div class="md3-card__content">
        <p class="text-body-2 text-medium-emphasis mb-4">
          {{ t('pixiv.smartSkip') }}
        </p>

        <md-tabs ref="tabsRef" class="mb-4">
          <md-primary-tab value="bookmark">{{ t('pixiv.tab.bookmark') }}</md-primary-tab>
          <md-primary-tab value="following">{{ t('pixiv.tab.following') }}</md-primary-tab>
        </md-tabs>

        <div v-if="tab === 'bookmark'" class="mb-4">
          <p class="text-body-2 text-medium-emphasis">
            {{ t('pixiv.limit.hint') }}
          </p>
        </div>

        <div v-else class="mb-4">
          <div class="d-flex align-start gap-4 flex-wrap mb-4">
            <md-outlined-select
              ref="followingSelectRef"
              :label="t('pixiv.followings')"
              style="min-width: 240px; flex: 1"
              :disabled="running || followingsLoading"
            >
              <md-select-option
                v-for="opt in followingOptions"
                :key="opt.value"
                :value="opt.value"
                :selected="selectedFollowing === opt.value"
              >
                {{ opt.label }}
              </md-select-option>
            </md-outlined-select>

            <md-filled-button
              :disabled="running || followingsLoading"
              @click="refreshFollowings(true)"
            >
              <MdiIcon slot="icon" :path="mdiRefresh" :size="18" />
              {{ t('pixiv.refreshFollowings') }}
            </md-filled-button>
          </div>

          <p v-if="followings.length === 0 && !followingsLoading && login" class="text-body-2 text-medium-emphasis">
            {{ t('pixiv.followings.empty') }}
          </p>
        </div>

        <div class="d-flex align-start gap-4 flex-wrap mb-4">
          <md-outlined-text-field
            :value="String(limit)"
            type="number"
            :label="t('pixiv.limit')"
            :disabled="running"
            style="width: 140px"
            @input="limit = Number(($event.target as HTMLInputElement).value)"
          />

          <md-filled-button
            :disabled="!canStart"
            @click="start"
          >
            <MdiIcon slot="icon" :path="mdiDownload" :size="18" />
            {{ running ? t('pixiv.running') : t('pixiv.start') }}
          </md-filled-button>

          <md-outlined-button
            v-if="running"
            @click="cancel"
          >
            <MdiIcon slot="icon" :path="mdiCancel" :size="18" />
            {{ t('pixiv.cancel') }}
          </md-outlined-button>
        </div>

        <div v-if="running" class="mb-4">
          <p class="text-body-2 text-medium-emphasis mb-2">
            {{ current ? `${phase}: ${current.index}/${total} — ${current.title}` : `${phase}: ${message}` }}
          </p>
          <md-linear-progress :value="progressValue" />
        </div>

        <p v-if="result" class="text-body-1 text-success mb-4">
          <MdiIcon :path="mdiAccount" :size="18" class="mr-2" />
          {{ t('pixiv.result', result) }}
        </p>

        <div v-if="logs.length > 0" class="rounded bg-surface-container">
          <md-list class="pixiv-logs">
            <md-list-item
              v-for="(line, i) in logs"
              :key="i"
            >
              <span slot="start" class="pixiv-log-icon">›</span>
              <span class="font-monospace text-body-2">{{ line }}</span>
            </md-list-item>
          </md-list>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import {
  mdiAccount,
  mdiAlphaP,
  mdiCancel,
  mdiDownload,
  mdiLogin,
  mdiRefresh,
} from '@mdi/js';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import MdiIcon from '@/components/MdiIcon.vue';

const { t } = useI18n();

interface PixivLogin {
  cookie: string;
  user_id: string;
}

interface FollowingUser {
  userId: string;
  userName: string;
  profileImageUrl: string;
}

type ProgressEvent =
  | { phase: string; message: string }
  | { totalBookmarks: number }
  | { index: number; total: number; illustId: string; title: string }
  | { illustId: string; title: string; pages: number }
  | { illustId: string; title: string; reason: string }
  | { illustId: string; title: string; error: string }
  | { downloaded: number; skipped: number; failed: number };

const DEFAULT_LIMIT = 100;

const login = ref<PixivLogin | null>(null);
const loggingIn = ref(false);
const tab = ref<'bookmark' | 'following'>('bookmark');
const limit = ref<number>(DEFAULT_LIMIT);
const running = ref(false);
const phase = ref('idle');
const message = ref('');
const total = ref(0);
const current = ref<{ index: number; title: string } | null>(null);
const logs = ref<string[]>([]);
const result = ref<{ downloaded: number; skipped: number; failed: number } | null>(null);

const followings = ref<FollowingUser[]>(readCachedFollowings());
const followingsLoading = ref(false);
const selectedFollowing = ref<string | null>(null);

const FOLLOWINGS_CACHE_KEY = 'erolib.pixiv.followings';
const FOLLOWINGS_CACHE_AT_KEY = 'erolib.pixiv.followingsAt';
const FOLLOWINGS_CACHE_TTL_MS = 1000 * 60 * 60 * 24; // 24h

function readCachedFollowings(): FollowingUser[] {
  try {
    const at = Number(window.localStorage.getItem(FOLLOWINGS_CACHE_AT_KEY));
    if (!at || Date.now() - at > FOLLOWINGS_CACHE_TTL_MS) return [];
    const raw = window.localStorage.getItem(FOLLOWINGS_CACHE_KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : [];
    if (Array.isArray(parsed)) return parsed as FollowingUser[];
  } catch {
    // ignore
  }
  return [];
}

function saveCachedFollowings(list: FollowingUser[]) {
  try {
    window.localStorage.setItem(FOLLOWINGS_CACHE_KEY, JSON.stringify(list));
    window.localStorage.setItem(FOLLOWINGS_CACHE_AT_KEY, String(Date.now()));
  } catch {
    // ignore
  }
}

function clearCachedFollowings() {
  try {
    window.localStorage.removeItem(FOLLOWINGS_CACHE_KEY);
    window.localStorage.removeItem(FOLLOWINGS_CACHE_AT_KEY);
  } catch {
    // ignore
  }
}

let unlistenLogin: UnlistenFn | undefined;
let unlistenProgress: UnlistenFn | undefined;

function pushLog(line: string) {
  logs.value = [line, ...logs.value].slice(0, 200);
}

onMounted(async () => {
  try {
    const l = await api.getPixivLogin();
    if (l) login.value = l;
  } catch {
    // ignore
  }

  unlistenLogin = await listen<{ user_id: string; cookie: string }>('pixiv://login', (evt) => {
    login.value = { user_id: evt.payload.user_id, cookie: evt.payload.cookie };
    loggingIn.value = false;
  });

  unlistenProgress = await listen<ProgressEvent>('pixiv://progress', (evt) => {
    const payload = evt.payload as Record<string, unknown>;
    if ('phase' in payload) {
      phase.value = String(payload.phase);
      message.value = String(payload.message);
      pushLog(`[${payload.phase}] ${payload.message}`);
    } else if ('totalBookmarks' in payload) {
      total.value = Number(payload.totalBookmarks);
    } else if ('index' in payload) {
      current.value = { index: Number(payload.index), title: String(payload.title) };
    } else if ('pages' in payload) {
      pushLog(`✓ ${payload.title} (${payload.pages} pages)`);
    } else if ('reason' in payload) {
      pushLog(`⊘ ${payload.title} — ${payload.reason}`);
    } else if ('error' in payload) {
      pushLog(`✗ ${payload.title} — ${payload.error}`);
    } else if ('downloaded' in payload) {
      result.value = {
        downloaded: Number(payload.downloaded),
        skipped: Number(payload.skipped),
        failed: Number(payload.failed),
      };
      running.value = false;
      current.value = null;
    }
  });
});

onUnmounted(() => {
  unlistenLogin?.();
  unlistenProgress?.();
});

async function refreshFollowings(force = false) {
  if (!login.value) return;
  if (
    !force &&
    followings.value.length > 0 &&
    Date.now() - Number(window.localStorage.getItem(FOLLOWINGS_CACHE_AT_KEY) ?? 0) < FOLLOWINGS_CACHE_TTL_MS
  ) {
    return;
  }
  followingsLoading.value = true;
  try {
    const list = await api.fetchPixivFollowings(500);
    followings.value = list;
    saveCachedFollowings(list);
  } catch (e) {
    pushLog(t('common.error', { message: `loading followings: ${e}` }));
  } finally {
    followingsLoading.value = false;
  }
}

watch([login, tab], () => {
  if (login.value && tab.value === 'following' && !followingsLoading.value) {
    refreshFollowings(false);
  }
});

async function startLogin() {
  loggingIn.value = true;
  try {
    await api.openPixivLoginWindow();
  } catch (e) {
    loggingIn.value = false;
    pushLog(t('common.error', { message: `opening login window: ${e}` }));
  }
}

function resolveTargetUserId(): string | null {
  if (tab.value === 'following') {
    return selectedFollowing.value;
  }
  return login.value?.user_id ?? null;
}

async function start() {
  result.value = null;
  logs.value = [];
  running.value = true;
  try {
    const target = resolveTargetUserId();
    if (!target) {
      pushLog(t('common.noTargetUser'));
      running.value = false;
      return;
    }
    if (tab.value === 'following') {
      await api.downloadPixivUserWorks(target, limit.value);
    } else {
      if (!login.value) {
        pushLog(t('common.notLoggedIn'));
        running.value = false;
        return;
      }
      await api.downloadPixivBookmarks(login.value.cookie, login.value.user_id, limit.value);
    }
  } catch (e) {
    pushLog(t('common.error', { message: String(e) }));
    running.value = false;
  }
}

async function cancel() {
  await api.cancelPixivDownload();
  pushLog(t('common.cancelledByUser'));
}

const progressValue = computed(() =>
  total.value > 0 && current.value ? Math.min(100, (current.value.index / total.value) * 100) : 0,
);

const canStart = computed(
  () => !running.value && (tab.value === 'following' ? !!resolveTargetUserId() : !!login.value),
);

const followingOptions = computed(() =>
  followings.value.map((f) => ({
    value: f.userId,
    label: `${f.userName} (${f.userId})`,
  })),
);

// Sync MWC tabs with tab state.
type MdTabs = HTMLElement & { activeTabIndex: number };
const tabsRef = ref<MdTabs | null>(null);
function onTabChange() {
  if (tabsRef.value) {
    const next = tabsRef.value.activeTabIndex === 0 ? 'bookmark' : 'following';
    if (next !== tab.value) tab.value = next;
  }
}
watch(tab, (v) => {
  if (tabsRef.value) {
    tabsRef.value.activeTabIndex = v === 'bookmark' ? 0 : 1;
  }
});
onMounted(() => {
  if (tabsRef.value) {
    tabsRef.value.activeTabIndex = tab.value === 'bookmark' ? 0 : 1;
    tabsRef.value.addEventListener('change', onTabChange);
  }
});
onUnmounted(() => {
  tabsRef.value?.removeEventListener('change', onTabChange);
});

// Sync MWC select with selectedFollowing.
type MdSelect = HTMLElement & { value: string };
const followingSelectRef = ref<MdSelect | null>(null);
function onFollowingChange() {
  if (followingSelectRef.value) {
    selectedFollowing.value = followingSelectRef.value.value || null;
  }
}
function syncFollowingSelect() {
  if (followingSelectRef.value) {
    const next = selectedFollowing.value ?? '';
    if (followingSelectRef.value.value !== next) {
      followingSelectRef.value.value = next;
    }
  }
}
watch(selectedFollowing, syncFollowingSelect);
watch(followings, () => {
  // After options render, ensure the selected value is still applied.
  requestAnimationFrame(syncFollowingSelect);
});
function bindFollowingSelect() {
  if (!followingSelectRef.value) return;
  syncFollowingSelect();
  followingSelectRef.value.addEventListener('change', onFollowingChange);
}
function unbindFollowingSelect() {
  followingSelectRef.value?.removeEventListener('change', onFollowingChange);
}
watch(followingSelectRef, (el, prev) => {
  if (prev) unbindFollowingSelect();
  if (el) bindFollowingSelect();
});
onMounted(() => {
  bindFollowingSelect();
});
onUnmounted(() => {
  unbindFollowingSelect();
});
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

.pixiv-spin {
  --md-circular-progress-size: 18px;
  --md-circular-progress-active-indicator-width: 6;
  margin-right: 8px;
}

.pixiv-logs {
  max-height: 320px;
  overflow-y: auto;
}

.pixiv-log-icon {
  color: var(--md-sys-color-on-surface-variant);
  margin-right: 8px;
  font-size: 12px;
}
</style>
