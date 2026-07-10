<template>
  <div id="erolib-app" class="erolib-app d-flex fill-height">
    <AppShell v-if="!isReader" />
    <main ref="appMainRef" class="app-main flex-grow-1">
      <router-view />
    </main>
    <AppToast />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick, watch } from 'vue';
import { useRoute } from 'vue-router';
import AppShell from './components/AppShell.vue';
import AppToast from './components/AppToast.vue';

const route = useRoute();
const isReader = computed(() => route.path.startsWith('/reader'));

// Scroll container — every secondary view scrolls inside this <main>.
const appMainRef = ref<HTMLElement | null>(null);

const isScrollPersistable = (path: string) => !path.startsWith('/reader');
const scrollKey = (path: string) => `erolib.scroll.${path}`;

// --- Save scrollTop (rAF-throttled so we don't write on every scroll event) ---
let saveScheduled = false;
function scheduleSaveScroll() {
  if (saveScheduled) return;
  saveScheduled = true;
  requestAnimationFrame(() => {
    saveScheduled = false;
    const el = appMainRef.value;
    const path = route.path;
    if (!el || !isScrollPersistable(path)) return;
    try {
      localStorage.setItem(scrollKey(path), String(el.scrollTop));
    } catch {
      // ignore quota / privacy-mode errors
    }
  });
}

// --- Restore scrollTop, retrying a few frames until content is tall enough ---
const MAX_RESTORE_FRAMES = 30;
function restoreScroll(path: string) {
  const el = appMainRef.value;
  if (!el || !isScrollPersistable(path)) return;

  let target: number;
  try {
    const raw = localStorage.getItem(scrollKey(path));
    if (raw == null) return;
    target = Number(raw);
  } catch {
    return;
  }
  if (!Number.isFinite(target) || target <= 0) return;

  let attempt = 0;
  const trySet = () => {
    if (el.scrollHeight >= target) {
      el.scrollTop = target;
      return;
    }
    attempt += 1;
    if (attempt < MAX_RESTORE_FRAMES) {
      requestAnimationFrame(trySet);
    } else {
      // Gave up waiting for async content; land as far down as possible.
      el.scrollTop = target;
    }
  };
  requestAnimationFrame(trySet);
}

function onMainScroll() {
  scheduleSaveScroll();
}

// On navigation: save the old path, then restore the new one after layout.
watch(
  () => route.path,
  (path, oldPath) => {
    const el = appMainRef.value;
    if (el && oldPath && isScrollPersistable(oldPath)) {
      try {
        localStorage.setItem(scrollKey(oldPath), String(el.scrollTop));
      } catch {
        // ignore
      }
    }
    // nextTick + rAF: wait for the view swap + layout to settle.
    nextTick(() => {
      requestAnimationFrame(() => restoreScroll(path));
    });
  },
);

onMounted(() => {
  const el = appMainRef.value;
  if (el) {
    el.addEventListener('scroll', onMainScroll, { passive: true });
  }
  // Restore the initial view's scroll after first layout.
  nextTick(() => {
    requestAnimationFrame(() => restoreScroll(route.path));
  });
});

onBeforeUnmount(() => {
  const el = appMainRef.value;
  if (el) {
    el.removeEventListener('scroll', onMainScroll);
  }
});
</script>

<style>
.erolib-app {
  min-height: 100vh;
  background: var(--md-sys-color-surface);
}

.app-main {
  display: flex;
  flex-direction: column;
  overflow: auto;
}
</style>
