import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { MdiIcon } from '@/components/MdiIcon';
import { usePresence } from '@/hooks/usePresence';

export interface MenuSurfaceItem {
  icon: string;
  label: string;
  /** 危险操作（删除等）——文字用 error 色。 */
  danger?: boolean;
  action: () => void;
}

interface MenuSurfaceProps {
  open: boolean;
  onClose: () => void;
  /** 菜单左上角固定定位坐标（锚点推导或右键光标位置）。 */
  x: number;
  y: number;
  /** 进场缩放基点，md-menu 从锚点角生长。 */
  transformOrigin?: string;
  minWidth?: number;
  items: MenuSurfaceItem[];
}

/** md-menu 语义对齐的弹出菜单：固定定位 + 内置进出场动效（进场 0.15s 淡入
 *  放大、离场 0.1s 淡出缩小），Esc / 外部点击 / 右键关闭，方向键在项间移动
 *  焦点，项 hover 走 state-layer 渐变。 */
export function MenuSurface({ open, onClose, x, y, transformOrigin = 'top left', minWidth = 200, items }: MenuSurfaceProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  // 离场动画期间保持挂载（与 CSS 离场时长一致）。
  const mounted = usePresence(open, 100);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!mounted) return null;

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
        className={`context-menu ${open ? 'context-menu--enter' : 'context-menu--exit'}`}
        style={{ left: x, top: y, minWidth, transformOrigin }}
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
            className="menu-item"
            style={item.danger ? { color: 'var(--md-sys-color-error)' } : undefined}
            onClick={() => { onClose(); item.action(); }}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onClose(); item.action(); } }}
          >
            <MdiIcon path={item.icon} size={18} /><span>{item.label}</span>
          </div>
        ))}
      </div>
    </>,
    document.body,
  );
}

/** 锚点推导菜单位置：默认锚点下方 4px，接近视口底部时翻转到上方并改缩放基点。 */
export function menuPlacementForAnchor(anchorId: string, itemCount: number): { x: number; y: number; origin: string } {
  const rect = document.getElementById(anchorId)?.getBoundingClientRect();
  // Placement estimate: one row ≈ 40px + 16px vertical padding.
  const estH = itemCount * 40 + 16;
  let y = (rect?.bottom ?? 0) + 4;
  let origin = 'top left';
  if (rect && y + estH > window.innerHeight - 8) {
    y = Math.max(8, rect.top - estH - 4);
    origin = 'bottom left';
  }
  const x = rect ? Math.max(8, Math.min(rect.left, window.innerWidth - 208)) : 8;
  return { x, y, origin };
}
