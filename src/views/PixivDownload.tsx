import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useI18n } from '@/hooks/useI18n';
import { useToastStore } from '@/stores/toast';
import { usePixivBrowseStore } from '@/stores/pixiv-browse';
import { SourceCard } from '@/components/SourceCard';
import { FeedList } from '@/components/FeedList';
import { SearchBox } from '@/components/SearchBox';
import { FabButton } from '@/components/FabButton';
import { MdiIcon } from '@/components/MdiIcon';
import { mdiArrowTopRight, mdiRefresh, mdiExitToApp } from '@mdi/js';
import type { PixivWork } from '@/types';

const TABS = ['recommend', 'following', 'bookmark'] as const;

export default function PixivDownload() {
  const navigate = useNavigate();
  const { t } = useI18n();
  const toast = useToastStore();
  const store = usePixivBrowseStore();
  const [login, setLogin] = useState<{ user_id: string; cookie: string; user_name: string } | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const [tab, setTab] = useState<string>(() => { try { return localStorage.getItem('erolib.pixiv.tab') ?? 'recommend'; } catch { return 'recommend'; } });

  useEffect(() => { void api.getPixivLogin().then((l) => { if (l?.user_id) setLogin(l); }).catch(() => {}); }, []);
  useEffect(() => {
    const unlisten: UnlistenFn[] = [];
    void (async () => { const u = await listen<{ user_id: string; cookie: string; user_name: string }>('pixiv://login', (event) => { setLogin(event.payload); setLoggingIn(false); }); unlisten.push(u); })();
    return () => { unlisten.forEach((fn) => fn()); };
  }, []);

  async function startLogin() { setLoggingIn(true); await api.openPixivLoginWindow(); }
  async function onLogout() { await api.pixivLogout(); setLogin(null); store.resetAll(); }

  function onCardClick(w: PixivWork) {
    const status = store.statusMap[w.id];
    if (status?.localBookId) { navigate(`/reader/${status.localBookId}`); }
    else if (status?.taskId && ['pending', 'running', 'paused'].includes(status.taskStatus ?? '')) { return; }
    else {
      void api.taskEnqueuePixivWork(w.id, w.title).then(() => {
        store.setStatus(w.id, { workId: w.id, taskId: '', taskStatus: 'pending', progressCurrent: 0, progressTotal: 0 });
        toast.addToast('success', t('tasks.toast.enqueued', { title: w.title }));
      }).catch((e) => { toast.addToast('error', t('common.error', { message: String(e) })); });
    }
  }

  function onTabChange(v: string) { setTab(v); try { localStorage.setItem('erolib.pixiv.tab', v); } catch { /* ignore */ } }

  if (!login) {
    return (
      <div className="pa-6">
        <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
          <h2 className="text-h5" style={{ margin: 0 }}>{t('nav.pixiv')}</h2>
          <span className="spacer" />
          <button className="md3-btn md3-btn--filled" disabled={loggingIn} onClick={startLogin}><MdiIcon path={mdiArrowTopRight} size={18} /> {t('pixiv.login.login')}</button>
        </div>
        <div className="text-center text-medium-emphasis mt-8">{t('pixiv.browse.loginRequired')}</div>
      </div>
    );
  }

  const currentFeed = store[tab as keyof typeof store] as { items: unknown[]; loading: boolean; end: boolean };

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{t('nav.pixiv')}</h2>
        <span className="spacer" />
        {tab === 'recommend' && <SearchBox value={store.searchKeyword} placeholder={t('pixiv.search.placeholder')} clearLabel={t('pixiv.search.clear')} onChange={(v) => {}} onCommit={(v) => store.setSearchKeyword(v)} />}
        <button className="md3-btn md3-btn--tonal" disabled={loggingIn} onClick={onLogout}><MdiIcon path={mdiExitToApp} size={18} /> {t('pixiv.login.relogin')}</button>
      </div>

      <div className="mb-4" style={{ display: 'flex', gap: 0 }}>
        {TABS.map((tKey) => (
          <button key={tKey} className="md3-btn md3-btn--text" onClick={() => onTabChange(tKey)}
            style={{ borderBottom: tab === tKey ? '2px solid var(--md-sys-color-primary)' : '2px solid transparent', borderRadius: 0, padding: '8px 16px', color: tab === tKey ? 'var(--md-sys-color-primary)' : 'var(--md-sys-color-on-surface-variant)' }}>
            {t(`pixiv.tab.${tKey}`)}
          </button>
        ))}
      </div>

      {tab === 'recommend' && store.searchKeyword ? (
        <FeedList feed={store.search} texts={{ empty: t('pixiv.search.empty'), end: t('pixiv.browse.end'), loadingMore: t('pixiv.browse.loadingMore') }} onLoadMore={() => store.loadMore('search')}>
          {store.search.items.map((w) => (<SourceCard key={w.id} title={w.title} pageCount={w.pageCount} subtitle={w.author} cover={store.coverMap[w.id] ?? null} status={store.statusMap[w.id]} onClick={() => onCardClick(w)} />))}
        </FeedList>
      ) : (
        <FeedList feed={store[tab as 'recommend']} texts={{ empty: t('pixiv.browse.empty'), end: t('pixiv.browse.end'), loadingMore: t('pixiv.browse.loadingMore') }} onLoadMore={() => store.loadMore(tab as 'recommend')}>
          {store[tab as 'recommend'].items.map((w: PixivWork) => (<SourceCard key={w.id} title={w.title} pageCount={w.pageCount} subtitle={w.author} cover={store.coverMap[w.id] ?? null} status={store.statusMap[w.id]} onClick={() => onCardClick(w)} />))}
        </FeedList>
      )}

      <FabButton icon={mdiRefresh} ariaLabel={t('common.refresh')} disabled={currentFeed.loading} onClick={() => store.reload(tab as 'recommend')} />
    </div>
  );
}