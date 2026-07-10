<template>
  <div class="toast-container">
    <TransitionGroup name="toast">
      <div
        v-for="msg in toasts"
        :key="msg.id"
        class="toast-item"
        :class="'toast--' + msg.kind"
        @click="dismiss(msg.id)"
      >
        <span class="toast-icon">
          <svg v-if="msg.kind === 'success'" viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
          </svg>
          <svg v-else-if="msg.kind === 'error'" viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M12 2C6.47 2 2 6.47 2 12s4.47 10 10 10 10-4.47 10-10S17.53 2 12 2zm5 13.59L15.59 17 12 13.41 8.41 17 7 15.59 10.59 12 7 8.41 8.41 7 12 10.59 15.59 7 17 8.41 13.41 12 17 15.59z"/>
          </svg>
          <svg v-else viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
          </svg>
        </span>
        <span class="toast-message">{{ msg.message }}</span>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { useToastStore } from '@/stores/toast';

const { toasts, dismiss } = useToastStore();
</script>

<style scoped>
.toast-container {
  position: fixed;
  top: 16px;
  right: 16px;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
  max-width: 400px;
}

.toast-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--md-sys-color-inverse-surface, #1e1e1e);
  color: var(--md-sys-color-inverse-on-surface, #e3e3e3);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  pointer-events: auto;
  cursor: pointer;
  font-size: 14px;
  line-height: 1.4;
  transition: transform 0.2s ease, opacity 0.2s ease;
}

.toast--success {
  border-left: 4px solid #4caf50;
}

.toast--error {
  border-left: 4px solid #f44336;
}

.toast--info {
  border-left: 4px solid #2196f3;
}

.toast-icon {
  display: flex;
  flex-shrink: 0;
}

.toast-message {
  flex: 1;
  min-width: 0;
  word-break: break-word;
}

.toast-enter-active {
  transition: all 0.3s ease;
}

.toast-leave-active {
  transition: all 0.2s ease;
}

.toast-enter-from {
  opacity: 0;
  transform: translateX(40px);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(40px);
}
</style>
