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

const ACTIVE = ['pending', 'running', 'paused'];

export default function AHentai() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const toast = useToastStore();
  const store = useAhentaiBrowseStore();

  function isBusy(id: string): boolean {
    const s = store.statusMap[id];
    return !!s?.taskId && ACTIVE.includes(s.taskStatus ?? '');
  }

  /** Enqueue download via the task system, optimistically marking the card as
   *  downloading so the progress mask shows immediately. The kernel's
   *  task://progress listener handles subsequent progress updates and, on
   *  completion, re-resolves the status to pick up the local book id. */
  async function onDownload(it: AhentaiGalleryItem) {
    try {
      const taskId = await api.taskEnqueueAhentaiGallery(it.id, it.title);
      store.setStatus(it.id, { galleryId: it.id, taskId, taskStatus: 'pending', progressCurrent: 0, progressTotal: 1 });
      toast.addToast('info', t('ah.browse.queued', { title: it.title }));
    } catch (e) {
      toast.addToast('error', t('common.error', { message: String(e) }));
    }
  }

  /** Card click dispatches by state: downloaded → reader, downloading →
   *  ignore, new → enqueue download. */
  function onCardClick(it: AhentaiGalleryItem) {
    const st = store.statusMap[it.id];
    if (st?.localBookId) { navigate(`/reader/${st.localBookId}`); return; }
    if (isBusy(it.id)) return;
    void onDownload(it);
  }

  /** SearchBox commit: push the keyword into the store and reload. */
  function onSearchCommit(v: string) {
    store.setKeyword(v);
    void store.reload();
  }

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6">
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.ahentai')}</h2>
        <span className="spacer" />
        <SearchBox
          value={store.keyword}
          placeholder={t('ah.search.placeholder')}
          clearLabel={t('common.clear')}
          onChange={() => {}}
          onCommit={onSearchCommit}
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
            cover={store.coverMap[it.id] ?? null}
            status={store.statusMap[it.id]}
            onClick={() => onCardClick(it)}
          />
        ))}
      </FeedList>
      <FabButton icon={mdiRefresh} ariaLabel={t('lib.refresh')} disabled={store.feed.loading} onClick={() => store.reload()} />
    </div>
  );
}
