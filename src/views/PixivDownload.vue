<template>
  <div class="pa-6 pixiv-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ t('nav.pixiv') }}</h2>
      <span class="spacer" />
      <span v-if="login?.user_id" class="view-header__user">
        {{ t('pixiv.login.loggedAs', { name: login.user_name || login.user_id }) }}
      </span>
      <md-outlined-button :disabled="loggingIn" @click="startLogin">
        <MdiIcon slot="icon" :path="login ? mdiRefresh : mdiArrowTopRight" :size="18" />
        {{ login ? t('pixiv.login.relogin') : t('pixiv.login.login') }}
      </md-outlined-button>
    </div>

    <div v-if="!login" class="text-center text-medium-emphasis mt-8">
      {{ t('pixiv.browse.loginRequired') }}
    </div>

    <template v-else>
      <md-tabs ref="tabsRef" class="mb-4">
        <md-primary-tab>{{ t('pixiv.tab.following') }}</md-primary-tab>
        <md-primary-tab>{{ t('pixiv.tab.bookmark') }}</md-primary-tab>
      </md-tabs>

      <!-- 关注 feed -->
      <div v-show="tab === 'following'">
        <div v-if="store.following.items.length" class="md3-grid">
          <PixivCard
            v-for="w in store.following.items"
            :key="'f-' + w.id"
            :work="w"
            :cover="store.coverMap[w.id] ?? null"
            :status="store.statusMap[w.id]"
            @click="onCardClick(w)"
          />
        </div>
        <div v-else-if="!store.following.loading" class="text-center text-medium-emphasis mt-8">
          {{ t('pixiv.browse.empty') }}
        </div>
        <div v-if="store.following.end && store.following.items.length" class="feed-end text-center text-medium-emphasis">
          {{ t('pixiv.browse.end') }}
        </div>
        <div v-if="store.following.loading" class="feed-loading text-center text-medium-emphasis">
          <md-circular-progress indeterminate />
          <span>{{ t('pixiv.browse.loadingMore') }}</span>
        </div>
        <div ref="followingSentinel" class="feed-sentinel" />
      </div>

      <!-- 收藏 feed -->
      <div v-show="tab === 'bookmark'">
        <div v-if="store.bookmark.items.length" class="md3-grid">
          <PixivCard
            v-for="w in store.bookmark.items"
            :key="'b-' + w.id"
            :work="w"
            :cover="store.coverMap[w.id] ?? null"
            :status="store.statusMap[w.id]"
            @click="onCardClick(w)"
          />
        </div>
        <div v-else-if="!store.bookmark.loading" class="text-center text-medium-emphasis mt-8">
          {{ t('pixiv.browse.empty') }}
        </div>
        <div v-if="store.bookmark.end && store.bookmark.items.length" class="feed-end text-center text-medium-emphasis">
          {{ t('pixiv.browse.end') }}
        </div>
        <div v-if="store.bookmark.loading" class="feed-loading text-center text-medium-emphasis">
          <md-circular-progress indeterminate />
          <span>{{ t('pixiv.browse.loadingMore') }}</span>
        </div>
        <div ref="bookmarkSentinel" class="feed-sentinel" />
      </div>

      <button
        class="fab-refresh"
        :aria-label="t('lib.refresh')"
        :disabled="currentFeedLoading"
        @click="onReload"
      >
        <MdiIcon :path="mdiRefresh" :size="24" />
      </button>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { useRouter } from 'vue-router';
import { mdiArrowTopRight, mdiRefresh } from '@mdi/js';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { useToastStore } from '@/stores/toast';
import { usePixivBrowseStore } from '@/stores/pixiv-browse';
import MdiIcon from '@/components/MdiIcon.vue';
import PixivCard from '@/components/PixivCard.vue';
import type { PixivWork } from '@/types';

const { t } = useI18n();
const toast = useToastStore();
const router = useRouter();
const store = usePixivBrowseStore();

interface PixivLogin {
  cookie: string;
  user_id: string;
  user_name?: string;
}

const ACTIVE = ['pending', 'running', 'paused'];

const login = ref<PixivLogin | null>(null);
const loggingIn = ref(false);
// Persisted tab (defaults to 'following' when absent/invalid).
const initialPixivTab = (() => {
  try {
    return localStorage.getItem('erolib.pixiv.tab') === 'bookmark' ? 'bookmark' : 'following';
  } catch {
    return 'following';
  }
})();
const tab = ref<'following' | 'bookmark'>(initialPixivTab);

const currentFeedLoading = computed(() =>
  tab.value === 'following' ? store.following.loading : store.bookmark.loading,
);

function isBusy(id: string): boolean {
  const s = store.statusMap[id];
  return !!s?.taskId && ACTIVE.includes(s.taskStatus ?? '');
}

