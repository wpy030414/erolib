<template>
  <div
    class="reader fill-height"
    :class="{ 'reader--ui-hidden': uiHidden }"
    @mousemove="onMouseMove"
    @mouseleave="uiHidden = true"
  >
    <header class="reader-topbar">
      <button
        class="icon-btn"
        :title="t('reader.back')"
        :aria-label="t('reader.back')"
        @click="goBack"
      >
        <svg :width="22" :height="22" viewBox="0 0 24 24" fill="currentColor">
          <path :d="mdiArrowLeft" />
        </svg>
      </button>

      <div class="reader-topbar__title text-title-medium truncate">
        {{ title || t('reader.untitled') }}
      </div>

      <span class="spacer" />

      <div class="reader-actions">
        <button
          class="icon-btn"
          :title="zoomMode === 'fill' ? t('reader.fitScreen') : t('reader.fitContent')"
          :aria-label="zoomMode === 'fill' ? t('reader.fitScreen') : t('reader.fitContent')"
          @click="toggleZoom"
        >
          <svg :width="22" :height="22" viewBox="0 0 24 24" fill="currentColor">
            <path :d="zoomMode === 'fill' ? mdiImageSizeSelectActual : mdiImageSizeSelectLarge" />
          </svg>
        </button>
      </div>
    </header>

    <div
      class="reader-viewport"
      @click="onViewportClick"
    >
      <template v-if="src">
        <img
          :key="current"
          :src="src"
          :alt="t('reader.page', { page: current + 1 })"
          class="reader-image"
          :class="{ 'reader-image--fill': zoomMode === 'fill' }"
          draggable="false"
        />
      </template>
      <template v-else>
        <div class="d-flex flex-column align-center justify-center ga-3">
          <md-circular-progress indeterminate />
          <span class="text-body-2">
            {{ t('reader.loadingPage', { page: current + 1, total: pageCount ?? '?' }) }}
          </span>
        </div>
      </template>
    </div>

    <div class="reader-footer d-flex align-center ga-3 px-4 py-2">
      <span class="reader-page-label text-body-2">{{ current + 1 }}</span>

      <md-slider
        v-if="pageCount != null"
        ref="sliderRef"
        class="flex-grow-1"
        :min="0"
        :max="Math.max(0, pageCount - 1)"
        :step="1"
        ticks
      />

      <span class="reader-page-label text-body-2">{{ pageCount ?? '?' }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  mdiArrowLeft,
  mdiImageSizeSelectLarge,
  mdiImageSizeSelectActual,
} from '@mdi/js';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import { applyMd3Theme } from '@/services/md3-theme';
import { useThemeStore } from '@/stores/theme';
import type { Seed } from '@/services/md3-theme';

type ZoomMode = 'fill' | 'contain';

const props = defineProps<{
  id: string;
}>();

const { t } = useI18n();
const router = useRouter();

/** Return to wherever the reader was opened from (e.g. the Pixiv grid); fall
 *  back to the library if there's no history. */
function goBack() {
  if (window.history.length > 1) router.back();
  else router.push('/library');
}

const pageCount = ref<number | null>(null);
const title = ref('');
const blobs = ref<Record<number, string>>({});
const current = ref(0);
const zoomMode = ref<ZoomMode>(readZoomMode());
const uiHidden = ref(false);
let uiHideTimer: ReturnType<typeof setTimeout> | null = null;

watch(zoomMode, (v) => saveZoomMode(v));
watch(current, (v) => saveBookProgress(props.id, v));

const src = computed(() => blobs.value[current.value]);

function readZoomMode(): ZoomMode {
  try {
    const saved = window.localStorage.getItem('erolib.reader.zoomMode');
    if (saved === 'fill' || saved === 'contain') return saved;
  } catch {
    // ignore
  }
  return 'contain';
}

function saveZoomMode(mode: ZoomMode) {
  try {
    window.localStorage.setItem('erolib.reader.zoomMode', mode);
  } catch {
    // ignore
  }
}

