import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/hooks/useI18n';
import { useToastStore } from '@/stores/toast';
import { useEhentaiBrowseStore } from '@/stores/ehentai-browse';
import { SourceCard } from '@/components/SourceCard';
import { FeedList } from '@/components/FeedList';
import { SearchBox } from '@/components/SearchBox';
import { FabButton } from '@/components/FabButton';
import { MdiIcon } from '@/components/MdiIcon';
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp } from '@mdi/js';
import type { GalleryListItem } from '@/types';

const CATEGORIES = ['doujinshi', 'manga', 'artistcg', 'gamecg', 'western', 'non-h', 'imageset', 'cosplay', 'asianporn', 'misc'];

export default function EHentai() {
  const navigate = useNavigate();
  const { t } = useI18n();
  const toast = useToastStore();
  const store = useEhentaiBrowseStore();
  const [loggedIn, setLoggedIn] = useState(false);
  const [loggingIn, setLoggingIn] = useState(false);
  const [cookie, setCookie] = useState<string | null>(null);

  useEffect(() => { void api.getEHentaiLogin().then((c) => { if (c) { setLoggedIn(true); setCookie(c); } }).catch(() => {}); }, []);
  useEffect(() => {
    const unlisten: UnlistenFn[] = [];
    void (async () => { const u = await listen<{ cookie: string }>('ehentai://login', (event) => { setLoggedIn(true); setCookie(event.payload.cookie); setLoggingIn(false); }); unlisten.push(u); })();
    return () => { unlisten.forEach((fn) => fn()); };
  }, []);

  async function startLogin() { setLoggingIn(true); await api.openEHentaiLoginWindow(); }
  async function onLogout() { await api.ehentaiLogout(); setLoggedIn(false); setCookie(null); store.resetAll(); }

  function onCardClick(item: GalleryListItem) {
    const url = store.galleryUrlOf(item);
    const status = store.statusMap[url];
    if (status?.localBookId) { navigate(`/reader/${status.localBookId}`); }
    else if (status?.taskId && ['pending', 'running', 'paused'].includes(status.taskStatus ?? '')) { return; }
    else {
      void api.taskEnqueueEhentaiGallery(url, item.title).then(() => {
        store.setStatus(url, { galleryUrl: url, taskId: '', taskStatus: 'pending', progressCurrent: 0, progressTotal: 0 });
        toast.addToast('success', t('tasks.toast.enqueued', { title: item.title }));
      }).catch((e) => { toast.addToast('error', t('common.error', { message: String(e) })); });
    }
  }

  const title = store.ex ? t('nav.exhentai') : t('nav.ehentai');

  if (!loggedIn) {
    return (
      <div className="pa-6">
        <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
          <h2 className="text-h5" style={{ margin: 0 }}>{title}</h2><span className="spacer" />
          <button className="md3-btn md3-btn--filled" disabled={loggingIn} onClick={startLogin}><MdiIcon path={mdiArrowTopRight} size={18} /> {t('eh.login.title')}</button>
        </div>
        <div className="text-center text-medium-emphasis mt-8">{t('eh.browse.loginRequired')}</div>
      </div>
    );
  }

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{title}</h2>
        <span className="spacer" />
        <label className="d-flex align-center gap-2" style={{ fontSize: 14, color: 'var(--md-sys-color-on-surface-variant)' }}>
          <span>{t('eh.ex')}</span>
          <input type="checkbox" checked={store.ex} onChange={(e) => store.setEx(e.target.checked)} style={{ accentColor: 'var(--md-sys-color-primary)' }} />
        </label>
        <SearchBox value={store.keyword} placeholder={t('eh.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={() => store.reload()} />
        <button className="md3-btn md3-btn--tonal" disabled={loggingIn} onClick={onLogout}><MdiIcon path={mdiExitToApp} size={18} /> {t('eh.login.relogin')}</button>
      </div>

      <div className="tag-chips mb-6">
        {CATEGORIES.map((cat) => (
          <button key={cat} className={`tag-chip${store.category === cat ? ' tag-chip--selected' : ''}`} onClick={() => store.selectCategory(store.category === cat ? null : cat)}>
            <span className="tag-chip__label">{t(`eh.category.${cat}`)}</span>
          </button>
        ))}
      </div>

      <FeedList feed={store.feed} texts={{ empty: t('eh.browse.empty'), end: t('eh.browse.end'), loadingMore: t('eh.browse.loadingMore') }} onLoadMore={() => store.loadMore()}>
        {store.feed.items.map((item) => (
          <SourceCard key={item.gid} title={item.title} pageCount={item.pageCount} subtitle={item.uploader} cover={store.coverMap[item.gid] ?? null} status={store.statusMap[store.galleryUrlOf(item)]} onClick={() => onCardClick(item)} />
        ))}
      </FeedList>

      <FabButton icon={mdiRefresh} ariaLabel={t('common.refresh')} disabled={store.feed.loading} onClick={() => store.reload()} />
    </div>
  );
}