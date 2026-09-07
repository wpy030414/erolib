import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { api } from '@/services/api';
import { useI18n } from '@/hooks/useI18n';
import { useToastStore } from '@/stores/toast';
import { useNicecatBrowseStore } from '@/stores/nicecat-browse';
import { SourceCard } from '@/components/SourceCard';
import { FeedList } from '@/components/FeedList';
import { FeedLoading } from '@/components/FeedLoading';
import { SearchBox } from '@/components/SearchBox';
import { FabButton } from '@/components/FabButton';
import { mdiRefresh } from '@mdi/js';
import type { NicecatComicItem } from '@/types';

export default function NiceCat() {
  const navigate = useNavigate();
  const { t } = useI18n();
  const toast = useToastStore();
  const store = useNicecatBrowseStore();

  // Initial load (the store no-ops while loading / already loaded).
  useEffect(() => { void store.loadHomepage(); }, []);

  /** Enqueue download via the task system, optimistically marking the card as
   *  downloading so the progress mask shows immediately. */
  async function onDownload(it: NicecatComicItem) {
    try {
      const taskId = await api.taskEnqueueNicecatGallery(it.uid, it.name);
      store.setStatus(it.uid, { comicId: it.uid, taskId, taskStatus: 'pending', progressCurrent: 0, progressTotal: 1 });
      toast.addToast('info', t('nc.browse.queued', { title: it.name }));
    } catch (e) {
      toast.addToast('error', t('common.error', { message: String(e) }));
    }
  }

  /** Card click dispatches by state: downloaded → reader, downloading →
   *  ignore, new → enqueue download. */
  function onCardClick(it: NicecatComicItem) {
    const st = store.statusMap[it.uid];
    if (st?.localBookId) { navigate(`/reader/${st.localBookId}`); return; }
    if (store.isBusy(it.uid)) return;
    void onDownload(it);
  }

  /** SearchBox commit: push keyword into store and reload. */
  function onSearchCommit(v: string) {
    store.setKeyword(v);
    void store.reload();
  }

  if (!store.isSearching) {
    return (
      <div className="pa-6">
        <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
          <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.nicecat')}</h2>
          <span className="spacer" />
          <SearchBox value={store.keyword} placeholder={t('nc.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={onSearchCommit} />
        </div>
        {/* Mutually exclusive: error → empty → loading (Vue v-if/else-if chain). */}
        {store.homeError && !store.homeLoading ? (
          <div className="error-state"><p className="error-state__msg">{store.homeError}</p></div>
        ) : !store.homeLoading && store.sections.length === 0 ? (
          <div className="text-center text-medium-emphasis" style={{ marginTop: 48 }}>{t('nc.browse.empty')}</div>
        ) : null}
        {store.homeLoading && <FeedLoading>{t('nc.browse.loadingMore')}</FeedLoading>}
        {store.sections.map((section) => (
          <div key={section.name} className="mb-4" style={{ background: 'var(--md-sys-color-surface)', border: '1px solid var(--md-sys-color-outline-variant)', borderRadius: 'var(--md-sys-shape-corner-medium)', paddingTop: 16, overflow: 'hidden' }}>
            <div className="md3-card__content" style={{ padding: '12px 16px' }}><h3 style={{ font: 'var(--md-sys-typescale-title-medium)', margin: 0 }}>{section.name}</h3></div>
            <div style={{ overflowX: 'auto', padding: '0 16px 16px' }}>
              <div style={{ display: 'flex', gap: 12 }}>
                {section.comics.map((comic) => (
                  <div key={comic.uid} style={{ flex: '0 0 160px' }}>
                    <SourceCard title={comic.name} pageCount={0} cover={store.coverMap[comic.uid] ?? null} status={store.statusMap[comic.uid]} onClick={() => onCardClick(comic)} />
                  </div>
                ))}
              </div>
            </div>
          </div>
        ))}
        <FabButton icon={mdiRefresh} ariaLabel={t('nc.home.refresh')} disabled={store.homeLoading} onClick={() => store.reload(true)} />
      </div>
    );
  }

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.nicecat')}</h2>
        <span className="spacer" />
        <SearchBox value={store.keyword} placeholder={t('nc.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={onSearchCommit} />
      </div>
      {store.homeError && !store.feed.loading && store.feed.items.length === 0 && (
        <div className="error-state mb-4"><p className="error-state__msg">{store.homeError}</p></div>
      )}
      <FeedList feed={store.feed} texts={{ empty: t('nc.browse.empty'), end: t('nc.browse.end'), loadingMore: t('nc.browse.loadingMore') }} onLoadMore={() => store.searchMore()}>
        {store.feed.items.map((item) => (
          <SourceCard key={item.uid} title={item.name} pageCount={0} cover={store.coverMap[item.uid] ?? null} status={store.statusMap[item.uid]} onClick={() => onCardClick(item)} />
        ))}
      </FeedList>
      <FabButton icon={mdiRefresh} ariaLabel={t('nc.home.refresh')} disabled={store.feed.loading} onClick={() => store.reload(true)} />
    </div>
  );
}