function readBookProgress(bookId: string): number {
  try {
    const raw = window.localStorage.getItem(`erolib.reader.progress.${bookId}`);
    const parsed = raw ? Number(raw) : 0;
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : 0;
  } catch {
    return 0;
  }
}

function saveBookProgress(bookId: string, page: number) {
  try {
    window.localStorage.setItem(`erolib.reader.progress.${bookId}`, String(page));
  } catch {
    // ignore
  }
}

/** Guess a mime type from magic bytes so blob URLs render all stored formats. */
function mimeFromBytes(bytes: number[]): string {
  const b = bytes.slice(0, 12);
  if (b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return 'image/jpeg';
  if (b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47) return 'image/png';
  if (
    b[0] === 0x52 &&
    b[1] === 0x49 &&
    b[2] === 0x46 &&
    b[3] === 0x46 &&
    b[8] === 0x57 &&
    b[9] === 0x45 &&
    b[10] === 0x42 &&
    b[11] === 0x50
  )
    return 'image/webp';
  // ugoira books are stored as a single gif; the <img> loops it natively.
  if (b[0] === 0x47 && b[1] === 0x49 && b[2] === 0x46) return 'image/gif';
  return 'image/jpeg';
}

function setCurrent(value: number) {
  if (pageCount.value == null) return;
  current.value = Math.max(0, Math.min(pageCount.value - 1, value));
}

function go(delta: number) {
  setCurrent(current.value + delta);
}

function onViewportClick(e: MouseEvent) {
  const target = e.currentTarget as HTMLElement;
  const rect = target.getBoundingClientRect();
  const x = e.clientX - rect.left;
  if (x > rect.width / 2) go(1);
  else go(-1);
}

function onKeyDown(e: KeyboardEvent) {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
  if (e.key === 'ArrowRight' || e.key === 'PageDown' || e.key === ' ') {
    e.preventDefault();
    go(1);
  } else if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
    e.preventDefault();
    go(-1);
  }
}

function clearBlobs() {
  Object.values(blobs.value).forEach(URL.revokeObjectURL);
  blobs.value = {};
}

async function loadMetadata() {
  if (!props.id) return;
  try {
    const [book, count] = await Promise.all([
      api.getBook(props.id),
      api.getBookPageCount(props.id),
    ]);
    title.value = book.title;
    pageCount.value = count;
    const saved = readBookProgress(props.id);
    current.value = Math.min(saved, Math.max(0, count - 1));
  } catch (e) {
    console.error('Failed to load book:', e);
  }
}

async function prefetchPages() {
  if (!props.id || pageCount.value == null) return;
  const span = 2;
  const pages: number[] = [];
  for (let p = current.value; p <= Math.min(pageCount.value - 1, current.value + span); p++) {
    if (!blobs.value[p]) pages.push(p);
  }

  for (const p of pages) {
    try {
      const bytes = await api.getBookPage(props.id, p);
      const blob = new Blob([new Uint8Array(bytes)], { type: mimeFromBytes(bytes) });
      const url = URL.createObjectURL(blob);
      blobs.value[p] = url;
    } catch (e) {
      console.warn(`Failed to load page ${p}:`, e);
    }
  }
}

function toggleZoom() {
  zoomMode.value = zoomMode.value === 'fill' ? 'contain' : 'fill';
}

function onMouseMove() {
  uiHidden.value = false;
  if (uiHideTimer) clearTimeout(uiHideTimer);
  uiHideTimer = setTimeout(() => {
    uiHidden.value = true;
  }, 2000);
}

/** Force dark theme while Reader is mounted, restore on leave. */
const previousMode = ref<'light' | 'dark' | null>(null);
const previousSeed = ref<Seed | null>(null);
const themeStore = useThemeStore();

onMounted(() => {
  loadMetadata();
  window.addEventListener('keydown', onKeyDown);
  previousMode.value = themeStore.mode;
  previousSeed.value = themeStore.seed;
  applyMd3Theme(themeStore.seed, 'dark');
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown);
  clearBlobs();
  if (uiHideTimer) clearTimeout(uiHideTimer);
  if (previousMode.value && previousSeed.value) {
    applyMd3Theme(previousSeed.value, previousMode.value);
  }
});

