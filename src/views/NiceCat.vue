<template>
  <div class="pa-6 nicecat-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ t('nav.nicecat') }}</h2>
      <span class="spacer" />
      <SearchBox
        :model-value="store.keyword"
        :placeholder="t('nc.search.placeholder')"
        :clear-label="t('common.clear')"
        @commit="onSearchCommit"
      />
    </div>

    <!-- Homepage mode: topic sections with horizontal scroll -->
    <template v-if="!store.isSearching">
      <!-- Topic sections (MD3 outlined card) -->
      <div
        v-for="section in store.sections"
        :key="section.name"
        class="topic-section mb-6"
      >
        <h3 class="text-h6 section-title">{{ section.name }}</h3>
        <div class="horizontal-scroll-wrapper">
          <div class="horizontal-scroll">
            <div class="horizontal-scroll__inner">
              <SourceCard
                v-for="comic in section.comics"
                :key="comic.uid"
                :title="comic.name"
                :page-count="0"
                :cover="store.coverMap[comic.uid] ?? null"
                :status="store.statusMap[comic.uid]"
                class="horizontal-scroll__card"
                @click="onCardClick(comic)"
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Error state -->
      <div v-if="store.error && !store.homeLoading" class="error-state">
        <p class="error-state__msg">{{ store.error }}</p>
      </div>

      <!-- Empty state -->
      <div v-else-if="!store.homeLoading && store.sections.length === 0" class="empty-state">
        {{ t('nc.browse.empty') }}
      </div>

      <!-- Loading -->
      <FeedLoading v-if="store.homeLoading">{{ t('nc.browse.loadingMore') }}</FeedLoading>

      <!-- Refresh FAB -->
      <FabButton
        :icon="mdiRefresh"
        :aria-label="t('nc.home.refresh')"
        :disabled="store.homeLoading"
        @click="() => store.reload(true)"
      />
    </template>

    <!-- Search mode: standard grid -->
    <template v-else>
      <!-- Error state -->
      <div v-if="store.error && !store.feed.loading && store.feed.items.length === 0" class="error-state">
        <p class="error-state__msg">{{ store.error }}</p>
      </div>

      <FeedList
        :feed="store.feed"
        :texts="{
          empty: t('nc.browse.empty'),
          end: t('nc.browse.end'),
          loadingMore: t('nc.browse.loadingMore'),
        }"
        @load-more="store.searchMore"
      >
        <SourceCard
          v-for="it in store.feed.items"
          :key="it.uid"
          :title="it.name"
          :page-count="0"
          :cover="store.coverMap[it.uid] ?? null"
          :status="store.statusMap[it.uid]"
          @click="onCardClick(it)"
        />
      </FeedList>

      <FabButton
        :icon="mdiRefresh"
        :aria-label="t('nc.home.refresh')"
        :disabled="store.feed.loading"
        @click="() => store.reload(true)"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router';
import { mdiRefresh } from '@mdi/js';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { useToastStore } from '@/stores/toast';
import { useNicecatBrowseStore } from '@/stores/nicecat-browse';
import SourceCard from '@/components/SourceCard.vue';
import FeedList from '@/components/FeedList.vue';
import FeedLoading from '@/components/FeedLoading.vue';
import SearchBox from '@/components/SearchBox.vue';
import FabButton from '@/components/FabButton.vue';
import type { NicecatComicItem } from '@/types';

const { t } = useI18n();
const toast = useToastStore();
const router = useRouter();
const store = useNicecatBrowseStore();

/** Enqueue download via the task system, optimistically marking the card as
 *  downloading so the progress mask shows immediately. */
async function onDownload(it: NicecatComicItem) {
  try {
    const taskId = await api.taskEnqueueNicecatGallery(it.uid, it.name);
    store.setStatus(it.uid, {
      comicId: it.uid,
      taskId,
      taskStatus: 'pending',
      progressCurrent: 0,
      progressTotal: 1,
    });
    toast.addToast('info', t('nc.browse.queued', { title: it.name }));
  } catch (e) {
    toast.addToast('error', t('common.error', { message: String(e) }));
  }
}

/** Card click dispatches by state: downloaded → reader, downloading → ignore,
 *  new → enqueue download. */
function onCardClick(it: NicecatComicItem) {
  const st = store.statusMap[it.uid];
  if (st?.localBookId) {
    router.push(`/reader/${st.localBookId}`);
    return;
  }
  if (store.isBusy(it.uid)) return;
  onDownload(it);
}

/** SearchBox commit: push keyword into store and reload. */
function onSearchCommit(v: string) {
  store.keyword = v;
  store.reload();
}

// Initial load.
store.loadHomepage();
</script>

<style scoped>
.nicecat-view {
  position: relative;
}

.view-header__title {
  margin: 0;
  white-space: nowrap;
}

/* ---- MD3 topic section card (outlined, matching task cards) ---- */
.topic-section {
  background: var(--md-sys-color-surface);
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: var(--md-sys-shape-corner-medium);
  padding-top: 16px;
}

.section-title {
  margin: 0;
  padding: 0 16px 2px 16px;
  color: var(--md-sys-color-on-surface);
}

/* ---- Horizontal scroll with hover-lift clearance ---- */
/* overflow-x:auto forces overflow-y:auto per CSS spec, so we use a two-layer
   wrapper to keep the horizontal scrollbar while giving hover-lift room. */
.horizontal-scroll-wrapper {
  overflow-x: auto;
  overflow-y: hidden;
  -webkit-overflow-scrolling: touch;
  scrollbar-width: thin;
}

.horizontal-scroll {
  display: flow-root;
  overflow: visible;
}

.horizontal-scroll__inner {
  display: flex;
  gap: 12px;
  padding: 4px 16px 16px 16px;
  width: max-content;
}

.horizontal-scroll__card {
  flex-shrink: 0;
  width: 160px;
}

.error-state {
  text-align: center;
  color: var(--md-sys-color-error);
  margin-top: 16px;
  padding: 12px 16px;
  background: var(--md-sys-color-error-container);
  border-radius: 8px;
}

.error-state__msg {
  margin: 0;
  font-size: 0.875rem;
  word-break: break-all;
}

.empty-state {
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
  margin-top: 48px;
}
</style>
