import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/hooks/useI18n';
import { useToastStore } from '@/stores/toast';
import { usePixivBrowseStore, type PixivTab } from '@/stores/pixiv-browse';
import { SourceCard } from '@/components/SourceCard';
import { FeedList } from '@/components/FeedList';
import { SearchBox } from '@/components/SearchBox';
import { FabButton } from '@/components/FabButton';
import { MdiIcon } from '@/components/MdiIcon';
import { M3eTabs, M3eTab } from '@m3e/react/tabs';
import { M3eButton } from '@m3e/react/button';
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp } from '@mdi/js';
import type { PixivWork } from '@/types';

interface PixivLogin {
  cookie: string;
  user_id: string;
  user_name?: string;
}

const ACTIVE = ['pending', 'running', 'paused'];
// Tab order matches the tab strip below.
const TABS: readonly PixivTab[] = ['recommend', 'following', 'bookmark'];

function feedOf(store: ReturnType<typeof usePixivBrowseStore.getState>, tab: PixivTab) {
  if (tab === 'following') return store.following;
  if (tab === 'bookmark') return store.bookmark;
  return store.recommend;
}

export default function PixivDownload() {
  const navigate = useNavigate();
  const { t } = useI18n();
  const toast = useToastStore();
  const store = usePixivBrowseStore();
  const [login, setLogin] = useState<PixivLogin | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  // Persisted tab (defaults to 'recommend' — the left-most entry — when
  // absent/invalid).
  const [tab, setTab] = useState<PixivTab>(() => {
    try {
      const saved = localStorage.getItem('erolib.pixiv.tab');
      return TABS.includes(saved as PixivTab) ? (saved as PixivTab) : 'recommend';
    } catch {
      return 'recommend';
    }
  });

  useEffect(() => {
    void api.getPixivLogin().then((l) => { if (l) setLogin(l); }).catch(() => {});
  }, []);
  useEffect(() => {
    const unlisten: UnlistenFn[] = [];
    void (async () => {
      const u = await listen<{ user_id: string; cookie: string; user_name?: string }>('pixiv://login', (event) => {
        setLogin({ user_id: event.payload.user_id, cookie: event.payload.cookie, user_name: event.payload.user_name });
        setLoggingIn(false);
        toast.addToast('success', t('pixiv.login.loggedInToast'));
      });
      unlisten.push(u);
    })();
    return () => { unlisten.forEach((fn) => fn()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function onTabChange(v: PixivTab) {
    setTab(v);
    try { localStorage.setItem('erolib.pixiv.tab', v); } catch { /* ignore */ }
  }

  async function startLogin() {
    setLoggingIn(true);
    try {
      await api.openPixivLoginWindow();
    } catch (e) {
      setLoggingIn(false);
      toast.addToast('error', t('pixiv.login.loginFailed', { error: String(e) }));
    }
  }

  /** Logout: clear the persisted session + the in-app browser's cookie memory
   *  for pixiv.net, then drop the browse feed so the next login starts
   *  fresh. */
  async function onLogout() {
    try {
      await api.pixivLogout();
      toast.addToast('success', t('pixiv.login.loggedOut'));
    } catch (e) {
      toast.addToast('error', t('pixiv.login.logoutFailed', { error: String(e) }));
    }
    store.resetAll();
    setLogin(null);
  }

  function isBusy(id: string): boolean {
    const s = store.statusMap[id];
    return !!s?.taskId && ACTIVE.includes(s.taskStatus ?? '');
  }

  async function onDownload(w: PixivWork) {
    if (!login) return;
    try {
      const taskId = await api.taskEnqueuePixivWork(login.cookie, w.id, w.title);
      // Optimistically mark as downloading so the mask shows immediately.
      store.setStatus(w.id, { workId: w.id, taskId, taskStatus: 'pending', progressCurrent: 0, progressTotal: 1 });
      toast.addToast('info', t('pixiv.browse.queued', { title: w.title }));
    } catch (e) {
      toast.addToast('error', t('common.error', { message: String(e) }));
    }
  }

  /** Card click dispatches by state: downloaded → reader, downloading →
   *  ignore, new → enqueue download. */
  async function onCardClick(w: PixivWork) {
    const st = store.statusMap[w.id];
    if (st?.localBookId) { navigate(`/reader/${st.localBookId}`); return; }
    if (isBusy(w.id)) return;
    await onDownload(w);
  }

  /** Manual reload of the current view (drops its cache, re-fetches). */
  function onReload() {
    if (tab === 'recommend') {
      void store.reload(store.searchKeyword ? 'search' : 'recommend');
    } else {
      void store.reload(tab);
    }
  }

  const currentFeedLoading = tab === 'recommend'
    ? (store.searchKeyword ? store.search.loading : store.recommend.loading)
    : feedOf(store, tab).loading;

  const browseTexts = { empty: t('pixiv.browse.empty'), end: t('pixiv.browse.end'), loadingMore: t('pixiv.browse.loadingMore') };

  const card = (w: PixivWork) => (
    <SourceCard key={w.id} title={w.title} pageCount={w.pageCount} subtitle={w.author} cover={store.coverMap[w.id] ?? null} status={store.statusMap[w.id]} onClick={() => onCardClick(w)} />
  );

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.pixiv')}</h2>
        <span className="spacer" />
        {login?.user_id && tab === 'recommend' && (
          <SearchBox value={store.searchKeyword} placeholder={t('pixiv.search.placeholder')} clearLabel={t('pixiv.search.clear')} onChange={() => {}} onCommit={(v) => store.setSearchKeyword(v)} />
        )}
        {!login
          ? <M3eButton variant="filled" disabled={loggingIn} onClick={startLogin}><MdiIcon path={mdiArrowTopRight} size={18} /> {t('pixiv.login.login')}</M3eButton>
          : <M3eButton variant="tonal" disabled={loggingIn} onClick={onLogout}><MdiIcon path={mdiExitToApp} size={18} /> {t('pixiv.login.relogin')}</M3eButton>}
      </div>

      {!login ? (
        <div className="text-center text-medium-emphasis mt-8">{t('pixiv.browse.loginRequired')}</div>
      ) : (
        <>
          {/* md-tabs 语义对齐：M3 tab 条，活动指示条在 tab 间滑动 */}
          <M3eTabs
            className="mb-4"
            variant="primary"
            onChange={(e) => {
              const i = (e.currentTarget as unknown as { selectedIndex: number }).selectedIndex;
              if (i >= 0 && i < TABS.length) onTabChange(TABS[i]);
            }}
          >
            {TABS.map((tKey) => (
              <M3eTab key={tKey} selected={tab === tKey}>{t(`pixiv.tab.${tKey}`)}</M3eTab>
            ))}
          </M3eTabs>

          {/* Feeds stay mounted across tab switches (v-show semantics) so each
              keeps its scroll position; the sentinel re-arms on visibility. */}
          <div style={{ display: tab === 'recommend' ? '' : 'none' }}>
            {/* 搜索结果（有词时） */}
            <div style={{ display: store.searchKeyword ? '' : 'none' }}>
              <FeedList feed={store.search} texts={{ ...browseTexts, empty: t('pixiv.search.empty') }} onLoadMore={() => store.loadMore('search')}>
                {store.search.items.map(card)}
              </FeedList>
            </div>
            {/* 推荐（无词时） */}
            <div style={{ display: store.searchKeyword ? 'none' : '' }}>
              <FeedList feed={store.recommend} texts={browseTexts} onLoadMore={() => store.loadMore('recommend')}>
                {store.recommend.items.map(card)}
              </FeedList>
            </div>
          </div>
          <div style={{ display: tab === 'following' ? '' : 'none' }}>
            <FeedList feed={store.following} texts={browseTexts} onLoadMore={() => store.loadMore('following')}>
              {store.following.items.map(card)}
            </FeedList>
          </div>
          <div style={{ display: tab === 'bookmark' ? '' : 'none' }}>
            <FeedList feed={store.bookmark} texts={browseTexts} onLoadMore={() => store.loadMore('bookmark')}>
              {store.bookmark.items.map(card)}
            </FeedList>
          </div>

          <FabButton icon={mdiRefresh} ariaLabel={t('lib.refresh')} disabled={currentFeedLoading} onClick={onReload} />
        </>
      )}
    </div>
  );
}
