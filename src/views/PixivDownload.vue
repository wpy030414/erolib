<template>
  <div class="pa-6 pixiv-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ t('nav.pixiv') }}</h2>
      <span class="spacer" />
      <div v-if="login?.user_id && tab === 'recommend'" class="search-box d-flex align-center">
        <MdiIcon :path="mdiMagnify" :size="18" class="search-icon" />
        <input
          v-model="searchInput"
          class="search-input"
          type="search"
          :placeholder="t('pixiv.search.placeholder')"
        />
        <button
          v-if="searchInput"
          class="search-clear"
          :aria-label="t('pixiv.search.clear')"
          @click="clearSearch"
        >
          <MdiIcon :path="mdiClose" :size="16" />
        </button>
      </div>
      <md-filled-button v-if="!login" :disabled="loggingIn" @click="startLogin">
        <MdiIcon slot="icon" :path="mdiArrowTopRight" :size="18" />
        {{ t('pixiv.login.login') }}
      </md-filled-button>
      <md-filled-tonal-button v-else :disabled="loggingIn" @click="onLogout">
        <MdiIcon slot="icon" :path="mdiExitToApp" :size="18" />
        {{ t('pixiv.login.relogin') }}
      </md-filled-tonal-button>
    </div>

    <div v-if="!login" class="text-center text-medium-emphasis mt-8">
      {{ t('pixiv.browse.loginRequired') }}
    </div>

    <template v-else>
      <md-tabs ref="tabsRef" class="mb-4">
        <md-primary-tab>{{ t('pixiv.tab.recommend') }}</md-primary-tab>
        <md-primary-tab>{{ t('pixiv.tab.following') }}</md-primary-tab>
        <md-primary-tab>{{ t('pixiv.tab.bookmark') }}</md-primary-tab>
      </md-tabs>

      <!-- 随便看看 tab：无关键词→推荐，有关键词→搜索结果（搜索框在标题行） -->
      <div v-show="tab === 'recommend'">
        <!-- 搜索结果（有词时） -->
        <div v-show="store.searchKeyword">
          <div v-if="store.search.items.length" class="md3-grid">
            <PixivCard
              v-for="w in store.search.items"
              :key="'s-' + w.id"
              :work="w"
              :cover="store.coverMap[w.id] ?? null"
              :status="store.statusMap[w.id]"
              @click="onCardClick(w)"
            />
          </div>
          <div v-else-if="!store.search.loading" class="text-center text-medium-emphasis mt-8">
            {{ t('pixiv.search.empty') }}
          </div>
          <div v-if="store.search.end && store.search.items.length" class="feed-end text-center text-medium-emphasis">
            {{ t('pixiv.browse.end') }}
          </div>
          <div v-if="store.search.loading" class="feed-loading text-center text-medium-emphasis">
            <md-circular-progress indeterminate />
            <span>{{ t('pixiv.browse.loadingMore') }}</span>
          </div>
          <div ref="searchSentinel" class="feed-sentinel" />
        </div>

        <!-- 推荐（无词时） -->
        <div v-show="!store.searchKeyword">
          <div v-if="store.recommend.items.length" class="md3-grid">
            <PixivCard
              v-for="w in store.recommend.items"
              :key="'r-' + w.id"
              :work="w"
              :cover="store.coverMap[w.id] ?? null"
              :status="store.statusMap[w.id]"
              @click="onCardClick(w)"
            />
          </div>
          <div v-else-if="!store.recommend.loading" class="text-center text-medium-emphasis mt-8">
            {{ t('pixiv.browse.empty') }}
          </div>
          <div v-if="store.recommend.end && store.recommend.items.length" class="feed-end text-center text-medium-emphasis">
            {{ t('pixiv.browse.end') }}
          </div>
          <div v-if="store.recommend.loading" class="feed-loading text-center text-medium-emphasis">
            <md-circular-progress indeterminate />
            <span>{{ t('pixiv.browse.loadingMore') }}</span>
          </div>
          <div ref="recommendSentinel" class="feed-sentinel" />
        </div>
      </div>

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
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp, mdiMagnify, mdiClose } from '@mdi/js';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { useToastStore } from '@/stores/toast';
import { usePixivBrowseStore, type PixivTab } from '@/stores/pixiv-browse';
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
// Tab order matches the <md-primary-tab> sequence above.
const TABS: readonly PixivTab[] = ['recommend', 'following', 'bookmark'];

const login = ref<PixivLogin | null>(null);
const loggingIn = ref(false);
// Local text in the search box. Seeded from the persisted store keyword so the
// box shows the current search when the view is re-mounted (the store lives
// until the app quits); committed back 500ms after typing settles.
const searchInput = ref(store.searchKeyword);

// Persisted tab (defaults to 'recommend' — the left-most entry — when
// absent/invalid).
const initialPixivTab = (() => {
  try {
    const saved = localStorage.getItem('erolib.pixiv.tab');
    return TABS.includes(saved as PixivTab) ? (saved as PixivTab) : 'recommend';
  } catch {
    return 'recommend';
  }
})();
const tab = ref<PixivTab>(initialPixivTab);

// The 随便看看 tab is in "search mode" when a keyword is committed.
const recommendSearching = computed(() => tab.value === 'recommend' && !!store.searchKeyword);

