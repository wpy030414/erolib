import { useParams, useNavigate } from 'react-router-dom';
import { useState, useEffect, useRef } from 'react';
import { useI18n } from '@/hooks/useI18n';
import { useThemeStore } from '@/stores/theme';
import { api } from '@/services/api';
import { mdiArrowLeft, mdiImageSizeSelectActual, mdiImageSizeSelectLarge } from '@mdi/js';

export default function Reader() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { t } = useI18n();

  const [title, setTitle] = useState('');
  const [pageCount, setPageCount] = useState(0);
  const [current, setCurrent] = useState(0);
  const [src, setSrc] = useState<string | null>(null);
  const [zoomMode, setZoomMode] = useState<'fill' | 'contain'>('contain');
  const [loading, setLoading] = useState(true);
  const blobs = useRef<Record<number, string>>({});
  const viewportRef = useRef<HTMLDivElement>(null);

  // Force dark mode
  const prevSeed = useRef(useThemeStore.getState().seed);
  const prevMode = useRef(useThemeStore.getState().mode);
  useEffect(() => {
    useThemeStore.getState().setMode('dark');
    return () => { useThemeStore.getState().setSeed(prevSeed.current); useThemeStore.getState().setMode(prevMode.current); };
  }, []);

  // Load book
  useEffect(() => {
    if (!id) return;
    void (async () => {
      try {
        const book = await api.getBook(id);
        setTitle(book.title);
        const count = await api.getBookPageCount(id);
        setPageCount(count);
        await loadPage(0);
        setLoading(false);
      } catch { setLoading(false); }
    })();
    return () => { for (const url of Object.values(blobs.current)) URL.revokeObjectURL(url); };
  }, [id]);

  async function loadPage(idx: number) {
    if (blobs.current[idx]) { setSrc(blobs.current[idx]); return; }
    try {
      const bytes = await api.getBookPage(id!, idx);
      const blob = new Blob([new Uint8Array(bytes)]);
      const url = URL.createObjectURL(blob);
      blobs.current[idx] = url;
      setSrc(url);
    } catch { /* ignore */ }
  }

  function goTo(idx: number) {
    const clamped = Math.max(0, Math.min(idx, pageCount - 1));
    setCurrent(clamped);
    void loadPage(clamped);
  }

  function onViewportClick(e: React.MouseEvent) {
    const rect = viewportRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = e.clientX - rect.left;
    if (x < rect.width / 3) goTo(current - 1);
    else if (x > rect.width * 2 / 3) goTo(current + 1);
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (e.key === 'ArrowRight' || e.key === 'PageDown') { goTo(current + 1); e.preventDefault(); }
      else if (e.key === 'ArrowLeft' || e.key === 'PageUp') { goTo(current - 1); e.preventDefault(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [current, pageCount]);

  return (
    <div className="reader fill-height" style={{ display: 'flex', flexDirection: 'column', height: '100vh', background: '#000', color: '#fff' }}>
      <header style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 16px', background: 'rgba(0,0,0,0.8)', zIndex: 10 }}>
        <button onClick={() => navigate(-1)} style={{ background: 'none', border: 'none', color: '#fff', cursor: 'pointer', padding: 8, borderRadius: '50%' }} title={t('reader.back')}>
          <svg width={22} height={22} viewBox="0 0 24 24" fill="currentColor"><path d={mdiArrowLeft} /></svg>
        </button>
        <div style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 16 }}>{title || t('reader.untitled')}</div>
        <button onClick={() => setZoomMode(zoomMode === 'fill' ? 'contain' : 'fill')} style={{ background: 'none', border: 'none', color: '#fff', cursor: 'pointer', padding: 8, borderRadius: '50%' }}>
          <svg width={22} height={22} viewBox="0 0 24 24" fill="currentColor"><path d={zoomMode === 'fill' ? mdiImageSizeSelectActual : mdiImageSizeSelectLarge} /></svg>
        </button>
      </header>
      <div ref={viewportRef} onClick={onViewportClick} style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', overflow: 'hidden', minHeight: 0 }}>
        {src ? (
          <img src={src} alt={`Page ${current + 1}`} style={zoomMode === 'fill' ? { width: '100%', height: '100%', objectFit: 'cover' } : { maxWidth: '100%', maxHeight: '100%', objectFit: 'contain' }} draggable={false} />
        ) : loading ? (
          <div style={{ textAlign: 'center' }}><svg className="spinner" style={{ color: 'var(--md-sys-color-primary)' }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg></div>
        ) : null}
      </div>
      <footer style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 16px', background: 'rgba(0,0,0,0.8)' }}>
        <span style={{ fontSize: 14 }}>{current + 1}</span>
        <input type="range" min={0} max={Math.max(0, pageCount - 1)} value={current} onChange={(e) => goTo(Number(e.target.value))} style={{ flex: 1 }} />
        <span style={{ fontSize: 14 }}>{pageCount || '?'}</span>
      </footer>
    </div>
  );
}