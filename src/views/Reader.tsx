import { useEffect, useState, useRef, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { save as dialogSave } from '@tauri-apps/plugin-dialog';
import { sourceColorFromImage, hexFromArgb } from '@material/material-color-utilities';
import { api } from '@/services/api';
import { deleteThumb } from '@/services/thumb-cache';
import { useI18n } from '@/hooks/useI18n';
import { useThemeStore } from '@/stores/theme';
import { useToastStore } from '@/stores/toast';
import { MdiIcon } from '@/components/MdiIcon';
import {
  mdiArrowLeft, mdiImageSizeSelectActual, mdiImageSizeSelectLarge,
  mdiPalette, mdiContentSave, mdiDelete,
} from '@mdi/js';
import type { Book } from '@/types';

const PREFETCH_SPAN = 10;
const SWIPE_THRESHOLD = 50;
const TICK_CAP_SECONDS = 2;

function mimeFromBytes(buf: ArrayBuffer): string {
  const b = new Uint8Array(buf, 0, Math.min(12, buf.byteLength));
  if (b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return 'image/jpeg';
  if (b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47) return 'image/png';
  if (b[0] === 0x52 && b[1] === 0x49 && b[2] === 0x46 && b[3] === 0x46
    && b[8] === 0x57 && b[9] === 0x45 && b[10] === 0x42 && b[11] === 0x50) return 'image/webp';
  if (b[4] === 0x66 && b[5] === 0x74 && b[6] === 0x79 && b[7] === 0x70) {
    const brand = String.fromCharCode(b[8], b[9], b[10], b[11]);
    if (brand === 'avif' || brand === 'avis') return 'image/avif';
  }
  return 'image/jpeg';
}

function readProgress(bookId: string): number {
  try { const v = Number(localStorage.getItem(`erolib.reader.progress.${bookId}`)); return Number.isFinite(v) && v >= 0 ? v : 0; }
  catch { return 0; }
}
function saveProgress(bookId: string, page: number) {
  try { localStorage.setItem(`erolib.reader.progress.${bookId}`, String(page)); } catch { /* ignore */ }
}
function readZoom(): 'fill' | 'contain' {
  try { const v = localStorage.getItem('erolib.reader.zoomMode'); return v === 'fill' ? 'fill' : 'contain'; }
  catch { return 'contain'; }
}
function saveZoom(mode: 'fill' | 'contain') {
  try { localStorage.setItem('erolib.reader.zoomMode', mode); } catch { /* ignore */ }
}
function readTimeKey(id: string) { return `erolib.reader.readtime.${id}`; }
function loadReadTime(id: string): number {
  try { const v = Number(localStorage.getItem(readTimeKey(id))); return Number.isFinite(v) && v >= 0 ? v : 0; }
  catch { return 0; }
}
function saveReadTime(id: string, sec: number) {
  try { localStorage.setItem(readTimeKey(id), String(Math.max(0, Math.round(sec)))); } catch { /* ignore */ }
}
function fmtReadTime(totalSec: number): string {
  const s = Math.max(0, Math.floor(totalSec));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
}

export default function Reader() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { t } = useI18n();
  const themeStore = useThemeStore();
  const toast = useToastStore();

  // ── Core state ──────────────────────────────────────────────────────
  const [pageCount, setPageCount] = useState<number | null>(null);
  const [title, setTitle] = useState('');
  const [current, setCurrent] = useState(0);
  const [zoomMode, setZoomMode] = useState<'fill' | 'contain'>(readZoom);
  const [uiHidden, setUiHidden] = useState(false);
  const [src, setSrc] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const blobsRef = useRef<Record<number, string>>({});
  const pageExtRef = useRef<Record<number, string>>({});
  const bookMetaRef = useRef<Book | null>(null);
  const prefetchInFlight = useRef<Set<number>>(new Set());

  // ── Animation (ugoira) ──────────────────────────────────────────────
  const frameDelaysRef = useRef<number[]>([]);
  const [isAnimated, setIsAnimated] = useState(false);
  const [animLoading, setAnimLoading] = useState(false);
  const animCanvasRef = useRef<HTMLCanvasElement>(null);
  const bitmapsRef = useRef<(ImageBitmap | null)[]>([]);
  const animFrameRef = useRef(0);
  const animTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const resizeObsRef = useRef<ResizeObserver | null>(null);
  const framesInFlightRef = useRef(false);

  // ── Context menu ────────────────────────────────────────────────────
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 });

  // ── UI auto-hide ────────────────────────────────────────────────────
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wasInZoneRef = useRef(false);

  // ── Reading time ────────────────────────────────────────────────────
  const readAccRef = useRef(0);
  const readStartRef = useRef<number | null>(null);
  const readLastTickRef = useRef<number | null>(null);
  const readTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const readSessionIdRef = useRef<number | null>(null);
  const readSessionBaselineRef = useRef(0);
  const readBookIdRef = useRef<string>('');
  const warnedNoSessionRef = useRef(false);
  const [readTimeDisplay, setReadTimeDisplay] = useState('0s');

  // ── Theme restore ───────────────────────────────────────────────────
  const prevModeRef = useRef(themeStore.mode);
  const prevSeedRef = useRef(themeStore.seed);
  const prevBgRef = useRef(themeStore.themeBgImage);

  // ── Helpers ─────────────────────────────────────────────────────────
  const clamp = useCallback((v: number) => {
    if (pageCount == null) return 0;
    return Math.max(0, Math.min(pageCount - 1, v));
  }, [pageCount]);

  const go = useCallback((delta: number) => {
    setCurrent((prev) => clamp(prev + delta));
  }, [clamp]);

  const goTo = useCallback((idx: number) => {
    setCurrent(clamp(idx));
  }, [clamp]);

  const goBack = useCallback(() => {
    if (window.history.length > 1) navigate(-1);
    else navigate('/library');
  }, [navigate]);

  // ── Animation rendering ─────────────────────────────────────────────
  const drawCurrentFrame = useCallback(() => {
    const canvas = animCanvasRef.current;
    const bmp = bitmapsRef.current[animFrameRef.current];
    if (!canvas || !bmp) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const cw = canvas.width, ch = canvas.height;
    ctx.clearRect(0, 0, cw, ch);
    const scale = zoomMode === 'fill'
      ? Math.max(cw / bmp.width, ch / bmp.height)
      : Math.min(cw / bmp.width, ch / bmp.height);
    const dw = bmp.width * scale, dh = bmp.height * scale;
    ctx.drawImage(bmp, (cw - dw) / 2, (ch - dh) / 2, dw, dh);
  }, [zoomMode]);

  const scheduleNextFrame = useCallback(() => {
    if (animTimerRef.current) clearTimeout(animTimerRef.current);
    if (!isAnimated || pageCount == null) return;
    const delay = frameDelaysRef.current[animFrameRef.current] ?? 100;
    animTimerRef.current = setTimeout(() => {
      animFrameRef.current = (animFrameRef.current + 1) % (pageCount ?? 1);
      drawCurrentFrame();
      scheduleNextFrame();
    }, Math.max(16, delay));
  }, [isAnimated, pageCount, drawCurrentFrame]);

  const resizeCanvas = useCallback(() => {
    const canvas = animCanvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.round(canvas.clientWidth * dpr));
    canvas.height = Math.max(1, Math.round(canvas.clientHeight * dpr));
  }, []);

  // ── Preload frames ──────────────────────────────────────────────────
  const preloadFrames = useCallback(async () => {
    if (!id || !isAnimated || bitmapsRef.current.length > 0 || framesInFlightRef.current) return;
    const n = frameDelaysRef.current.length || pageCount || 0;
    if (n === 0) return;
    framesInFlightRef.current = true;
    setAnimLoading(true);
    try {
      const results = await Promise.all(
        Array.from({ length: n }, async (_, p): Promise<ImageBitmap | null> => {
          try { const buf = await api.getBookPage(id, p); return await createImageBitmap(new Blob([buf], { type: mimeFromBytes(buf) })); }
          catch { return null; }
        }),
      );
      if (!results.some((b) => b !== null)) {
        frameDelaysRef.current = [];
        bitmapsRef.current = [];
        return;
      }
      bitmapsRef.current = results;
      resizeCanvas();
      if (!resizeObsRef.current && animCanvasRef.current) {
        resizeObsRef.current = new ResizeObserver(() => { resizeCanvas(); drawCurrentFrame(); });
        resizeObsRef.current.observe(animCanvasRef.current);
      }
      drawCurrentFrame();
      scheduleNextFrame();
    } finally { setAnimLoading(false); framesInFlightRef.current = false; }
  }, [id, isAnimated, pageCount, resizeCanvas, drawCurrentFrame, scheduleNextFrame]);

  // ── Prefetch ────────────────────────────────────────────────────────
  const prefetchPages = useCallback(async () => {
    if (!id || pageCount == null || isAnimated) return;
    const lo = Math.max(0, current - PREFETCH_SPAN);
    const hi = Math.min(pageCount - 1, current + PREFETCH_SPAN);
    for (const keyStr of Object.keys(blobsRef.current)) {
      const k = Number(keyStr);
      if (Number.isNaN(k) || k < lo || k > hi) {
        URL.revokeObjectURL(blobsRef.current[k]);
        delete blobsRef.current[k];
      }
    }
    const targets: number[] = [];
    for (let p = lo; p <= hi; p++) {
      if (blobsRef.current[p] || prefetchInFlight.current.has(p)) continue;
      targets.push(p);
      prefetchInFlight.current.add(p);
    }
    await Promise.all(targets.map(async (p) => {
      try {
        const buf = await api.getBookPage(id, p);
        const mime = mimeFromBytes(buf);
        pageExtRef.current[p] = mime.split('/')[1] ?? 'jpg';
        blobsRef.current[p] = URL.createObjectURL(new Blob([buf], { type: mime }));
      } catch { /* ignore */ }
      finally { prefetchInFlight.current.delete(p); }
    }));
  }, [id, pageCount, current, isAnimated]);

  // ── Reading time ────────────────────────────────────────────────────
  const reportReadTime = useCallback((bookId: string) => {
    if (readSessionIdRef.current === null) {
      if (!warnedNoSessionRef.current) {
        warnedNoSessionRef.current = true;
        console.warn(`[Reader] recordReading skipped — no session_id for ${bookId}`);
      }
      return;
    }
    warnedNoSessionRef.current = false;
    const delta = Math.max(0, Math.round((readAccRef.current - readSessionBaselineRef.current) * 1000));
    void api.recordReading(bookId, delta).catch(() => {});
  }, []);

  const startReadTime = useCallback((bookId: string) => {
    if (!bookId || readStartRef.current != null) return;
    readAccRef.current = loadReadTime(bookId);
    readBookIdRef.current = bookId;
    readStartRef.current = Date.now();
    readLastTickRef.current = readStartRef.current;
    setReadTimeDisplay(fmtReadTime(readAccRef.current));
    if (readSessionIdRef.current === null) {
      readSessionBaselineRef.current = readAccRef.current;
      void api.openBook(bookId).then((sid) => { readSessionIdRef.current = sid; }).catch(() => {});
    }
  }, []);

  const stopReadTime = useCallback(() => {
    readStartRef.current = null;
    readLastTickRef.current = null;
  }, []);

  const ensureReadTimeTimer = useCallback(() => {
    if (readTimerRef.current != null) return;
    readTimerRef.current = setInterval(() => {
      if (readStartRef.current != null && readLastTickRef.current != null && readBookIdRef.current) {
        const now = Date.now();
        const delta = (now - readLastTickRef.current) / 1000;
        readLastTickRef.current = now;
        if (delta > 0 && delta <= TICK_CAP_SECONDS) {
          readAccRef.current += delta;
        }
        setReadTimeDisplay(fmtReadTime(readAccRef.current));
        saveReadTime(readBookIdRef.current, readAccRef.current);
        reportReadTime(readBookIdRef.current);
      }
    }, 1000);
  }, [reportReadTime]);

  // ── UI auto-hide ────────────────────────────────────────────────────
  const clearHideTimer = useCallback(() => {
    if (hideTimerRef.current) { clearTimeout(hideTimerRef.current); hideTimerRef.current = null; }
  }, []);

  const scheduleHide = useCallback(() => {
    clearHideTimer();
    hideTimerRef.current = setTimeout(() => { setUiHidden(true); }, 2000);
  }, [clearHideTimer]);

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    const inZone = e.clientY < 64 || e.clientY > window.innerHeight - 56;
    if (inZone) { setUiHidden(false); clearHideTimer(); }
    else if (wasInZoneRef.current) { scheduleHide(); }
    wasInZoneRef.current = inZone;
  }, [clearHideTimer, scheduleHide]);

  const onMouseLeave = useCallback(() => {
    wasInZoneRef.current = false;
    setUiHidden(true);
    clearHideTimer();
  }, [clearHideTimer]);

  // ── Context menu ────────────────────────────────────────────────────
  const onContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setMenuPos({ x: e.clientX, y: e.clientY });
    setMenuOpen(true);
  }, []);

  const onSetAsTheme = useCallback(async () => {
    setMenuOpen(false);
    const srcUrl = blobsRef.current[current];
    if (!srcUrl) return;
    const img = new Image();
    img.crossOrigin = 'anonymous';
    const colorP = new Promise<number>((resolve, reject) => {
      img.onload = () => { sourceColorFromImage(img).then(resolve).catch(reject); };
      img.onerror = () => reject(new Error('load failed'));
    });
    img.src = srcUrl;
    try {
      const argb = await colorP;
      const hex = hexFromArgb(argb);
      const MAX_BG = 1920;
      const fullCanvas = document.createElement('canvas');
      const fs = Math.min(MAX_BG / img.naturalWidth, MAX_BG / img.naturalHeight, 1);
      fullCanvas.width = Math.round(img.naturalWidth * fs);
      fullCanvas.height = Math.round(img.naturalHeight * fs);
      fullCanvas.getContext('2d')!.drawImage(img, 0, 0, fullCanvas.width, fullCanvas.height);
      const imageB64 = fullCanvas.toDataURL('image/jpeg', 0.7);
      const THUMB = 100;
      const thumbCanvas = document.createElement('canvas');
      const ts = Math.min(THUMB / img.naturalWidth, THUMB / img.naturalHeight, 1);
      thumbCanvas.width = Math.round(img.naturalWidth * ts);
      thumbCanvas.height = Math.round(img.naturalHeight * ts);
      thumbCanvas.getContext('2d')!.drawImage(img, 0, 0, thumbCanvas.width, thumbCanvas.height);
      const thumbnailB64 = thumbCanvas.toDataURL('image/jpeg', 0.6);
      themeStore.addCustomTheme(hex, imageB64, thumbnailB64, current, id!, bookMetaRef.current?.title ?? '');
      themeStore.setMode('dark');
      prevModeRef.current = 'dark';
      prevSeedRef.current = themeStore.seed;
      toast.addToast('success', t('reader.menu.themeApplied'));
    } catch { /* ignore */ }
  }, [current, id, themeStore, toast, t]);

  const onSaveImage = useCallback(async () => {
    setMenuOpen(false);
    if (isAnimated || !id) return;
    const titleStr = (bookMetaRef.current?.title || 'page').replace(/[/\\?%*:|"<>]/g, '_');
    const tw = String(pageCount ?? 1).length;
    const pn = String(current + 1).padStart(Math.max(1, tw), '0');
    const ext = pageExtRef.current[current] ?? 'jpg';
    const dest = await dialogSave({
      defaultPath: `${titleStr}_p${pn}.${ext}`,
      filters: [{ name: `Image (.${ext})`, extensions: [ext] }, { name: t('lib.save'), extensions: ['*'] }],
    });
    if (dest) {
      try { await api.saveBookPage(id, current, dest); toast.addToast('success', t('reader.menu.imageSaved')); }
      catch { toast.addToast('error', t('reader.menu.imageSaveFailed')); }
    }
  }, [isAnimated, id, pageCount, current, toast, t]);

  const onDeletePage = useCallback(async () => {
    setMenuOpen(false);
    if (isAnimated || pageCount == null || !id) return;
    const dropped = current;
    try {
      const newCount = await api.deletePage(id, dropped);
      const nextIdx = Math.min(dropped, Math.max(0, newCount - 1));
      const carriedOver = nextIdx !== dropped;
      let replacement: string | null = null;
      if (!carriedOver) {
        try {
          const buf = await api.getBookPage(id, nextIdx);
          const mime = mimeFromBytes(buf);
          pageExtRef.current[nextIdx] = mime.split('/')[1] ?? 'jpg';
          replacement = URL.createObjectURL(new Blob([buf], { type: mime }));
        } catch { /* ignore */ }
      }
      setPageCount(newCount);
      const stale = blobsRef.current;
      if (replacement) blobsRef.current = { [nextIdx]: replacement };
      else { const kept = stale[nextIdx]; blobsRef.current = kept ? { [nextIdx]: kept } : {}; }
      for (const url of Object.values(stale)) { if (!Object.values(blobsRef.current).includes(url)) URL.revokeObjectURL(url); }
      setCurrent(nextIdx);
      saveProgress(id, nextIdx);
      if (dropped === 0) void deleteThumb(id);
      toast.addToast('success', t('reader.menu.pageDeleted'));
    } catch { toast.addToast('error', t('reader.menu.pageDeleteFailed')); }
  }, [isAnimated, pageCount, current, id, toast, t]);

  // ── Keyboard ────────────────────────────────────────────────────────
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (e.key === 'ArrowRight' || e.key === 'PageDown' || e.key === ' ') { e.preventDefault(); go(1); }
      else if (e.key === 'ArrowLeft' || e.key === 'PageUp') { e.preventDefault(); go(-1); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [go]);

  // ── Trackpad swipe ──────────────────────────────────────────────────
  useEffect(() => {
    let accX = 0;
    const onWheel = (e: WheelEvent) => {
      if (isAnimated) return;
      if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) { accX = 0; return; }
      if (Math.abs(e.deltaX) < 1) return;
      e.preventDefault();
      accX += e.deltaX;
      if (Math.abs(accX) >= SWIPE_THRESHOLD) {
        go(accX > 0 ? -1 : 1);
        accX = 0;
      }
    };
    window.addEventListener('wheel', onWheel, { passive: false });
    return () => window.removeEventListener('wheel', onWheel);
  }, [go, isAnimated]);

  // ── Click zones ─────────────────────────────────────────────────────
  const onViewportClick = useCallback((e: React.MouseEvent) => {
    if (isAnimated) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const zone = x / rect.width;
    if (zone < 0.33) go(-1);
    else if (zone > 0.66) go(1);
  }, [go, isAnimated]);

  // ── Zoom ────────────────────────────────────────────────────────────
  const toggleZoom = useCallback(() => {
    setZoomMode((prev) => { const next = prev === 'fill' ? 'contain' : 'fill'; saveZoom(next); return next; });
  }, []);

  // ── Load metadata ───────────────────────────────────────────────────
  useEffect(() => {
    if (!id) return;
    void (async () => {
      try {
        const [book, count] = await Promise.all([api.getBook(id), api.getBookPageCount(id)]);
        bookMetaRef.current = book;
        setTitle(book.title);
        setPageCount(count);
        try { frameDelaysRef.current = book.delays ? JSON.parse(book.delays) as number[] : []; } catch { frameDelaysRef.current = []; }
        setIsAnimated(frameDelaysRef.current.length > 1);
        const start = frameDelaysRef.current.length > 1 ? 0 : Math.min(readProgress(id), Math.max(0, count - 1));
        setCurrent(start);
        setLoading(false);
      } catch { setLoading(false); }
    })();
    return () => {
      for (const url of Object.values(blobsRef.current)) URL.revokeObjectURL(url);
      blobsRef.current = {};
    };
  }, [id]);

  // ── Load current page ───────────────────────────────────────────────
  useEffect(() => {
    if (!id || pageCount == null) return;
    if (blobsRef.current[current]) { setSrc(blobsRef.current[current]); return; }
    void (async () => {
      try {
        const buf = await api.getBookPage(id, current);
        const mime = mimeFromBytes(buf);
        pageExtRef.current[current] = mime.split('/')[1] ?? 'jpg';
        const url = URL.createObjectURL(new Blob([buf], { type: mime }));
        blobsRef.current[current] = url;
        setSrc(url);
      } catch { /* ignore */ }
    })();
  }, [id, current, pageCount]);

  // ── Prefetch / preload frames ───────────────────────────────────────
  useEffect(() => {
    if (pageCount == null) return;
    if (isAnimated) {
      if (bitmapsRef.current.length === 0) void preloadFrames();
    } else {
      void prefetchPages();
    }
  }, [current, pageCount, isAnimated, preloadFrames, prefetchPages]);

  // ── Save progress ───────────────────────────────────────────────────
  useEffect(() => {
    if (id && pageCount != null && !isAnimated) saveProgress(id, current);
  }, [current, id, pageCount, isAnimated]);

  // ── Redraw on zoom ──────────────────────────────────────────────────
  useEffect(() => {
    if (isAnimated) drawCurrentFrame();
  }, [zoomMode, drawCurrentFrame, isAnimated]);

  // ── Force dark mode ─────────────────────────────────────────────────
  useEffect(() => {
    prevModeRef.current = themeStore.mode;
    prevSeedRef.current = themeStore.seed;
    themeStore.setMode('dark');
    return () => {
      themeStore.setSeed(prevSeedRef.current);
      themeStore.setMode(prevModeRef.current);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Reading time lifecycle ──────────────────────────────────────────
  useEffect(() => {
    if (!id) return;
    const onVis = () => { if (document.hidden) stopReadTime(); else startReadTime(id); };
    document.addEventListener('visibilitychange', onVis);
    ensureReadTimeTimer();
    if (!document.hidden) startReadTime(id);
    return () => {
      document.removeEventListener('visibilitychange', onVis);
      if (readTimerRef.current) { clearInterval(readTimerRef.current); readTimerRef.current = null; }
      if (readSessionIdRef.current !== null && readBookIdRef.current) {
        stopReadTime();
        reportReadTime(readBookIdRef.current);
      }
    };
  }, [id, startReadTime, stopReadTime, ensureReadTimeTimer, reportReadTime]);

  // ── UI auto-hide init ───────────────────────────────────────────────
  useEffect(() => {
    scheduleHide();
    return () => clearHideTimer();
  }, [scheduleHide, clearHideTimer]);

  // ── Render ──────────────────────────────────────────────────────────
  return (
    <div
      className={`reader fill-height${uiHidden ? ' reader--ui-hidden' : ''}`}
      onMouseMove={onMouseMove}
      onMouseLeave={onMouseLeave}
    >
      <header className="reader-topbar">
        <button className="icon-btn" title={t('reader.back')} aria-label={t('reader.back')} onClick={goBack}>
          <MdiIcon path={mdiArrowLeft} size={22} />
        </button>
        <div className="reader-topbar__title truncate">{title || t('reader.untitled')}</div>
        <span className="spacer" />
        <div className="reader-actions">
          <button className="icon-btn" title={zoomMode === 'fill' ? t('reader.fitScreen') : t('reader.fitContent')} onClick={toggleZoom}>
            <MdiIcon path={zoomMode === 'fill' ? mdiImageSizeSelectActual : mdiImageSizeSelectLarge} size={22} />
          </button>
        </div>
      </header>

      <div className="reader-viewport" onClick={onViewportClick} onContextMenu={onContextMenu}>
        {isAnimated ? (
          <>
            <canvas ref={animCanvasRef} className="reader-image reader-image--anim" style={{ display: animLoading ? 'none' : 'block' }} />
            {animLoading && (
              <div className="d-flex flex-column align-center justify-center ga-3">
                <svg className="spinner" style={{ color: 'var(--md-sys-color-primary)' }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg>
              </div>
            )}
          </>
        ) : src ? (
          <img key={current} src={src} alt={t('reader.page', { page: current + 1 })} className={`reader-image${zoomMode === 'fill' ? ' reader-image--fill' : ''}`} draggable={false} />
        ) : loading ? (
          <div className="d-flex flex-column align-center justify-center ga-3">
            <svg className="spinner" style={{ color: 'var(--md-sys-color-primary)' }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg>
            <span className="text-body-2">{t('reader.loadingPage', { page: current + 1, total: pageCount ?? '?' })}</span>
          </div>
        ) : null}
      </div>

      {!isAnimated && (
        <div className="reader-footer d-flex align-center ga-3 px-4 py-2">
          <span className="reader-page-label text-body-2">{current + 1}</span>
          <input
            type="range" className="flex-grow-1" style={{ accentColor: 'var(--md-sys-color-primary)' }}
            min={0} max={pageCount != null ? Math.max(0, pageCount - 1) : 0} step={1}
            value={current} onChange={(e) => goTo(Number(e.target.value))}
          />
          <span className="reader-page-label text-body-2">{pageCount ?? '?'}</span>
        </div>
      )}

      {/* Context menu */}
      {menuOpen && (
        <>
          <div style={{ position: 'fixed', inset: 0, zIndex: 9998 }} onClick={() => setMenuOpen(false)} onContextMenu={(e) => { e.preventDefault(); setMenuOpen(false); }} />
          <div style={{
            position: 'fixed', left: menuPos.x, top: menuPos.y, zIndex: 9999,
            background: 'var(--md-sys-color-surface-container-high)',
            borderRadius: 'var(--md-sys-shape-corner-small)', boxShadow: 'var(--md-sys-elevation-level3)',
            minWidth: 180, padding: '4px 0',
          }}>
            <button className="icon-btn" onClick={onSetAsTheme} style={{ display: 'flex', alignItems: 'center', gap: 12, width: '100%', padding: '10px 16px', border: 'none', background: 'transparent', color: 'var(--md-sys-color-on-surface)', cursor: 'pointer', fontSize: 14, borderRadius: 0 }}>
              <MdiIcon path={mdiPalette} size={18} /> {t('reader.menu.setAsTheme')}
            </button>
            {!isAnimated && (
              <button className="icon-btn" onClick={onSaveImage} style={{ display: 'flex', alignItems: 'center', gap: 12, width: '100%', padding: '10px 16px', border: 'none', background: 'transparent', color: 'var(--md-sys-color-on-surface)', cursor: 'pointer', fontSize: 14, borderRadius: 0 }}>
                <MdiIcon path={mdiContentSave} size={18} /> {t('reader.menu.saveImage')}
              </button>
            )}
            {!isAnimated && (
              <button className="icon-btn" onClick={onDeletePage} style={{ display: 'flex', alignItems: 'center', gap: 12, width: '100%', padding: '10px 16px', border: 'none', background: 'transparent', color: 'var(--md-sys-color-error)', cursor: 'pointer', fontSize: 14, borderRadius: 0 }}>
                <MdiIcon path={mdiDelete} size={18} /> {t('reader.menu.deletePage')}
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}