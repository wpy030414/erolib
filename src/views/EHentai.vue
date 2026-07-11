<template>
  <div class="pa-6 ehentai-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ titleText }}</h2>
      <md-switch v-show="loggedIn" ref="exSwitchRef" :selected="store.ex" :aria-label="t('eh.exLabel')" />
      <span class="spacer" />
      <SearchBox
        v-if="loggedIn"
        :model-value="store.keyword"
        :placeholder="t('eh.search.placeholder')"
        :clear-label="t('common.clear')"
        @commit="onSearchCommit"
      />
      <md-filled-button v-if="!loggedIn" :disabled="loggingIn" @click="startLogin">
        <MdiIcon slot="icon" :path="mdiArrowTopRight" :size="18" />
        {{ t('eh.login.login') }}
      </md-filled-button>
      <md-filled-tonal-button v-else :disabled="loggingIn" @click="onLogout">
        <MdiIcon slot="icon" :path="mdiExitToApp" :size="18" />
        {{ t('eh.login.relogin') }}
      </md-filled-tonal-button>
    </div>

    <div v-if="!loggedIn" class="text-center text-medium-emphasis mt-8">
      {{ t('eh.browse.loginRequired') }}
    </div>

    <template v-else>
      <!-- Category chips (single-select; path segment filters the listing). -->
      <div class="cat-chips mb-6">
        <button
          v-for="c in categories"
          :key="c.path"
          class="cat-chip"
          :class="{ 'cat-chip--selected': store.category === c.path }"
          :aria-pressed="store.category === c.path"
          @click="store.selectCategory(store.category === c.path ? null : c.path)"
        >
          {{ t(c.label) }}
        </button>
      </div>

      <!-- Results grid -->
      <FeedList
        :feed="store.feed"
        :texts="{
          empty: t('eh.browse.empty'),
          end: t('eh.browse.end'),
          loadingMore: t('eh.browse.loadingMore'),
        }"
        @load-more="store.loadMore"
      >
        <SourceCard
          v-for="it in store.feed.items"
          :key="it.gid"
          :title="it.title"
          :page-count="it.pageCount"
          :subtitle="it.uploader"
          :cover="store.coverMap[it.gid] ?? null"
          :status="store.statusMap[gurl(it)]"
          @click="onCardClick(it)"
        />
      </FeedList>

      <FabButton
        :icon="mdiRefresh"
        :aria-label="t('lib.refresh')"
        :disabled="store.feed.loading"
        @click="onReload"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { useRouter } from 'vue-router';
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp } from '@mdi/js';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { useToastStore } from '@/stores/toast';
import { useEhentaiBrowseStore } from '@/stores/ehentai-browse';
import MdiIcon from '@/components/MdiIcon.vue';
import SourceCard from '@/components/SourceCard.vue';
import FeedList from '@/components/FeedList.vue';
import SearchBox from '@/components/SearchBox.vue';
import FabButton from '@/components/FabButton.vue';
import type { GalleryListItem } from '@/types';

const { t } = useI18n();
const toast = useToastStore();
const router = useRouter();
const store = useEhentaiBrowseStore();

const ACTIVE = ['pending', 'running', 'paused'];

const loggedIn = ref(false);
const loggingIn = ref(false);
const cookie = ref('');

const titleText = computed(() => (store.ex ? t('nav.exhentai') : t('nav.ehentai')));

/** The 10 e-hentai categories as path segments — e-hentai filters by the path
 *  (e.g. /doujinshi), NOT the f_cats bitmask (which doesn't actually filter). */
const categories = [
  { path: 'doujinshi', label: 'eh.category.doujinshi' },
  { path: 'manga', label: 'eh.category.manga' },
  { path: 'artistcg', label: 'eh.category.artistcg' },
  { path: 'gamecg', label: 'eh.category.gamecg' },
  { path: 'western', label: 'eh.category.western' },
  { path: 'non-h', label: 'eh.category.nonh' },
  { path: 'imageset', label: 'eh.category.imageset' },
  { path: 'cosplay', label: 'eh.category.cosplay' },
  { path: 'asianporn', label: 'eh.category.asianporn' },
  { path: 'misc', label: 'eh.category.misc' },
] as const;

function gurl(it: GalleryListItem): string {
  return store.galleryUrlOf(it);
}

