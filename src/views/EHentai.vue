<template>
  <div class="pa-6 ehentai-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ titleText }}</h2>
      <md-switch v-show="loggedIn" ref="exSwitchRef" :selected="store.ex" :aria-label="t('eh.exLabel')" />
      <span class="spacer" />
      <div v-if="loggedIn" class="search-box">
        <MdiIcon :path="mdiMagnify" :size="18" class="search-icon" />
        <input
          class="search-input"
          type="search"
          :value="store.keyword"
          :placeholder="t('eh.search.placeholder')"
          @input="onSearchInput"
        />
        <button
          v-if="store.keyword"
          class="search-clear"
          :aria-label="t('common.clear')"
          @click="clearEhSearch"
        >
          <MdiIcon :path="mdiClose" :size="16" />
        </button>
      </div>
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
      <!-- Category chips (multi-select OR; cats = OR of selected bits). -->
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
      <div v-if="store.items.length" class="md3-grid">
        <EHentaiCard
          v-for="it in store.items"
          :key="gurl(it)"
          :item="it"
          :cover="store.coverMap[it.gid] ?? null"
          :status="store.statusMap[gurl(it)]"
          @click="onCardClick(it)"
        />
      </div>
      <div v-else-if="!store.loading" class="text-center text-medium-emphasis mt-8">
        {{ t('eh.browse.empty') }}
      </div>
      <div v-if="store.end && store.items.length" class="feed-end text-center text-medium-emphasis">
        {{ t('eh.browse.end') }}
      </div>
      <div v-if="store.loading" class="feed-loading text-center text-medium-emphasis">
        <md-circular-progress indeterminate />
        <span>{{ t('eh.browse.loadingMore') }}</span>
      </div>
      <div ref="sentinel" class="feed-sentinel" />

      <button
        class="fab-refresh"
        :aria-label="t('lib.refresh')"
        :disabled="store.loading"
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
import { useEhentaiBrowseStore } from '@/stores/ehentai-browse';
import MdiIcon from '@/components/MdiIcon.vue';
import EHentaiCard from '@/components/EHentaiCard.vue';
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

// Debounced search: 500ms after the last keystroke → store.keyword + reload.
let searchTimer: ReturnType<typeof setTimeout> | null = null;
function onSearchInput(e: Event) {
  const v = (e.target as HTMLInputElement).value;
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    store.keyword = v.trim();
    store.reload();
  }, 500);
}

/** Clear the search box and reload immediately (bypasses the input debounce). */
function clearEhSearch() {
  if (searchTimer) {
    clearTimeout(searchTimer);
    searchTimer = null;
  }
  store.keyword = '';
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
  if (searchTimer) {
    clearTimeout(searchTimer);
    searchTimer = null;
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

const sentinel = ref<HTMLElement | null>(null);
let observer: IntersectionObserver | null = null;
let unlistenLogin: UnlistenFn | undefined;

// Load the feed once the user logs in (covers a late login after mount).
watch(loggedIn, (l) => {
  if (l && store.items.length === 0 && !store.loading && !store.end) {
    store.loadMore();
  }
});

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

  // task://progress is handled at the store level (survives view unmount).
  exSwitchRef.value?.addEventListener('change', onExChange);

  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((en) => en.isIntersecting)) store.loadMore();
    },
    { rootMargin: '300px' },
  );
  nextTick(() => {
    if (sentinel.value) observer?.observe(sentinel.value);
  });

  // First load — but only if the store hasn't already populated (state
  // survives view switches until the app quits).
  if (loggedIn.value && store.items.length === 0 && !store.end && !store.loading) {
    store.loadMore();
  }
});

onBeforeUnmount(() => {
  unlistenLogin?.();
  observer?.disconnect();
  exSwitchRef.value?.removeEventListener('change', onExChange);
  if (searchTimer) clearTimeout(searchTimer);
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

/* Floating reload button (bottom-right) — reloads the listing on demand. */
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
