import { useCallback, useState } from 'react';

/** Context-menu state shared by the Home and Library grids: which book's
 *  menu is open (one at a time) and which book's collection picker is
 *  showing. Menu items render through the shared <BookMenu> component. */
export function useBookMenu() {
  const [openBookId, setOpenBookId] = useState<string | null>(null);
  const [pickerBookId, setPickerBookId] = useState<string | null>(null);

  const openMenu = useCallback((bookId: string) => setOpenBookId(bookId), []);
  const closeMenu = useCallback(() => setOpenBookId(null), []);

  /** Open the picker for a book (closing the menu); an empty id closes the
   *  picker — views pass '' from the picker's onClose. */
  const openCollectionPicker = useCallback((bookId: string) => {
    setOpenBookId(null);
    setPickerBookId(bookId || null);
  }, []);

  return { openBookId, pickerBookId, openMenu, closeMenu, openCollectionPicker };
}
