import { useEffect, useState, useRef } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { api } from '@/services/api';
import { getThumb, setThumb, deleteThumb } from '@/services/thumb-cache';
import { useI18n } from '@/hooks/useI18n';
import { useLibraryStore } from '@/stores/library';
import { useCollectionsStore } from '@/stores/collections';
import { useToastStore } from '@/stores/toast';
import { useBookMenu } from '@/hooks/useBookMenu';
import { useInfiniteSentinel } from '@/hooks/useInfiniteSentinel';
import { MdiIcon } from '@/components/MdiIcon';
import { SourceCard } from '@/components/SourceCard';
import { SearchBox } from '@/components/SearchBox';
import { FabButton } from '@/components/FabButton';
import { lazy, Suspense } from 'react';
import { mdiFolderOpen, mdiPlaylistPlay } from '@mdi/js';
import { BookMetaDialog, type BookMetaDialogHandle } from '@/components/BookMetaDialog';
import { BookExportDialog, type BookExportDialogHandle } from '@/components/BookExportDialog';
import type { Book } from '@/types';

const BookCollectionPicker = lazy(() => import('@/components/BookCollectionPicker'));
const CollectionDialog = lazy(() => import('@/components/CollectionDialog'));

const TAG_DISPLAY_LIMIT = 30;

export default function Library() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { t } = useI18n();
  const libraryStore = useLibraryStore();
  const collectionsStore = useCollectionsStore();
  const toast = useToastStore();
  const { menuOpen, pickerBookId, openMenu, openCollectionPicker, cleanupBook, clearAll } = useBookMenu();
  const metaDialogRef = useRef<BookMetaDialogHandle>(null);
  const exportDialogRef = useRef<BookExportDialogHandle>(null);
  const [coverMap, setCoverMap] = useState<Record<string, string | null>>({});
  const [showCollectionDialog, setShowCollectionDialog] = useState(false);
  const sentinelEl = useRef<HTMLDivElement>(null);
  const prevIds = useRef<Set<string>>(new Set());
  const title = collectionsStore.isAllActive ? t('nav.library') : `"${collectionsStore.activeCollectionName}"`;

  useInfiniteSentinel(sentinelEl, () => libraryStore.loadMore(), {
    feedState: { loading: libraryStore.isLoading || libraryStore.isLoadingMore, end: !libraryStore.hasMore },
  });

  async function loadCover(book: Book) {
    if (book.id in coverMap) return;
    setCoverMap((prev) => ({ ...prev, [book.id]: null }));
    let alive = true; let made: string | null = null;
    try {
      const cacheKey = book.source_post_id || book.id;
      let blob = await getThumb(cacheKey);
      if (!blob) { const bytes = await api.getBookCoverThumb(book.id); if (!alive) return; blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(cacheKey, blob); }
      if (!alive) return; made = URL.createObjectURL(blob);
      setCoverMap((prev) => ({ ...prev, [book.id]: made }));
    } catch { /* ignore */ }
  }

  useEffect(() => {
    const currentIds = new Set(libraryStore.books.map((b) => b.id));
    for (const id of prevIds.current) {
      if (!currentIds.has(id)) { const url = coverMap[id]; if (url) URL.revokeObjectURL(url); setCoverMap((prev) => { const n = { ...prev }; delete n[id]; return n; }); cleanupBook(id); }
    }
    prevIds.current = currentIds;
    for (const book of libraryStore.books) void loadCover(book);
  }, [libraryStore.books]);

  useEffect(() => { libraryStore.ensureLoaded(); collectionsStore.ensureLoaded(); }, []);
  useEffect(() => { libraryStore.collectionFilter = collectionsStore.activeCollectionName; libraryStore.applySearch(); }, [collectionsStore.activeCollectionId]);
  useEffect(() => {
    const text = searchParams.get('search') ?? '';
    if (!text) return;
    libraryStore.collectionFilter = collectionsStore.activeCollectionName;
    if (text === libraryStore.query) { libraryStore.applySearch(); return; }
    libraryStore.query = text; libraryStore.applySearch();
  }, [searchParams]);

  useEffect(() => () => { for (const url of Object.values(coverMap)) if (url) URL.revokeObjectURL(url); clearAll(); }, []);

  async function onImport() {
    const file = await api.openFile([{ name: t('lib.import.filterName'), extensions: ['cb7', 'cbz', 'cbr', 'epub', 'pdf'] }]);
    if (typeof file === 'string') {
      try { const book = await api.importBook(file); await libraryStore.refresh(); toast.addToast('success', t('lib.imported', { title: (book as any)?.title ?? '' })); }
      catch (e) { toast.addToast('error', t('lib.importFailed', { error: String(e) })); }
    }
  }

  return (
    <div className="pa-6">
      <div className="library-header d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{title}</h2>
        <span className="spacer" />
        <SearchBox value={libraryStore.query} placeholder={t('lib.search.placeholder')} clearLabel={t('common.clear')} onChange={() => {}} onCommit={() => libraryStore.applySearch()} />
        <button className="md3-btn md3-btn--filled" onClick={onImport}><MdiIcon path={mdiFolderOpen} size={20} /> {t('lib.import')}</button>
        {libraryStore.isLoading && <svg className="spinner" style={{ color: 'var(--md-sys-color-primary)', width: 24, height: 24 }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg>}
      </div>

      {libraryStore.allTags.length > 0 && (
        <div className="tag-chips mb-6">
          {libraryStore.allTags.map((tag) => (
            <button key={tag.name} className={`tag-chip${libraryStore.selectedTags.includes(tag.name) ? ' tag-chip--selected' : ''}`} aria-pressed={libraryStore.selectedTags.includes(tag.name)} onClick={() => libraryStore.toggleTag(tag.name)}>
              <span className="tag-chip__label">{tag.name}</span><span className="tag-chip__count">({tag.count})</span>
            </button>
          ))}
          {libraryStore.allTags.length >= TAG_DISPLAY_LIMIT && <span className="tag-chip tag-chip--ellipsis" aria-hidden="true">…</span>}
        </div>
      )}

      {libraryStore.books.length > 0 ? (
        <div className="md3-grid">
          {libraryStore.books.map((book) => (
            <div key={book.id}>
              <SourceCard id={`book-anchor-${book.id}`} title={book.title} pageCount={book.page_count} subtitle={book.author} cover={coverMap[book.id] ?? null}
                onClick={() => navigate(`/reader/${book.id}`)} onContextMenu={(e) => { e.preventDefault(); openMenu(book.id); }} />
            </div>
          ))}
        </div>
      ) : !libraryStore.isLoading ? <div className="text-center text-medium-emphasis mt-8">{t('lib.empty')}</div> : null}

      {libraryStore.books.length > 0 && (
        <div className="feed-sentinel-wrap">
          <div ref={sentinelEl} className="feed-sentinel" />
          {libraryStore.isLoadingMore && <div className="feed-loading"><svg className="spinner" style={{ color: 'var(--md-sys-color-primary)', width: 24, height: 24 }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg></div>}
        </div>
      )}

      <FabButton icon={mdiPlaylistPlay} ariaLabel={t('lib.collections.manage')} onClick={() => setShowCollectionDialog(true)} />
      {showCollectionDialog && <Suspense><CollectionDialog onClose={() => setShowCollectionDialog(false)} /></Suspense>}
      {pickerBookId && <Suspense><BookCollectionPicker bookId={pickerBookId} onClose={() => openCollectionPicker('')} /></Suspense>}
      <BookMetaDialog ref={metaDialogRef} />
      <BookExportDialog ref={exportDialogRef} />
    </div>
  );
}