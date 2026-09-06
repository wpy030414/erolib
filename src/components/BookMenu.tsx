import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { MdiIcon } from '@/components/MdiIcon';

export interface BookMenuItem {
  icon: string;
  label: string;
  action: () => void;
}

interface BookMenuProps {
  /** DOM id of the card element this menu is anchored to. */
  anchorId: string;
  open: boolean;
  onClose: () => void;
  items: BookMenuItem[];
}

/** Card-anchored context menu (portaled to body), mirroring the Vue md-menu
 *  with `positioning="fixed"` + `anchor`: opens below the anchor (flips above
 *  near the viewport bottom), closes on Escape / click / right-click outside,
 *  and supports arrow-key navigation between items. */
export function BookMenu({ anchorId, open, onClose, items }: BookMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  const rect = document.getElementById(anchorId)?.getBoundingClientRect();
  // Placement estimate: one row ≈ 40px + 16px vertical padding.
  const estH = items.length * 40 + 16;
  let top = (rect?.bottom ?? 0) + 4;
  if (rect && top + estH > window.innerHeight - 8) top = Math.max(8, rect.top - estH - 4);
  const left = rect ? Math.max(8, Math.min(rect.left, window.innerWidth - 208)) : 8;

  return createPortal(
    <>
      <div
        style={{ position: 'fixed', inset: 0, zIndex: 999 }}
        onClick={onClose}
        onContextMenu={(e) => { e.preventDefault(); onClose(); }}
      />
      <div
        ref={menuRef}
        role="menu"
        tabIndex={-1}
        autoFocus
        style={{ position: 'fixed', left, top, zIndex: 1000, background: 'var(--md-sys-color-surface-container)', borderRadius: 'var(--md-sys-shape-corner-medium)', boxShadow: 'var(--md-sys-elevation-level3)', padding: '8px 0', minWidth: 200, outline: 'none' }}
        onKeyDown={(e) => {
          const nodes = Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
          const i = nodes.indexOf(document.activeElement as HTMLElement);
          if (e.key === 'ArrowDown') { e.preventDefault(); nodes[Math.min(i + 1, nodes.length - 1)]?.focus(); }
          else if (e.key === 'ArrowUp') { e.preventDefault(); nodes[Math.max(i - 1, 0)]?.focus(); }
        }}
      >
        {items.map((item) => (
          <div
            key={item.label}
            role="menuitem"
            tabIndex={-1}
            onClick={() => { onClose(); item.action(); }}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onClose(); item.action(); } }}
            style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 16px', cursor: 'pointer', fontSize: 14, color: 'var(--md-sys-color-on-surface)' }}
            onMouseOver={(e) => (e.currentTarget.style.background = 'var(--md-sys-color-surface-container-highest)')}
            onMouseOut={(e) => (e.currentTarget.style.background = 'transparent')}
          >
            <MdiIcon path={item.icon} size={18} /><span>{item.label}</span>
          </div>
        ))}
      </div>
    </>,
    document.body,
  );
}
