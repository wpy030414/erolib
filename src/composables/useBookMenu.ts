import { reactive, ref } from 'vue';

/** md-menu element shape — @material/web menu exposes show/close + open flag. */
export type MdMenuElement = HTMLElement & {
  show: () => void;
  close: () => void;
  open: boolean;
};

/**
 * Shared right-click menu state for book cards (used by Home and Library).
 * Manages the per-book open flag, element refs for calling show(), and the
 * collection-picker trigger. The template stays in each view (the menu items
 * are identical) but all the plumbing lives here.
 */
export function useBookMenu() {
  const menuOpen = reactive<Record<string, boolean>>({});
  const menuRefs = new Map<string, MdMenuElement | null>();
  const pickerBookId = ref<string | null>(null);

  function setMenuRef(bookId: string, el: MdMenuElement | null) {
    if (el) {
      menuRefs.set(bookId, el);
    } else {
      menuRefs.delete(bookId);
    }
  }

  function openMenu(bookId: string) {
    menuOpen[bookId] = true;
    const menuEl = menuRefs.get(bookId);
    if (menuEl && typeof menuEl.show === 'function') {
      menuEl.show();
    }
  }

  function closeMenu(bookId: string) {
    menuOpen[bookId] = false;
  }

  function openCollectionPicker(bookId: string) {
    menuOpen[bookId] = false;
    pickerBookId.value = bookId;
  }

  /** Clean up refs when a book is removed from the list. */
  function cleanupBook(bookId: string) {
    delete menuOpen[bookId];
    menuRefs.delete(bookId);
  }

  function clearAll() {
    menuRefs.clear();
  }

  return {
    menuOpen,
    menuRefs,
    pickerBookId,
    setMenuRef,
    openMenu,
    closeMenu,
    openCollectionPicker,
    cleanupBook,
    clearAll,
  };
}
