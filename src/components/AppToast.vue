<template>
  <div class="toast-container">
    <TransitionGroup name="toast">
      <div
        v-for="msg in toasts"
        :key="msg.id"
        class="toast"
        role="status"
        @click="dismiss(msg.id)"
      >
        <MdiIcon :path="iconFor(msg.kind)" :size="18" class="toast-icon" />
        <span class="toast-message">{{ msg.message }}</span>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { useToastStore } from '@/stores/toast';
import MdiIcon from '@/components/MdiIcon.vue';
import {
  mdiCheckCircleOutline,
  mdiAlertCircleOutline,
  mdiInformationOutline,
} from '@mdi/js';

const { toasts, dismiss } = useToastStore();

/** MD3 Snackbar keeps a single neutral `inverse-surface` look — the message
 *  kind is conveyed by the leading icon's shape, not by colour or a side bar. */
function iconFor(kind: 'success' | 'error' | 'info') {
  if (kind === 'success') return mdiCheckCircleOutline;
  if (kind === 'error') return mdiAlertCircleOutline;
  return mdiInformationOutline;
}
</script>

<style scoped>
/* MD3 Snackbar: anchored bottom-centre (clears the nav rail / FAB), stacked
 * newest-at-the-bottom. The container never eats pointer events; each toast
 * re-enables them so a click can dismiss. */
.toast-container {
  position: fixed;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 9999;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  pointer-events: none;
  width: min(640px, calc(100vw - 32px));
}

.toast {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  /* MD3 plain snackbar: inverse-surface fill, no border, no elevation. The
   * 4dp/8dp corner + 48dp min-height match the single-line spec. */
  min-height: 48px;
  padding: 8px 16px;
  box-sizing: border-box;
  border-radius: var(--md-sys-shape-corner-small, 8px);
  background: var(--md-sys-color-inverse-surface);
  color: var(--md-sys-color-inverse-on-surface);
  cursor: pointer;
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
}

.toast-icon {
  flex-shrink: 0;
  /* `inverse-primary` is the M3 emphasis colour on an inverse surface (the
   * same role the action label plays on a snackbar). */
  color: var(--md-sys-color-inverse-primary);
}

.toast-message {
  flex: 1;
  min-width: 0;
  word-break: break-word;
}

/* MD3 motion: fade + slide up from below (snackbar standard), not from the
 * side. Leaving toasts stay in flow so siblings don't jump. */
.toast-enter-active {
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}

.toast-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(16px);
}
</style>
