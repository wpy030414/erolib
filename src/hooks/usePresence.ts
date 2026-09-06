import { useEffect, useState } from 'react';

/**
 * Vue `<Transition>` 语义的挂载/离场状态机。
 *
 * - `open` 变 true：立即挂载。进场动效由 CSS animation 播放（挂载即播）。
 * - `open` 变 false：先保持挂载，让元素播放 CSS 离场过渡（调用方按 `open`
 *   推导离场类），`durationMs`（须与 CSS 离场时长一致）后再真正卸载。
 */
export function usePresence(open: boolean, durationMs: number): boolean {
  const [mounted, setMounted] = useState(open);

  useEffect(() => {
    if (open) {
      setMounted(true);
      return;
    }
    if (!mounted) return;
    const timer = window.setTimeout(() => setMounted(false), durationMs);
    return () => window.clearTimeout(timer);
    // `mounted` 只在超时后翻转，不参与依赖（避免重复调度）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, durationMs]);

  return mounted;
}
