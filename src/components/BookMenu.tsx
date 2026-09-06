import { MenuSurface, menuPlacementForAnchor, type MenuSurfaceItem as BookMenuItem } from '@/components/MenuSurface';

export type { BookMenuItem };

interface BookMenuProps {
  /** DOM id of the card element this menu is anchored to. */
  anchorId: string;
  open: boolean;
  onClose: () => void;
  items: BookMenuItem[];
}

/** Card-anchored context menu (portaled to body), mirroring the Vue md-menu
 *  with `positioning="fixed"` + `anchor`: opens below the anchor (flips above
 *  near the viewport bottom) with the md-menu open/close animation, closes on
 *  Escape / click / right-click outside, and supports arrow-key navigation. */
export function BookMenu({ anchorId, open, onClose, items }: BookMenuProps) {
  const { x, y, origin } = menuPlacementForAnchor(anchorId, items.length);
  return (
    <MenuSurface open={open} onClose={onClose} x={x} y={y} transformOrigin={origin} items={items} />
  );
}
