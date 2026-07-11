<template>
  <div class="pa-6 pixiv-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ t('nav.pixiv') }}</h2>
      <span class="spacer" />
      <SearchBox
        v-if="login?.user_id && tab === 'recommend'"
        :model-value="store.searchKeyword"
        :placeholder="t('pixiv.search.placeholder')"
        :clear-label="t('pixiv.search.clear')"
        @commit="store.setSearchKeyword"
      />
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
          <FeedList
            :feed="store.search"
            :texts="{
              empty: t('pixiv.search.empty'),
              end: t('pixiv.browse.end'),
              loadingMore: t('pixiv.browse.loadingMore'),
            }"
            @load-more="store.loadMore('search')"
          >
            <SourceCard
              v-for="w in store.search.items"
              :key="'s-' + w.id"
              :title="w.title"
              :page-count="w.pageCount"
              :subtitle="w.author"
              :cover="store.coverMap[w.id] ?? null"
              :status="store.statusMap[w.id]"
              @click="onCardClick(w)"
            />
          </FeedList>
        </div>

        <!-- 推荐（无词时） -->
        <div v-show="!store.searchKeyword">
          <FeedList
            :feed="store.recommend"
            :texts="{
              empty: t('pixiv.browse.empty'),
              end: t('pixiv.browse.end'),
              loadingMore: t('pixiv.browse.loadingMore'),
            }"
            @load-more="store.loadMore('recommend')"
          >
            <SourceCard
              v-for="w in store.recommend.items"
              :key="'r-' + w.id"
              :title="w.title"
              :page-count="w.pageCount"
              :subtitle="w.author"
              :cover="store.coverMap[w.id] ?? null"
              :status="store.statusMap[w.id]"
              @click="onCardClick(w)"
            />
          </FeedList>
        </div>
      </div>

      <!-- 关注 feed -->
      <div v-show="tab === 'following'">
        <FeedList
          :feed="store.following"
          :texts="{
            empty: t('pixiv.browse.empty'),
            end: t('pixiv.browse.end'),
            loadingMore: t('pixiv.browse.loadingMore'),
          }"
          @load-more="store.loadMore('following')"
        >
          <SourceCard
            v-for="w in store.following.items"
            :key="'f-' + w.id"
            :title="w.title"
            :page-count="w.pageCount"
            :subtitle="w.author"
            :cover="store.coverMap[w.id] ?? null"
            :status="store.statusMap[w.id]"
            @click="onCardClick(w)"
          />
        </FeedList>
      </div>

      <!-- 收藏 feed -->
      <div v-show="tab === 'bookmark'">
        <FeedList
          :feed="store.bookmark"
          :texts="{
            empty: t('pixiv.browse.empty'),
            end: t('pixiv.browse.end'),
            loadingMore: t('pixiv.browse.loadingMore'),
          }"
          @load-more="store.loadMore('bookmark')"
        >
          <SourceCard
            v-for="w in store.bookmark.items"
            :key="'b-' + w.id"
            :title="w.title"
            :page-count="w.pageCount"
            :subtitle="w.author"
            :cover="store.coverMap[w.id] ?? null"
            :status="store.statusMap[w.id]"
            @click="onCardClick(w)"
          />
        </FeedList>
      </div>

      <FabButton
        :icon="mdiRefresh"
        :aria-label="t('lib.refresh')"
        :disabled="currentFeedLoading"
        @click="onReload"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { useRouter } from 'vue-router';
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp } from '@mdi/js';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { useToastStore } from '@/stores/toast';
import { usePixivBrowseStore, type PixivTab } from '@/stores/pixiv-browse';
import MdiIcon from '@/components/MdiIcon.vue';
import SourceCard from '@/components/SourceCard.vue';
import FeedList from '@/components/FeedList.vue';
import SearchBox from '@/components/SearchBox.vue';
import FabButton from '@/components/FabButton.vue';
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
});
// Note: no manual lazy-first-load wiring — each <FeedList>'s sentinel arms on
// mount and auto-loads the first page once the tab becomes visible.

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

  // task://progress is handled at the store level (survives view unmount); the
  // listener is armed at app start (App.vue instantiates the store).

  if (tabsRef.value) {
    tabsRef.value.activeTabIndex = TABS.indexOf(tab.value);
    tabsRef.value.addEventListener('change', onTabChange);
  }
});

onBeforeUnmount(() => {
  unlistenLogin?.();
  tabsRef.value?.removeEventListener('change', onTabChange);
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
</style>