const currentFeedLoading = computed(() => {
  if (tab.value === 'recommend') {
    return store.searchKeyword ? store.search.loading : store.recommend.loading;
  }
  if (tab.value === 'following') return store.following.loading;
  return store.bookmark.loading;
});

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

/** Manual reload of the current view (drops its cache, re-fetches). */
function onReload() {
  if (tab.value === 'recommend') {
    store.reload(store.searchKeyword ? 'search' : 'recommend');
  } else {
    store.reload(tab.value);
  }
}

/** Commit the search box text to the store, which resets the search feed and
 *  fires the first page. An empty query clears the search (back to recommend). */
function commitSearch() {
  store.setSearchKeyword(searchInput.value);
}

// Auto-search 500ms after typing settles — no Enter needed. The clear (×)
// button bypasses the throttle via clearSearch().
let searchTimer: ReturnType<typeof setTimeout> | null = null;
watch(searchInput, () => {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    searchTimer = null;
    commitSearch();
  }, 500);
});

function clearSearch() {
  searchInput.value = '';
  if (searchTimer) {
    clearTimeout(searchTimer);
    searchTimer = null;
  }
  store.setSearchKeyword('');
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

/** Logout: clear the persisted session + the in-app browser's cookie memory for
 *  pixiv.net, then drop the browse feed so the next login starts fresh. */
async function onLogout() {
  try {
    await api.pixivLogout();
  } catch (e) {
    console.error('pixiv logout:', e);
  }
  if (searchTimer) {
    clearTimeout(searchTimer);
    searchTimer = null;
  }
  searchInput.value = '';
  store.resetAll();
  login.value = null;
}

// Sync MWC tabs (command-style — no v-model).
type MdTabs = HTMLElement & { activeTabIndex: number };
const tabsRef = ref<MdTabs | null>(null);
function onTabChange() {
  if (tabsRef.value) {
    const next = TABS[tabsRef.value.activeTabIndex] ?? 'recommend';
    if (next !== tab.value) tab.value = next;
  }
}

watch(tab, (v) => {
  if (tabsRef.value) tabsRef.value.activeTabIndex = TABS.indexOf(v);
  try {
    localStorage.setItem('erolib.pixiv.tab', v);
  } catch {
    // ignore storage errors
  }
  // Lazy first load when the user first switches to a tab.
  const feed = v === 'recommend' ? store.recommend : v === 'following' ? store.following : store.bookmark;
  if (feed.items.length === 0 && !feed.loading && !feed.end) {
    store.loadMore(v);
  }
});

// After a fresh login, load the default tab if it hasn't been loaded yet.
watch(login, (l) => {
  if (l && store.recommend.items.length === 0 && !store.recommend.loading) {
    store.recommend.end = false;
    store.loadMore('recommend');
  }
});

// Keep the search box in sync if the store keyword changes elsewhere.
watch(() => store.searchKeyword, (kw) => {
  if (kw !== searchInput.value) searchInput.value = kw;
});

const recommendSentinel = ref<HTMLElement | null>(null);
const followingSentinel = ref<HTMLElement | null>(null);
const bookmarkSentinel = ref<HTMLElement | null>(null);
const searchSentinel = ref<HTMLElement | null>(null);
let recommendObserver: IntersectionObserver | null = null;
let followingObserver: IntersectionObserver | null = null;
let bookmarkObserver: IntersectionObserver | null = null;
let searchObserver: IntersectionObserver | null = null;

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
    tabsRef.value.activeTabIndex = TABS.indexOf(tab.value);
    tabsRef.value.addEventListener('change', onTabChange);
  }

  // All four sentinels live in always-rendered (v-show) containers, so they
  // exist now and IntersectionObserver reacts to display changes automatically
  // — no need to re-wire when the tab or search keyword flips.
  const ioOpts: IntersectionObserverInit = { rootMargin: '300px' };
  recommendObserver = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) store.loadMore('recommend');
    },
    ioOpts,
  );
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
  searchObserver = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) store.loadMore('search');
    },
    ioOpts,
  );
  nextTick(() => {
    if (recommendSentinel.value) recommendObserver?.observe(recommendSentinel.value);
    if (followingSentinel.value) followingObserver?.observe(followingSentinel.value);
    if (bookmarkSentinel.value) bookmarkObserver?.observe(bookmarkSentinel.value);
    if (searchSentinel.value) searchObserver?.observe(searchSentinel.value);
  });

  // First load of the active tab — but only if the store hasn't already
  // populated it (state survives view switches until the app quits).
  const activeFeed =
    tab.value === 'recommend'
      ? store.recommend
      : tab.value === 'following'
        ? store.following
        : store.bookmark;
  if (activeFeed.items.length === 0 && !activeFeed.end && !activeFeed.loading) {
    store.loadMore(tab.value);
  }
});

onBeforeUnmount(() => {
  unlistenLogin?.();
  if (searchTimer) clearTimeout(searchTimer);
  recommendObserver?.disconnect();
  followingObserver?.disconnect();
  bookmarkObserver?.disconnect();
  searchObserver?.disconnect();
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

/* Search-box styles are global (src/styles/md3.css) — shared with Library
   and EHentai. */

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

/* Floating reload button (bottom-right) — reloads the current view on demand. */
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
