import { useEffect, useRef, useState } from 'react';
import { useI18n } from '@/hooks/useI18n';
import { useNavigate } from 'react-router-dom';
import { api } from '@/services/api';
import { getThumb, setThumb, deleteThumb } from '@/services/thumb-cache';
import { useToastStore } from '@/stores/toast';
import { useBookMenu } from '@/hooks/useBookMenu';
import { MdiIcon } from '@/components/MdiIcon';
import { SourceCard } from '@/components/SourceCard';
import { WallCover } from '@/components/WallCover';
import { BookMenu } from '@/components/BookMenu';
import {
  mdiContentSave, mdiDelete, mdiInformationOutline, mdiPlaylistPlus,
} from '@mdi/js';
import type { Book } from '@/types';

import { BookCollectionPicker } from '@/components/BookCollectionPicker';
import { BookMetaDialog, type BookMetaDialogHandle } from '@/components/BookMetaDialog';
import { BookExportDialog, type BookExportDialogHandle } from '@/components/BookExportDialog';

const WALL_SLOTS = 21;

function hashU32(s: string): number { let h = 2166136261 >>> 0; for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; }

export default function Home() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const toast = useToastStore();
  const { openBookId, pickerBookId, openMenu, closeMenu, openCollectionPicker } = useBookMenu();
  const metaDialogRef = useRef<BookMetaDialogHandle>(null);
  const exportDialogRef = useRef<BookExportDialogHandle>(null);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [totalMs, setTotalMs] = useState(0);
  const [recent, setRecent] = useState<Book[]>([]);
  const [library, setLibrary] = useState<Book[]>([]);
  const [coverMap, setCoverMap] = useState<Record<string, string | null>>({});
  const pendingCovers = useRef(new Set<string>());
  const disposalsRef = useRef<Array<() => void>>([]);

  const totalMinutes = totalMs / 60000;
  const reading = totalMinutes >= 60
    ? { value: (totalMinutes / 60).toFixed(1), unit: 'hour' as const }
    : totalMs <= 0 ? { value: '0', unit: 'minute' as const } : { value: String(Math.max(1, Math.round(totalMinutes))), unit: 'minute' as const };

  const shuffledLibrary = [...library].sort((a, b) => hashU32(a.id) - hashU32(b.id));
  const wallBooks = shuffledLibrary.slice(0, WALL_SLOTS);

  async function loadCover(book: Book): Promise<void> {
    if (book.id in coverMap || pendingCovers.current.has(book.id)) return;
    pendingCovers.current.add(book.id);
    setCoverMap((prev) => ({ ...prev, [book.id]: null }));
    let url: string | null = null;
    try {
      const key = book.source_post_id || book.id;
      let blob = await getThumb(key);
      if (!blob) { const bytes = await api.getBookCoverThumb(book.id); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(key, blob); }
      url = URL.createObjectURL(blob);
      setCoverMap((prev) => ({ ...prev, [book.id]: url }));
    } catch { /* leave null */ }
    finally {
      pendingCovers.current.delete(book.id);
      disposalsRef.current.push(() => { if (url) URL.revokeObjectURL(url); });
    }
  }

  useEffect(() => {
    void (async () => {
      try {
        const [ms, rec, lib] = await Promise.all([api.getWeeklyReadingMs(), api.listRecentBooks(12), api.listBooks()]);
        setTotalMs(ms); setRecent(rec); setLibrary(lib);
        // Load covers concurrently using the fresh lists (same wall sampling
        // as the render: hash sort → first 21).
        const shuffled = [...lib].sort((a, b) => hashU32(a.id) - hashU32(b.id));
        await Promise.all([...rec, ...shuffled.slice(0, WALL_SLOTS)].map((b) => loadCover(b)));
      } catch (e) { setError(t('common.error', { message: String(e) })); }
      finally { setLoading(false); }
    })();
    return () => { disposalsRef.current.forEach((d) => d()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function deleteBookItem(book: Book) {
    closeMenu();
    try {
      await api.deleteBook(book.id); void deleteThumb(book.id);
      setCoverMap((prev) => { const next = { ...prev }; delete next[book.id]; return next; });
      const fresh = await api.listRecentBooks(12);
      const oldIds = new Set(recent.map((b) => b.id));
      const newcomers = fresh.filter((b) => !oldIds.has(b.id));
      setRecent(fresh);
      await Promise.all(newcomers.map((b) => loadCover(b)));
      toast.addToast('success', t('lib.deleted', { title: book.title }));
    } catch (e) { toast.addToast('error', t('lib.deleteFailed', { error: String(e) })); }
  }

  // Header stays rendered through loading / error so the page chrome never
  // flashes away (Vue template keeps it outside the v-if/v-else-if chain).
  const header = (
    <div className="home-header d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
      <h2 className="text-h5 home-header__title" style={{ margin: 0 }}>{t('nav.home')}</h2>
      <span className="spacer" />
    </div>
  );

  if (loading) return (
    <div className="pa-6">
      {header}
      <div className="home-loading"><svg className="spinner" style={{ color: 'var(--md-sys-color-primary)' }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg></div>
    </div>
  );
  if (error) return <div className="pa-6">{header}<div className="error-state"><p className="error-state__msg">{error}</p></div></div>;

  return (
    <div className="pa-6">
      {header}
      <section className="hero">
        <div className="hero__text">
          <div className="hero__icon" aria-hidden="true">⏱</div>
          <div className="hero__body">
            <div className="hero__label">{t('home.weekly')}</div>
            <h1 className="hero__value">{reading.value}{t(`home.unit.${reading.unit}`)}</h1>
          </div>
        </div>
        {wallBooks.length > 0 && <div className="hero__wall"><WallCover books={wallBooks} coverMap={coverMap} /></div>}
      </section>
      <section className="home-section mb-6">
        <h3 className="text-h6 home-section__title" style={{ margin: '0 0 12px' }}>{t('home.recently')}</h3>
        {recent.length > 0 ? (
          <div className="md3-grid">
            {recent.map((book) => (
              <div key={book.id}>
                <SourceCard id={`home-recent-${book.id}`} title={book.title} pageCount={book.page_count} subtitle={book.author} cover={coverMap[book.id] ?? null}
                  onClick={() => navigate(`/reader/${book.id}`)} onContextMenu={(e) => { e.preventDefault(); openMenu(book.id); }} />
                <BookMenu
                  anchorId={`home-recent-${book.id}`}
                  open={openBookId === book.id}
                  onClose={closeMenu}
                  items={[
                    { icon: mdiPlaylistPlus, label: t('lib.collections.addTo'), action: () => openCollectionPicker(book.id) },
                    { icon: mdiInformationOutline, label: t('lib.viewMeta'), action: () => metaDialogRef.current?.open(book) },
                    { icon: mdiContentSave, label: t('lib.save'), action: () => exportDialogRef.current?.open(book) },
                    { icon: mdiDelete, label: t('lib.delete'), action: () => void deleteBookItem(book) },
                  ]}
                />
              </div>
            ))}
          </div>
        ) : <div className="text-body-2 text-medium-emphasis home-empty" style={{ padding: '8px 0' }}>{t('home.noData')}</div>}
      </section>
      {pickerBookId && <BookCollectionPicker bookId={pickerBookId} onClose={() => openCollectionPicker('')} />}
      <BookMetaDialog ref={metaDialogRef} />
      <BookExportDialog ref={exportDialogRef} />
    </div>
  );
}
