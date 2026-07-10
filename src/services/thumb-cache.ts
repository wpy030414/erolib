import { openDB, type IDBPDatabase } from 'idb';

// IndexedDB cache for low-res cover thumbnails. The library grid reads these
// directly (instant, no IPC); on a miss the caller fetches a thumb from the
// backend and stores it here. Best-effort — every op swallows errors so a
// broken/closed DB never breaks the UI.

const DB_NAME = 'erolib';
const DB_VERSION = 1;
const STORE = 'thumbs';

let dbPromise: Promise<IDBPDatabase> | null = null;

function db(): Promise<IDBPDatabase> {
  if (!dbPromise) {
    dbPromise = openDB(DB_NAME, DB_VERSION, {
      upgrade(db) {
        if (!db.objectStoreNames.contains(STORE)) {
          db.createObjectStore(STORE);
        }
      },
    });
  }
  return dbPromise;
}

/** Fetch a cached thumbnail blob, or undefined on miss / error. */
export async function getThumb(bookId: string): Promise<Blob | undefined> {
  try {
    return await (await db()).get(STORE, bookId);
  } catch {
    return undefined;
  }
}

/** Store a thumbnail blob keyed by book id. */
export async function setThumb(bookId: string, blob: Blob): Promise<void> {
  try {
    await (await db()).put(STORE, blob, bookId);
  } catch {
    // ignore — caching is best-effort
  }
}

/** Drop a cached thumbnail (e.g. when the book is deleted or re-downloaded). */
export async function deleteThumb(bookId: string): Promise<void> {
  try {
    await (await db()).delete(STORE, bookId);
  } catch {
    // ignore
  }
}
