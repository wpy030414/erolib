import { useEffect, useRef, useState } from 'react';
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
import { BookMenu } from '@/components/BookMenu';
import {
  mdiContentSave,
  mdiDelete,
  mdiFolderOpen,
  mdiInformationOutline,
  mdiPlaylistPlay,
  mdiPlaylistPlus,
} from '@mdi/js';
import { BookMetaDialog, type BookMetaDialogHandle } from '@/components/BookMetaDialog';
import { BookExportDialog, type BookExportDialogHandle } from '@/components/BookExportDialog';
import { BookCollectionPicker } from '@/components/BookCollectionPicker';
import { CollectionDialog } from '@/components/CollectionDialog';
import { M3eButton } from '@m3e/react/button';
import type { Book } from '@/types';

const TAG_DISPLAY_LIMIT = 30;

export default function Library() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { t } = useI18n();
  const libraryStore = useLibraryStore();
  const collectionsStore = useCollectionsStore();
  const toast = useToastStore();
  const { openBookId, pickerBookId, openMenu, closeMenu, openCollectionPicker } = useBookMenu();
  const metaDialogRef = useRef<BookMetaDialogHandle>(null);
  const exportDialogRef = useRef<BookExportDialogHandle>(null);
  const [coverMap, setCoverMap] = useState<Record<string, string | null>>({});
  const coverMapRef = useRef(coverMap);
  coverMapRef.current = coverMap;
  const [showCollectionDialog, setShowCollectionDialog] = useState(false);
  const pendingCovers = useRef(new Set<string>());
  const title = collectionsStore.isAllActive ? t('nav.library') : `"${collectionsStore.activeCollectionName}"`;

  const sentinelRef = useInfiniteSentinel(() => libraryStore.loadMore(), {
    feedState: { loading: libraryStore.isLoading || libraryStore.isLoadingMore, end: !libraryStore.hasMore },
  });

  async function loadCover(book: Book) {
    if (book.id in coverMap || pendingCovers.current.has(book.id)) return;
    pendingCovers.current.add(book.id);
    setCoverMap((prev) => ({ ...prev, [book.id]: null }));
    try {
      const cacheKey = book.source_post_id || book.id;
      let blob = await getThumb(cacheKey);
      if (!blob) { const bytes = await api.getBookCoverThumb(book.id); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(cacheKey, blob); }
      const made = URL.createObjectURL(blob);
      setCoverMap((prev) => ({ ...prev, [book.id]: made }));
    } catch { /* leave placeholder */ }
    finally { pendingCovers.current.delete(book.id); }
  }

  // Revoke covers for books that left the grid, load covers for new ones.
  const prevIds = useRef<Set<string>>(new Set());
  useEffect(() => {
    const currentIds = new Set(libraryStore.books.map((b) => b.id));
    for (const id of prevIds.current) {
      if (!currentIds.has(id)) {
        const url = coverMapRef.current[id];
        if (url) URL.revokeObjectURL(url);
        setCoverMap((prev) => { const n = { ...prev }; delete n[id]; return n; });
      }
    }
    prevIds.current = currentIds;
    for (const book of libraryStore.books) void loadCover(book);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [libraryStore.books]);

  useEffect(() => { libraryStore.ensureLoaded(); collectionsStore.ensureLoaded(); }, []);

  // When the active collection changes, re-filter the library and re-tally
  // tags. Skips the initial mount (ensureLoaded already covers it) — the Vue
  // watch had no `immediate`.
  const firstCollectionRun = useRef(true);
  const activeCollectionId = collectionsStore.activeCollectionId;
  useEffect(() => {
    if (firstCollectionRun.current) { firstCollectionRun.current = false; return; }
    useLibraryStore.setState({ collectionFilter: useCollectionsStore.getState().activeCollectionName });
    libraryStore.applySearch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeCollectionId]);

  // Navigated here (e.g. from Tasks "view") with ?search=…: set the text box
  // and trigger a search. Runs on mount too (the Vue watch was `immediate`).
  const search = searchParams.get('search') ?? '';
  useEffect(() => {
    if (!search) return;
    // Always sync collectionFilter first so a prior setActiveCollection(null)
    // from another view takes effect.
    useLibraryStore.setState({ collectionFilter: useCollectionsStore.getState().activeCollectionName });
    if (search === useLibraryStore.getState().query) { libraryStore.applySearch(); return; }
    useLibraryStore.setState({ query: search });
    libraryStore.applySearch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search]);

  // Revoke every cover URL on unmount (read through a ref — the cleanup
  // closure would otherwise capture the initial empty map).
  useEffect(() => () => {
    for (const url of Object.values(coverMapRef.current)) if (url) URL.revokeObjectURL(url);
  }, []);

  async function onImport() {
    const file = await api.openFile([{ name: t('lib.import.filterName'), extensions: ['cb7', 'cbz', 'cbr', 'epub', 'pdf'] }]);
    if (typeof file === 'string') {
      try { const book = await api.importBook(file); await libraryStore.refresh(); toast.addToast('success', t('lib.imported', { title: (book as any)?.title ?? '' })); }
      catch (e) { toast.addToast('error', t('lib.importFailed', { error: String(e) })); }
    }
  }

  async function deleteBookItem(book: Book) {
    try {
      await libraryStore.deleteBook(book.id);
      void deleteThumb(book.id);
      toast.addToast('success', t('lib.deleted', { title: book.title }));
    } catch (e) {
      toast.addToast('error', t('lib.deleteFailed', { error: String(e) }));
    }
  }

  return (
    <div className="pa-6">
      <div className="library-header d-flex align-center gap-4 mb-6" style={{ minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0, whiteSpace: 'nowrap' }}>{title}</h2>
        <span className="spacer" />
        <SearchBox value={libraryStore.query} placeholder={t('lib.search.placeholder')} clearLabel={t('common.clear')} onChange={(v) => useLibraryStore.setState({ query: v })} onCommit={() => libraryStore.applySearch()} />
        <M3eButton variant="filled" onClick={onImport}><MdiIcon path={mdiFolderOpen} size={20} /> {t('lib.import')}</M3eButton>
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
              <BookMenu
                anchorId={`book-anchor-${book.id}`}
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
      ) : !libraryStore.isLoading ? <div className="text-center text-medium-emphasis mt-8">{t('lib.empty')}</div> : null}

      {libraryStore.books.length > 0 && (
        <div className="feed-sentinel-wrap">
          <div ref={sentinelRef} className="feed-sentinel" />
          {libraryStore.isLoadingMore && <div className="feed-loading"><svg className="spinner" style={{ color: 'var(--md-sys-color-primary)', width: 24, height: 24 }} viewBox="0 0 50 50"><circle className="spinner-track" cx="25" cy="25" r="20" /><circle className="spinner-arc" cx="25" cy="25" r="20" /></svg></div>}
        </div>
      )}

      <FabButton icon={mdiPlaylistPlay} ariaLabel={t('lib.collections.manage')} onClick={() => setShowCollectionDialog(true)} />
      {/* 抽屉常驻挂载，open 驱动滑入/滑出（Vue 语义） */}
      <CollectionDialog open={showCollectionDialog} onClose={() => setShowCollectionDialog(false)} />
      {pickerBookId && <BookCollectionPicker bookId={pickerBookId} onClose={() => openCollectionPicker('')} />}
      <BookMetaDialog ref={metaDialogRef} />
      <BookExportDialog ref={exportDialogRef} />
    </div>
  );
}
