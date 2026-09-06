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
  /** 锚点元素 id——关闭后焦点归还（md-menu restore focus 语义）。 */
  anchorId?: string;
  items: MenuSurfaceItem[];
}

/** md-menu 语义对齐的弹出菜单：固定定位 + 内置进出场动效（进场 0.15s 淡入
 *  放大、离场 0.1s 淡出缩小），Esc / 外部点击 / 右键关闭，方向键与 Home/End
 *  在项间移动焦点，项 hover 走 state-layer 渐变。 */
export function MenuSurface({ open, onClose, x, y, transformOrigin = 'top left', minWidth = 200, anchorId, items }: MenuSurfaceProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  // 离场动画期间保持挂载（与 CSS 离场时长一致）。
  const mounted = usePresence(open, 100);

  // md-menu 语义对齐：无全屏遮罩——打开期间用 document 捕获阶段监听外部
  // click / contextmenu 关闭，事件随后自然流向下方元素：右键另一张卡片时
  // 旧菜单先关、新菜单由卡片自身 handler 立即弹出（遮罩层会吞掉事件导致
  // 「开着菜单改右键另一张卡」弹不出新菜单）。菜单内部不受拦截。
  useEffect(() => {
    if (!open) return;
    const inside = (target: EventTarget | null) => menuRef.current?.contains(target as Node) ?? false;
    const onOutside = (e: MouseEvent) => { if (!inside(e.target)) onClose(); };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('click', onOutside, true);
    document.addEventListener('contextmenu', onOutside, true);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('click', onOutside, true);
      document.removeEventListener('contextmenu', onOutside, true);
      document.removeEventListener('keydown', onKey);
    };
  }, [open, onClose]);

  // 关闭后焦点归还锚点（md-menu restore focus）：仅当焦点仍留在菜单内或
  // 已随卸载落到 body 时归还，不抢用户刚点中的元素。
  useEffect(() => {
    if (open) return;
    const active = document.activeElement as HTMLElement | null;
    const menu = menuRef.current;
    if (!active || active === document.body || (menu && menu.contains(active))) {
      document.getElementById(anchorId ?? '')?.focus({ preventScroll: true });
    }
  }, [open, anchorId]);

  if (!mounted) return null;

  return createPortal(
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
        else if (e.key === 'Home') { e.preventDefault(); nodes[0]?.focus(); }
        else if (e.key === 'End') { e.preventDefault(); nodes[nodes.length - 1]?.focus(); }
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
    </div>,
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
