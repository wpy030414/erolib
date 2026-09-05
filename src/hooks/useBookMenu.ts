import { useState, useRef, useCallback } from 'react';

export type MdMenuElement = HTMLElement & { show: () => void; close: () => void; open: boolean };

export function useBookMenu() {
  const [menuOpen, setMenuOpen] = useState<Record<string, boolean>>({});
  const menuRefs = useRef<Map<string, MdMenuElement | null>>(new Map());
  const [pickerBookId, setPickerBookId] = useState<string | null>(null);

  const setMenuRef = useCallback((bookId: string, el: MdMenuElement | null) => {
    menuRefs.current.set(bookId, el);
  }, []);

  const openMenu = useCallback((bookId: string) => {
    setMenuOpen((prev) => ({ ...prev, [bookId]: true }));
    const el = menuRefs.current.get(bookId);
    if (el) el.show();
  }, []);

  const closeMenu = useCallback((bookId: string) => {
    setMenuOpen((prev) => ({ ...prev, [bookId]: false }));
  }, []);

  const openCollectionPicker = useCallback((bookId: string) => {
    closeMenu(bookId);
    setPickerBookId(bookId);
  }, [closeMenu]);

  const cleanupBook = useCallback((bookId: string) => {
    menuRefs.current.delete(bookId);
    setMenuOpen((prev) => {
      const next = { ...prev };
      delete next[bookId];
      return next;
    });
  }, []);

  const clearAll = useCallback(() => {
    menuRefs.current.clear();
    setMenuOpen({});
  }, []);

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