<template>
  <div class="pa-6 ahentai-view">
    <div class="view-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 view-header__title">{{ t('nav.ahentai') }}</h2>
      <span class="spacer" />
      <SearchBox
        :model-value="store.keyword"
        :placeholder="t('ah.search.placeholder')"
        :clear-label="t('common.clear')"
        @commit="onSearchCommit"
      />
    </div>

    <FeedList
      :feed="store.feed"
      :texts="{
        empty: t('ah.browse.empty'),
        end: t('ah.browse.end'),
        loadingMore: t('ah.browse.loadingMore'),
      }"
      @load-more="store.loadMore"
    >
      <SourceCard
        v-for="it in store.feed.items"
        :key="it.id"
        :title="it.title"
        :page-count="it.pageCount"
        :cover="store.coverMap[it.id] ?? null"
        :status="store.statusMap[it.id]"
        @click="onCardClick(it)"
      />
    </FeedList>

    <FabButton
      :icon="mdiRefresh"
      :aria-label="t('lib.refresh')"
      :disabled="store.feed.loading"
      @click="() => store.reload()"
    />
  </div>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router';
import { mdiRefresh } from '@mdi/js';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { useToastStore } from '@/stores/toast';
import { useAhentaiBrowseStore } from '@/stores/ahentai-browse';
import SourceCard from '@/components/SourceCard.vue';
import FeedList from '@/components/FeedList.vue';
import SearchBox from '@/components/SearchBox.vue';
import FabButton from '@/components/FabButton.vue';
import type { AhentaiGalleryItem } from '@/types';

const { t } = useI18n();
const toast = useToastStore();
const router = useRouter();
const store = useAhentaiBrowseStore();

const ACTIVE = ['pending', 'running', 'paused'];

function isBusy(id: string): boolean {
  const s = store.statusMap[id];
  return !!s?.taskId && ACTIVE.includes(s.taskStatus ?? '');
}

/** Enqueue download via the task system, optimistically marking the card as
 *  downloading so the progress mask shows immediately. The composable's
 *  task://progress listener handles subsequent progress updates and, on
 *  completion, re-resolves the status to pick up the local book id. */
async function onDownload(it: AhentaiGalleryItem) {
  try {
    const taskId = await api.taskEnqueueAhentaiGallery(it.id, it.title);
    store.setStatus(it.id, {
      galleryId: it.id,
      taskId,
      taskStatus: 'pending',
      progressCurrent: 0,
      progressTotal: 1,
    });
    toast.addToast('info', t('ah.browse.queued', { title: it.title }));
  } catch (e) {
    toast.addToast('error', t('common.error', { message: String(e) }));
  }
}

/** Card click dispatches by state: downloaded → reader, downloading → ignore,
 *  new → enqueue download. */
function onCardClick(it: AhentaiGalleryItem) {
  const st = store.statusMap[it.id];
  if (st?.localBookId) {
    router.push(`/reader/${st.localBookId}`);
    return;
  }
  if (isBusy(it.id)) return;
  onDownload(it);
}

/** SearchBox commit: push the keyword into the store and reload. */
function onSearchCommit(v: string) {
  store.keyword = v;
  store.reload();
}
</script>

<style scoped>
.ahentai-view {
  position: relative;
}

.view-header__title {
  margin: 0;
  white-space: nowrap;
}
</style>
