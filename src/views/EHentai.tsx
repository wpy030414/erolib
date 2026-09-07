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
import { M3eSwitch } from '@m3e/react/switch';
import { M3eButton } from '@m3e/react/button';
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp } from '@mdi/js';
import type { GalleryListItem } from '@/types';

const CATEGORIES = ['doujinshi', 'manga', 'artistcg', 'gamecg', 'western', 'non-h', 'imageset', 'cosplay', 'asianporn', 'misc'];

/** Statuses in which a card is "busy" — repeated clicks are ignored. */
const ACTIVE_STATUSES = ['pending', 'running', 'paused'];

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
    void (async () => { const u = await listen<{ cookie: string }>('ehentai://login', (event) => { setLoggedIn(true); setCookie(event.payload.cookie); setLoggingIn(false); toast.addToast('success', t('eh.login.loggedInToast')); }); unlisten.push(u); })();
    return () => { unlisten.forEach((fn) => fn()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function startLogin() {
    setLoggingIn(true);
    try { await api.openEHentaiLoginWindow(); }
    catch (e) {
      setLoggingIn(false);
      toast.addToast('error', t('eh.login.loginFailed', { error: String(e) }));
    }
  }
  async function onLogout() {
    try {
      await api.ehentaiLogout();
      toast.addToast('success', t('eh.login.loggedOut'));
    } catch (e) {
      toast.addToast('error', t('eh.login.logoutFailed', { error: String(e) }));
    } finally {
      setLoggedIn(false);
      setCookie(null);
      store.resetAll();
    }
  }

  function onSearchCommit(v: string) {
    store.setKeyword(v);
    void store.reload();
  }

  async function onCardClick(item: GalleryListItem) {
    const url = store.galleryUrlOf(item);
    const status = store.statusMap[url];
    if (status?.localBookId) { navigate(`/reader/${status.localBookId}`); }
    else if (status?.taskId && ACTIVE_STATUSES.includes(status.taskStatus ?? '')) { return; }
    else {
      try {
        const taskId = await api.taskEnqueueEhentaiGallery(cookie ?? '', url, item.title);
        store.setStatus(url, { galleryUrl: url, taskId, taskStatus: 'pending', progressCurrent: 0, progressTotal: 1 });
        toast.addToast('info', t('eh.browse.queued', { title: item.title }));
      } catch (e) { toast.addToast('error', t('common.error', { message: String(e) })); }
    }
  }

  const title = store.ex ? t('nav.exhentai') : t('nav.ehentai');

  if (!loggedIn) {
    return (
      <div className="pa-6">
        <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
          <h2 className="text-h5" style={{ margin: 0 }}>{title}</h2><span className="spacer" />
          <M3eButton variant="filled" disabled={loggingIn} onClick={startLogin}><MdiIcon path={mdiArrowTopRight} size={18} /> {t('eh.login.login')}</M3eButton>
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
        {/* md-switch 语义对齐：裸开关 + aria-label，滑块弹性滑动 */}
        <M3eSwitch
          checked={store.ex}
          aria-label={t('eh.exLabel')}
          onChange={(e) => { store.setEx((e.currentTarget as unknown as { checked: boolean }).checked); void store.reload(); }}
        />
        <SearchBox value={store.keyword} placeholder={t('eh.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={onSearchCommit} />
        <M3eButton variant="tonal" disabled={loggingIn} onClick={onLogout}><MdiIcon path={mdiExitToApp} size={18} /> {t('eh.login.relogin')}</M3eButton>
      </div>

      <div className="cat-chips mb-6">
        {CATEGORIES.map((cat) => (
          <button key={cat} className={`cat-chip${store.category === cat ? ' cat-chip--selected' : ''}`} aria-pressed={store.category === cat} onClick={() => store.selectCategory(store.category === cat ? null : cat)}>
            <span>{t(`eh.category.${cat}`)}</span>
          </button>
        ))}
      </div>

      <FeedList feed={store.feed} texts={{ empty: t('eh.browse.empty'), end: t('eh.browse.end'), loadingMore: t('eh.browse.loadingMore') }} onLoadMore={() => store.loadMore()}>
        {store.feed.items.map((item) => (
          <SourceCard key={item.gid} title={item.title} pageCount={item.pageCount} subtitle={item.uploader} cover={store.coverMap[item.gid] ?? null} status={store.statusMap[store.galleryUrlOf(item)]} onClick={() => onCardClick(item)} />
        ))}
      </FeedList>

      <FabButton icon={mdiRefresh} ariaLabel={t('lib.refresh')} disabled={store.feed.loading} onClick={() => store.reload()} />
    </div>
  );
}
