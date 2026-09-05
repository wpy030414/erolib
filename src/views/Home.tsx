import { useEffect, useState, useRef } from 'react';
import { useI18n } from '@/hooks/useI18n';
import { useNavigate } from 'react-router-dom';
import { api } from '@/services/api';
import { getThumb, setThumb, deleteThumb } from '@/services/thumb-cache';
import { useToastStore } from '@/stores/toast';
import { useBookMenu } from '@/hooks/useBookMenu';
import { MdiIcon } from '@/components/MdiIcon';
import { SourceCard } from '@/components/SourceCard';
import { WallCover } from '@/components/WallCover';
import { BookMetaDialog, type BookMetaDialogHandle } from '@/components/BookMetaDialog';
import { BookExportDialog, type BookExportDialogHandle } from '@/components/BookExportDialog';
import { BookCollectionPicker } from '@/components/BookCollectionPicker';
import {
  mdiContentSave, mdiDelete, mdiInformationOutline, mdiPlaylistPlus,
} from '@mdi/js';
import type { Book } from '@/types';

export default function Home() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const toast = useToastStore();
  const { menuOpen, setMenuRef, openMenu, openCollectionPicker, pickerBookId, clearAll } = useBookMenu();
  const metaDialog = useRef<BookMetaDialogHandle>(null);
  const exportDialog = useRef<BookExportDialogHandle>(null);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [totalMs, setTotalMs] = useState(0);
  const [recent, setRecent] = useState<Book[]>([]);
  const [library, setLibrary] = useState<Book[]>([]);
  const [coverMap, setCoverMap] = useState<Record<string, string | null>>({});

  // Reading time display
  const totalMinutes = totalMs / 60000;
  const reading = totalMinutes >= 60
    ? { value: (totalMinutes / 60).toFixed(1), unit: 'hour' }
    : totalMs <= 0 ? { value: '0', unit: 'minute' } : { value: String(Math.max(1, Math.round(totalMinutes))), unit: 'minute' };

  // Wall books
  const WALL_SLOTS = 21;
  const shuffledLibrary = [...library].sort((a, b) => hashU32(a.id) - hashU32(b.id));
  const wallBooks = shuffledLibrary.slice(0, WALL_SLOTS);

  function hashU32(s: string): number { let h = 2166136261 >>> 0; for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; }

  async function loadCover(book: Book): Promise<(() => void) | undefined> {
    if (book.id in coverMap) return;
    setCoverMap((prev) => ({ ...prev, [book.id]: null }));
    try {
      const key = book.source_post_id || book.id;
      let blob = await getThumb(key);
      if (!blob) { const bytes = await api.getBookCoverThumb(book.id); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(key, blob); }
      const url = URL.createObjectURL(blob);
      setCoverMap((prev) => ({ ...prev, [book.id]: url }));
      return () => { URL.revokeObjectURL(url); };
    } catch { /* leave null */ }
  }

  useEffect(() => {
    void (async () => {
      try {
        const [ms, rec, lib] = await Promise.all([api.getWeeklyReadingMs(), api.listRecentBooks(12), api.listBooks()]);
        setTotalMs(ms); setRecent(rec); setLibrary(lib);
        for (const b of rec) await loadCover(b);
        for (const b of wallBooks) await loadCover(b);
      } catch (e) { setError(t('common.error', { message: String(e) })); }
      finally { setLoading(false); }
    })();
    return () => clearAll();
  }, []);

  // Book actions
  async function deleteBookItem(book: Book) {
    menuOpen[book.id] = false;
    try {
      await api.deleteBook(book.id);
      void deleteThumb(book.id);
      setCoverMap((prev) => { const next = { ...prev }; delete next[book.id]; return next; });
      const fresh = await api.listRecentBooks(12);
      const oldIds = new Set(recent.map((b) => b.id));
      const newcomers = fresh.filter((b) => !oldIds.has(b.id));
      setRecent(fresh);
      for (const b of newcomers) await loadCover(b);
      toast.addToast('success', t('lib.deleted', { title: book.title }));
    } catch (e) { toast.addToast('error', t('lib.deleteFailed', { error: String(e) })); }
  }

  if (loading) return <div className="home-loading"><div className="spinner" /></div>;
  if (error) return <div className="error-state"><p className="error-state__msg">{error}</p></div>;

  return (
    <div className="pa-6">
      <div className="home-header d-flex align-center gap-4 mb-6">
        <h2 className="text-h5 home-header__title">{t('nav.home')}</h2>
        <span className="spacer" />
      </div>

      {/* Hero card */}
      <section className="hero">
        <div className="hero__text">
          <div className="hero__icon" aria-hidden="true">⏱</div>
          <div className="hero__body">
            <div className="hero__label">{t('home.weekly')}</div>
            <h1 className="hero__value">{reading.value}{t('home.unit.' + reading.unit)}</h1>
          </div>
        </div>
        {wallBooks.length > 0 && <div className="hero__wall"><WallCover books={wallBooks} coverMap={coverMap} /></div>}
      </section>

      {/* Recent books */}
      <section className="home-section mb-6">
        <h3 className="text-h6 home-section__title">{t('home.recently')}</h3>
        {recent.length > 0 ? (
          <div className="md3-grid">
            {recent.map((book) => (
              <div key={book.id}>
                <SourceCard
                  id={`home-recent-${book.id}`}
                  title={book.title}
                  pageCount={book.page_count}
                  subtitle={book.author}
                  cover={coverMap[book.id] ?? null}
                  onClick={() => navigate(`/reader/${book.id}`)}
                  onContextMenu={(e) => { e.preventDefault(); openMenu(book.id); }}
                />
                {/* Right-click menu - simplified inline */}
                {menuOpen[book.id] && (
                  <div style={{ position: 'fixed', zIndex: 1000, background: 'var(--md-sys-color-surface-container)', borderRadius: 'var(--md-sys-shape-corner-medium)', boxShadow: 'var(--md-sys-elevation-level3)', padding: '8px 0', minWidth: 180 }}>
                    {[
                      { icon: mdiPlaylistPlus, label: t('lib.collections.addTo'), action: () => openCollectionPicker(book.id) },
                      { icon: mdiInformationOutline, label: t('lib.viewMeta'), action: () => { menuOpen[book.id] = false; metaDialog.current?.open(book); } },
                      { icon: mdiContentSave, label: t('lib.save'), action: () => { menuOpen[book.id] = false; exportDialog.current?.open(book); } },
                      { icon: mdiDelete, label: t('lib.delete'), action: () => deleteBookItem(book) },
                    ].map((item, i) => (
                      <div key={i} onClick={item.action} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 16px', cursor: 'pointer', fontSize: 14 }} onMouseOver={(e) => (e.currentTarget.style.background = 'var(--md-sys-color-surface-container-highest)')} onMouseOut={(e) => (e.currentTarget.style.background = 'transparent')}>
                        <MdiIcon path={item.icon} size={18} /><span>{item.label}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        ) : <div className="text-body-2 text-medium-emphasis home-empty">{t('home.noData')}</div>}
      </section>

      <BookCollectionPicker bookId={pickerBookId ?? ''} onClose={() => {}} />
      <BookMetaDialog ref={metaDialog} />
      <BookExportDialog ref={exportDialog} />
    </div>
  );
}