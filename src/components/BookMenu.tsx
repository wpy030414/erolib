import { useEffect, useState } from 'react';
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
 *  Escape / click / right-click outside, supports arrow-key navigation, and
 *  repositions on window resize while open. */
export function BookMenu({ anchorId, open, onClose, items }: BookMenuProps) {
  // md-menu 在窗口 resize 时自动重新定位——定位在渲染期从锚点推导，
  // resize 时强制重渲染即可跟随锚点的新位置。
  const [, resizeTick] = useState(0);
  useEffect(() => {
    if (!open) return;
    const onResize = () => resizeTick((n) => n + 1);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [open]);
  const { x, y, origin } = menuPlacementForAnchor(anchorId, items.length);
  return (
    <MenuSurface open={open} onClose={onClose} x={x} y={y} transformOrigin={origin} anchorId={anchorId} items={items} />
  );
}