async function onDownload(w: PixivWork) {
  if (!login.value) return;
  try {
    const taskId = await api.taskEnqueuePixivWork(login.value.cookie, w.id, w.title);
    // Optimistically mark as downloading so the mask shows immediately.
    store.setStatus(w.id, {
      workId: w.id,
      taskId,
      taskStatus: 'pending',
      progressCurrent: 0,
      progressTotal: 1,
    });
    toast.addToast('info', t('pixiv.browse.queued', { title: w.title }));
  } catch (e) {
    toast.addToast('error', t('common.error', { message: String(e) }));
  }
}

/** Card click dispatches by state: downloaded → reader, downloading → ignore,
 *  new → enqueue download. */
async function onCardClick(w: PixivWork) {
  const st = store.statusMap[w.id];
  if (st?.localBookId) {
    router.push(`/reader/${st.localBookId}`);
    return;
  }
  if (isBusy(w.id)) return;
  await onDownload(w);
}

/** Manual reload of the current tab (drops its cache, re-fetches). */
function onReload() {
  store.reload(tab.value);
}

async function startLogin() {
  loggingIn.value = true;
  try {
    await api.openPixivLoginWindow();
  } catch (e) {
    loggingIn.value = false;
    console.error('opening login window:', e);
  }
}

// Sync MWC tabs (command-style — no v-model).
type MdTabs = HTMLElement & { activeTabIndex: number };
const tabsRef = ref<MdTabs | null>(null);
function onTabChange() {
  if (tabsRef.value) {
    const next = tabsRef.value.activeTabIndex === 0 ? 'following' : 'bookmark';
    if (next !== tab.value) tab.value = next;
  }
}

watch(tab, (v) => {
  if (tabsRef.value) tabsRef.value.activeTabIndex = v === 'following' ? 0 : 1;
  try {
    localStorage.setItem('erolib.pixiv.tab', v);
  } catch {
    // ignore storage errors
  }
  // Lazy first load of 收藏 when the user first switches to it.
  if (
    v === 'bookmark' &&
    store.bookmark.items.length === 0 &&
    !store.bookmark.loading &&
    !store.bookmark.end
  ) {
    store.loadMore('bookmark');
  }
});

// After a fresh login, load the default tab if it hasn't been loaded yet.
watch(login, (l) => {
  if (l && store.following.items.length === 0 && !store.following.loading) {
    store.following.end = false;
    store.loadMore('following');
  }
});

const followingSentinel = ref<HTMLElement | null>(null);
const bookmarkSentinel = ref<HTMLElement | null>(null);
let followingObserver: IntersectionObserver | null = null;
let bookmarkObserver: IntersectionObserver | null = null;

let unlistenLogin: UnlistenFn | undefined;

onMounted(async () => {
  try {
    const l = await api.getPixivLogin();
    if (l) login.value = l;
  } catch {
    // ignore
  }

  unlistenLogin = await listen<{ user_id: string; cookie: string; user_name?: string }>(
    'pixiv://login',
    (evt) => {
      login.value = {
        user_id: evt.payload.user_id,
        cookie: evt.payload.cookie,
        user_name: evt.payload.user_name,
      };
      loggingIn.value = false;
    },
  );

  // task://progress is handled at the store level (survives view unmount).

  if (tabsRef.value) {
    tabsRef.value.activeTabIndex = tab.value === 'following' ? 0 : 1;
    tabsRef.value.addEventListener('change', onTabChange);
  }

  const ioOpts: IntersectionObserverInit = { rootMargin: '300px' };
  followingObserver = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) store.loadMore('following');
    },
    ioOpts,
  );
  bookmarkObserver = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) store.loadMore('bookmark');
    },
    ioOpts,
  );
  nextTick(() => {
    if (followingSentinel.value) followingObserver?.observe(followingSentinel.value);
    if (bookmarkSentinel.value) bookmarkObserver?.observe(bookmarkSentinel.value);
  });

  // First load of the active tab — but only if the store hasn't already
  // populated it (state survives view switches until the app quits).
  const activeFeedState = tab.value === 'following' ? store.following : store.bookmark;
  if (
    activeFeedState.items.length === 0 &&
    !activeFeedState.end &&
    !activeFeedState.loading
  ) {
    store.loadMore(tab.value);
  }
});

onBeforeUnmount(() => {
  unlistenLogin?.();
  followingObserver?.disconnect();
  bookmarkObserver?.disconnect();
  // Intentionally do NOT clear store state or revoke covers here — the browse
  // state persists across view switches until the app exits.
});
</script>

<style scoped>
.pixiv-view {
  position: relative;
}

.view-header__title {
  margin: 0;
  white-space: nowrap;
}

.view-header__user {
  color: var(--md-sys-color-on-surface-variant);
  font-size: 13px;
  white-space: nowrap;
}

.feed-sentinel {
  height: 1px;
}

.feed-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px 0;
}

.feed-end {
  padding: 20px 0;
  font-size: 13px;
}

/* Floating reload button (bottom-right) — reloads the current tab on demand. */
.fab-refresh {
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

.fab-refresh:hover:not(:disabled) {
  box-shadow: var(--md-sys-elevation-level4);
  transform: scale(1.05);
}

.fab-refresh:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
