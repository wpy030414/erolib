import { useEffect, useState } from 'react';
import { useAhentaiBrowseStore } from '@/stores/ahentai-browse';
import { useI18n } from '@/hooks/useI18n';
import { useToastStore } from '@/stores/toast';
import { useNavigate } from 'react-router-dom';
import { api } from '@/services/api';
import { SourceCard } from '@/components/SourceCard';
import { FeedList } from '@/components/FeedList';
import { SearchBox } from '@/components/SearchBox';
import { FabButton } from '@/components/FabButton';
import { mdiRefresh } from '@mdi/js';
import type { AhentaiGalleryItem } from '@/types';

export default function AHentai() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const toast = useToastStore();
  const store = useAhentaiBrowseStore();

  useEffect(() => { void store.loadMore(); }, []);

  function isBusy(id: string): boolean {
    const s = store.statusMap[id];
    return s ? ['pending', 'running', 'paused'].includes(s.taskStatus ?? '') : false;
  }

  async function onDownload(it: AhentaiGalleryItem) {
    try {
      await api.taskEnqueueAhentaiGallery(it.id, it.title);
      store.setStatus(it.id, { galleryId: it.id, progressCurrent: 0, progressTotal: it.pageCount });
    } catch (e) {
      toast.addToast('error', t('common.error', { message: String(e) }));
    }
  }

  function onCardClick(it: AhentaiGalleryItem) {
    const s = store.statusMap[it.id];
    if (s?.localBookId) { navigate(`/reader/${s.localBookId}`); return; }
    if (isBusy(it.id)) return;
    void onDownload(it);
  }

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6">
        <h2 className="text-h5" style={{ margin: 0 }}>ASMHentai</h2>
        <span className="spacer" />
        <SearchBox
          value={store.keyword}
          placeholder={t('ah.search.placeholder')}
          clearLabel={t('ah.search.clear')}
          onChange={(v) => {}}
          onCommit={(v) => { store.keyword = v; void store.reload(); }}
        />
      </div>
      <FeedList
        feed={store.feed}
        texts={{ empty: t('ah.browse.empty'), end: t('ah.browse.end'), loadingMore: t('ah.browse.loadingMore') }}
        onLoadMore={() => store.loadMore()}
      >
        {store.feed.items.map((it) => (
          <SourceCard
            key={it.id}
            title={it.title}
            pageCount={it.pageCount}
            subtitle={it.uploader}
            cover={store.coverMap[it.id] ?? null}
            status={store.statusMap[it.id]}
            onClick={() => onCardClick(it)}
          />
        ))}
      </FeedList>
      <FabButton icon={mdiRefresh} ariaLabel={t('ah.browse.refresh')} onClick={() => store.reload()} />
    </div>
  );
}