watch(() => props.id, () => {
  current.value = 0;
  clearBlobs();
  loadMetadata();
});

watch([current, pageCount], () => {
  prefetchPages();
});

// Sync MWC slider with current page.
type MdSlider = HTMLElement & { value: number };
const sliderRef = ref<MdSlider | null>(null);
function syncSliderValue() {
  if (sliderRef.value) {
    sliderRef.value.value = current.value;
  }
}

function onSliderInput() {
  if (sliderRef.value) {
    const next = Number(sliderRef.value.value);
    if (!Number.isNaN(next)) {
      setCurrent(next);
    }
  }
}

function bindSlider() {
  if (!sliderRef.value) return;
  syncSliderValue();
  sliderRef.value.addEventListener('input', onSliderInput);
  sliderRef.value.addEventListener('change', onSliderInput);
}

function unbindSlider() {
  sliderRef.value?.removeEventListener('input', onSliderInput);
  sliderRef.value?.removeEventListener('change', onSliderInput);
}

watch(current, syncSliderValue);
watch(sliderRef, (el, prev) => {
  if (prev) unbindSlider();
  if (el) bindSlider();
});
onMounted(() => {
  bindSlider();
});
onBeforeUnmount(() => {
  unbindSlider();
});
</script>

<style scoped>
.reader {
  position: relative;
  overflow: hidden;
  background: var(--md-sys-color-surface);
}

.reader-topbar,
.reader-footer {
  position: absolute;
  left: 0;
  right: 0;
  z-index: 10;
  transition:
    opacity 0.25s ease,
    transform 0.25s ease;
}

.reader-topbar {
  top: 0;
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 64px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: color-mix(in srgb, var(--md-sys-color-surface) 85%, transparent);
  backdrop-filter: blur(8px);
}

.reader-footer {
  bottom: 0;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  border-top: 1px solid var(--md-sys-color-outline-variant);
  background: color-mix(in srgb, var(--md-sys-color-surface) 85%, transparent);
  backdrop-filter: blur(8px);
}

.reader--ui-hidden .reader-topbar,
.reader--ui-hidden .reader-footer {
  opacity: 0;
  pointer-events: none;
}

.reader--ui-hidden .reader-topbar {
  transform: translateY(-100%);
}

.reader--ui-hidden .reader-footer {
  transform: translateY(100%);
}

.reader-topbar__title {
  flex: 0 1 auto;
  min-width: 0;
  color: var(--md-sys-color-on-surface);
  font: var(--md-sys-typescale-title-medium-weight)
    var(--md-sys-typescale-title-medium-size) /
    var(--md-sys-typescale-title-medium-line-height)
    var(--md-sys-typescale-font);
}

.reader-actions {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 4px;
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  padding: 0;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.icon-btn:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}

.reader-page-label {
  min-width: 28px;
  text-align: center;
  font-variant-numeric: tabular-nums;
  color: var(--md-sys-color-on-surface);
}

.reader-viewport {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: auto;
  background: var(--md-sys-color-surface);
  cursor: pointer;
}

.reader-image {
  display: block;
  user-select: none;
}

/* 贴合内容 (contain, default): image fills the viewport box, then
   object-fit:contain scales it down to fit entirely (letterbox). Crucially we
   size the element to 100% (not max-width alone) so LOW-RES images also scale
   up to the screen instead of sitting at their tiny natural size centered in a
   sea of margin. */
.reader-image:not(.reader-image--fill) {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

/* 贴合屏幕 (fill/cover): absolutely cover the viewport — 放大填满、裁剪、无
   留白。用 absolute 把图拽出 viewport 的 flex 居中流，配显式 top/left + 100%
   尺寸，确保 object-fit:cover 把低分辨率图也放大到完全贴边、不留 margin。 */
.reader-image--fill {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  object-fit: cover;
}

.truncate {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
</style>
