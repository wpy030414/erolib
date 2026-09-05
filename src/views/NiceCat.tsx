import { useEffect, useState } from 'react';
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

  useEffect(() => {
    if (!store.loaded && !store.homeLoading && !store.isSearching) void store.loadHomepage();
  }, []);

  function onCardClick(item: NicecatComicItem) {
    const status = store.statusMap[item.uid];
    if (status?.localBookId) { navigate(`/reader/${status.localBookId}`); }
    else if (status?.taskId && ['pending', 'running', 'paused'].includes(status.taskStatus ?? '')) { return; }
    else {
      void api.taskEnqueueNicecatGallery(item.uid, item.name).then(() => {
        store.setStatus(item.uid, { comicId: item.uid, taskId: '', taskStatus: 'pending', progressCurrent: 0, progressTotal: 0 });
        toast.addToast('success', t('tasks.toast.enqueued', { title: item.name }));
      }).catch((e) => { toast.addToast('error', t('common.error', { message: String(e) })); });
    }
  }

  if (!store.isSearching) {
    return (
      <div className="pa-6">
        <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
          <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.nicecat')}</h2>
          <span className="spacer" />
          <SearchBox value={store.keyword} placeholder={t('nc.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={() => {}} />
        </div>
        {store.homeError && <div className="error-state mb-4"><p className="error-state__msg">{store.homeError}</p></div>}
        {store.homeLoading && <FeedLoading>{t('nc.browse.loadingMore')}</FeedLoading>}
        {!store.homeLoading && store.sections.length === 0 && !store.homeError && <div className="text-center text-medium-emphasis mt-8">{t('nc.browse.empty')}</div>}
        {store.sections.map((section) => (
          <div key={section.name} className="md3-card md3-card--outlined mb-4" style={{ overflow: 'hidden' }}>
            <div className="md3-card__content" style={{ padding: '12px 16px' }}><h3 style={{ font: 'var(--md-sys-typescale-title-medium)', margin: 0 }}>{section.name}</h3></div>
            <div style={{ overflowX: 'auto', padding: '0 16px 16px' }}>
              <div style={{ display: 'flex', gap: 12 }}>
                {section.comics.map((comic) => (
                  <div key={comic.uid} style={{ flex: '0 0 160px' }}>
                    <SourceCard title={comic.name} pageCount={0} subtitle={comic.categories} cover={store.coverMap[comic.uid] ?? null} status={store.statusMap[comic.uid]} onClick={() => onCardClick(comic)} />
                  </div>
                ))}
              </div>
            </div>
          </div>
        ))}
        <FabButton icon={mdiRefresh} ariaLabel={t('common.refresh')} disabled={store.homeLoading} onClick={() => store.reload(true)} />
      </div>
    );
  }

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.nicecat')}</h2>
        <span className="spacer" />
        <SearchBox value={store.keyword} placeholder={t('nc.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={() => {}} />
      </div>
      <FeedList feed={store.feed} texts={{ empty: t('nc.search.empty'), end: t('nc.browse.end'), loadingMore: t('nc.browse.loadingMore') }} onLoadMore={() => store.reload()}>
        {store.feed.items.map((item) => (
          <SourceCard key={item.uid} title={item.name} pageCount={0} subtitle={item.categories} cover={store.coverMap[item.uid] ?? null} status={store.statusMap[item.uid]} onClick={() => onCardClick(item)} />
        ))}
      </FeedList>
      <FabButton icon={mdiRefresh} ariaLabel={t('common.refresh')} disabled={store.feed.loading} onClick={() => store.reload(true)} />
    </div>
  );
}