function isBusy(url: string): boolean {
  const s = store.statusMap[url];
  return !!s?.taskId && ACTIVE.includes(s.taskStatus ?? '');
}

async function onDownload(it: GalleryListItem) {
  if (!cookie.value) return;
  const url = gurl(it);
  try {
    const taskId = await api.taskEnqueueEhentaiGallery(cookie.value, url, it.title);
    // Optimistically mark as downloading so the mask shows immediately.
    store.setStatus(url, {
      galleryUrl: url,
      taskId,
      taskStatus: 'pending',
      progressCurrent: 0,
      progressTotal: 1,
    });
    toast.addToast('info', t('eh.browse.queued', { title: it.title }));
  } catch (e) {
    toast.addToast('error', t('common.error', { message: String(e) }));
  }
}

/** Card click dispatches by state: downloaded → reader, downloading → ignore,
 *  new → enqueue download. */
async function onCardClick(it: GalleryListItem) {
  const url = gurl(it);
  const st = store.statusMap[url];
  if (st?.localBookId) {
    router.push(`/reader/${st.localBookId}`);
    return;
  }
  if (isBusy(url)) return;
  await onDownload(it);
}

/** SearchBox commit: push the keyword into the store and reload. The box's own
 *  debounce already throttled the keystrokes; an empty commit clears. */
function onSearchCommit(v: string) {
  store.keyword = v;
  store.reload();
}

async function startLogin() {
  loggingIn.value = true;
  try {
    await api.openEHentaiLoginWindow();
  } catch (e) {
    loggingIn.value = false;
    console.error('opening login window:', e);
  }
}

/** Logout: clear the persisted cookie + the in-app browser's cookie memory for
 *  e-hentai/exhentai, then drop the browse feed so the next login starts fresh. */
async function onLogout() {
  try {
    await api.ehentaiLogout();
  } catch (e) {
    console.error('ehentai logout:', e);
  }
  cookie.value = '';
  store.resetAll();
  loggedIn.value = false;
}

function onReload() {
  store.reload();
}

// EX switch is command-style (no v-model for MWC switch): read .selected in
// the change handler, mirror it back through store.ex + :selected.
type MdSwitchEl = HTMLElement & { selected: boolean };
const exSwitchRef = ref<MdSwitchEl | null>(null);
function onExChange() {
  if (!exSwitchRef.value) return;
  store.setEx(exSwitchRef.value.selected);
  store.reload();
}

let unlistenLogin: UnlistenFn | undefined;

onMounted(async () => {
  try {
    const saved = await api.getEHentaiLogin();
    if (saved) {
      cookie.value = saved;
      loggedIn.value = true;
    }
  } catch {
    // ignore
  }

  unlistenLogin = await listen<{ cookie: string }>('ehentai://login', (evt) => {
    if (evt.payload.cookie) {
      cookie.value = evt.payload.cookie;
      loggedIn.value = true;
      loggingIn.value = false;
    }
  });

  // task://progress is handled at the store level (armed at app start via
  // App.vue + AppShell). EX switch is command-style:
  exSwitchRef.value?.addEventListener('change', onExChange);

  // No manual first-load / sentinel wiring — <FeedList>'s sentinel auto-loads
  // once the (post-login) grid mounts.
});

onBeforeUnmount(() => {
  unlistenLogin?.();
  exSwitchRef.value?.removeEventListener('change', onExChange);
  // Intentionally do NOT clear store state or revoke covers here — the browse
  // state persists across view switches until the app exits.
});
</script>

<style scoped>
.ehentai-view {
  position: relative;
}

.view-header__title {
  margin: 0;
  white-space: nowrap;
}

.cat-chips {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
}

.cat-chip {
  display: inline-flex;
  align-items: center;
  height: 28px;
  padding: 0 12px;
  border: 1px solid var(--md-sys-color-outline);
  border-radius: var(--md-sys-shape-corner-full);
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 12px;
  line-height: 1;
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    color 0.15s ease,
    border-color 0.15s ease;
}

.cat-chip:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}

.cat-chip--selected {
  background: var(--md-sys-color-secondary-container);
  border-color: transparent;
  color: var(--md-sys-color-on-secondary-container);
}

.cat-chip--selected:hover {
  background: color-mix(
    in srgb,
    var(--md-sys-color-on-secondary-container) 12%,
    var(--md-sys-color-secondary-container)
  );
}
</style>
