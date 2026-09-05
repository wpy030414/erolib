import { useEffect, useRef, useCallback, lazy, Suspense } from 'react';
import { HashRouter, Routes, Route, useLocation } from 'react-router-dom';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { I18nProvider, onLocaleChange } from '@/hooks/useI18n';
import { useThemeStore } from '@/stores/theme';
import { useSettingsStore } from '@/stores/settings';
import { useLibraryStore } from '@/stores/library';
import { AppShell } from '@/components/AppShell';
import { AppToast } from '@/components/AppToast';

const Home = lazy(() => import('@/views/Home'));
const Library = lazy(() => import('@/views/Library'));
const Reader = lazy(() => import('@/views/Reader'));
const PixivDownload = lazy(() => import('@/views/PixivDownload'));
const EHentai = lazy(() => import('@/views/EHentai'));
const AHentai = lazy(() => import('@/views/AHentai'));
const NiceCat = lazy(() => import('@/views/NiceCat'));
const Tasks = lazy(() => import('@/views/Tasks'));
const Settings = lazy(() => import('@/views/Settings'));

function LoadingFallback() {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 48 }}>
      <svg className="spinner" style={{ color: 'var(--md-sys-color-primary)' }} viewBox="0 0 50 50" aria-hidden="true">
        <circle className="spinner-track" cx="25" cy="25" r="20" />
        <circle className="spinner-arc" cx="25" cy="25" r="20" />
      </svg>
    </div>
  );
}

function isScrollPersistable(path: string) { return !path.startsWith('/reader'); }
function scrollKey(path: string) { return `erolib.scroll.${path}`; }

function AppContent() {
  const location = useLocation();
  const isReader = location.pathname.startsWith('/reader');
  const themeBgImage = useThemeStore((s) => s.themeBgImage);
  const mainRef = useRef<HTMLElement>(null);
  const saveScheduled = useRef(false);
  const oldPath = useRef(location.pathname);

  const scheduleSaveScroll = useCallback(() => {
    if (saveScheduled.current) return;
    saveScheduled.current = true;
    requestAnimationFrame(() => {
      saveScheduled.current = false;
      const el = mainRef.current;
      const path = location.pathname;
      if (!el || !isScrollPersistable(path)) return;
      try { localStorage.setItem(scrollKey(path), String(el.scrollTop)); } catch { /* ignore */ }
    });
  }, [location.pathname]);

  const restoreScroll = useCallback((path: string) => {
    const el = mainRef.current;
    if (!el || !isScrollPersistable(path)) return;
    let target: number;
    try { const raw = localStorage.getItem(scrollKey(path)); if (raw == null) return; target = Number(raw); } catch { return; }
    if (!Number.isFinite(target) || target <= 0) return;
    let attempt = 0;
    const trySet = () => {
      if (el.scrollHeight >= target) { el.scrollTop = target; return; }
      attempt++;
      if (attempt < 30) requestAnimationFrame(trySet);
      else el.scrollTop = target;
    };
    requestAnimationFrame(trySet);
  }, []);

  useEffect(() => {
    const el = mainRef.current;
    if (el && oldPath.current && isScrollPersistable(oldPath.current)) {
      try { localStorage.setItem(scrollKey(oldPath.current), String(el.scrollTop)); } catch { /* ignore */ }
    }
    oldPath.current = location.pathname;
    requestAnimationFrame(() => restoreScroll(location.pathname));
  }, [location.pathname, restoreScroll]);

  useEffect(() => {
    const onKeyDown = async (e: KeyboardEvent) => {
      if (e.key === 'F11') {
        e.preventDefault();
        try { const win = getCurrentWindow(); const isFs = await win.isFullscreen(); await win.setFullscreen(!isFs); } catch { /* ignore */ }
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  useEffect(() => {
    void useSettingsStore.getState().autoStartAll();
    const unsub = onLocaleChange(() => { void useLibraryStore.getState().refresh(); });
    return unsub;
  }, []);

  useEffect(() => {
    function handleMouseOver(e: MouseEvent) {
      const title = (e.target as HTMLElement).closest('.md3-card__title') as HTMLElement | null;
      if (!title || title.dataset.marqueeReady === '1') return;
      title.dataset.marqueeReady = '1';
      const inner = title.querySelector<HTMLElement>('.title-inner');
      if (!inner) return;
      const overflow = inner.scrollWidth - title.clientWidth;
      if (overflow <= 0) return;
      const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
      title.style.setProperty('--title-scroll', `${-overflow}px`);
      title.style.setProperty('--title-dur', `${overflow / (2 * rem) + 3}s`);
      title.classList.add('is-marquee');
    }
    document.addEventListener('mouseover', handleMouseOver);
    return () => document.removeEventListener('mouseover', handleMouseOver);
  }, []);

  return (
    <div id="erolib-app" className="erolib-app d-flex fill-height">
      {themeBgImage && !isReader && <div className="theme-bg-overlay" />}
      {!isReader && <AppShell />}
      <main ref={mainRef} className="app-main flex-grow-1" onScroll={scheduleSaveScroll}>
        <Suspense fallback={<LoadingFallback />}>
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/home" element={<Home />} />
            <Route path="/library" element={<Library />} />
            <Route path="/reader/:id" element={<Reader />} />
            <Route path="/pixiv" element={<PixivDownload />} />
            <Route path="/ehentai" element={<EHentai />} />
            <Route path="/ahentai" element={<AHentai />} />
            <Route path="/nicecat" element={<NiceCat />} />
            <Route path="/tasks" element={<Tasks />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </Suspense>
      </main>
      <AppToast />
    </div>
  );
}

export function App() {
  return (
    <I18nProvider>
      <HashRouter>
        <AppContent />
      </HashRouter>
    </I18nProvider>
  );
}
