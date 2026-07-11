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
      <template v-if="isAnimated">
        <canvas v-show="!animLoading" ref="animCanvas" class="reader-image reader-image--anim" />
        <div v-if="animLoading" class="d-flex flex-column align-center justify-center ga-3">
          <svg class="spinner" style="color: var(--md-sys-color-primary)" viewBox="0 0 50 50" aria-hidden="true">
            <circle class="spinner-track" cx="25" cy="25" r="20" />
            <circle class="spinner-arc" cx="25" cy="25" r="20" />
          </svg>
        </div>
      </template>
      <template v-else-if="src">
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

    <div v-if="!isAnimated" class="reader-footer d-flex align-center ga-3 px-4 py-2">
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
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
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

watch(zoomMode, (v) => {
  saveZoomMode(v);
  if (isAnimated.value) drawCurrentFrame();
});
watch(current, () => {
  if (isAnimated.value) {
    drawCurrentFrame();
    scheduleNextFrame();
  } else {
    saveBookProgress(props.id, current.value);
  }
});

const src = computed(() => blobs.value[current.value]);

// Ugoira (動図) books store the original jpg frames + per-frame delays (ms).
// The reader plays the sequence on a timer instead of treating each frame as
// a page — lossless, native resolution, each frame a tiny jpg.
const frameDelays = ref<number[]>([]);
const isAnimated = computed(() => frameDelays.value.length > 1);
const animLoading = ref(false);
let playTimer: ReturnType<typeof setTimeout> | null = null;

function clearPlayTimer() {
  if (playTimer) {
    clearTimeout(playTimer);
    playTimer = null;
  }
}

/** Schedule the next animated frame; a no-op for non-animated books. */
function scheduleNextFrame() {
  clearPlayTimer();
  if (!isAnimated.value || pageCount.value == null) return;
  const delay = frameDelays.value[current.value] ?? 100;
  playTimer = setTimeout(
    () => {
      const n = pageCount.value ?? 1;
      current.value = (current.value + 1) % n;
    },
    Math.max(16, delay),
  );
}

// Animated books render to a <canvas>: every frame is decoded up front to an
// ImageBitmap, then the timer just drawImage()'s the next bitmap. No <img>
// src-swap → no flicker, and playback is cheap (a single draw per frame).
const animCanvas = ref<HTMLCanvasElement | null>(null);
// Sparse by frame index: entry i is frame i's bitmap, or null if that frame
// failed to decode (drawCurrentFrame / scheduleNextFrame skip holes).
const bitmaps = ref<(ImageBitmap | null)[]>([]);
let resizeObserver: ResizeObserver | null = null;
// Guards preloadFrames against overlapping invocations (see comment inside).
let framesInFlight = false;

async function preloadFrames() {
  // `framesInFlight` blocks re-entry during the await: a second trigger (e.g.
  // ArrowRight while the spinner is still up) would otherwise re-fetch every
  // frame and overwrite `bitmaps` without closing the first batch.
  if (!props.id || !isAnimated.value || bitmaps.value.length > 0 || framesInFlight)
    return;
  const n = frameDelays.value.length || pageCount.value || 0;
  if (n === 0) return;
  framesInFlight = true;
  animLoading.value = true;
  try {
    // Fetch all frames concurrently. The big win for ugoira start-up latency is
    // parallel IPC + parallel zip unpacking on the backend (was a serial for
    // loop, one IPC round-trip per frame). Frames land in array order so frame
    // i still maps to bitmaps[i]; a per-frame failure leaves a null hole that
    // drawCurrentFrame/scheduleNextFrame already skip via their !bmp guards.
    const results = await Promise.all(
      Array.from({ length: n }, async (_, p): Promise<ImageBitmap | null> => {
        try {
          const buf = await api.getBookPage(props.id, p);
          const blob = new Blob([buf], { type: mimeFromArrayBuffer(buf) });
          return await createImageBitmap(blob);
        } catch (e) {
          console.warn(`Failed to load frame ${p}:`, e);
          return null;
        }
      }),
    );
    // If every frame failed, fall back to the static page path instead of
    // busy-spinning a null-only animation forever (isAnimated would stay true
    // and scheduleNextFrame would cycle through nulls at ~60fps).
    if (!results.some((b) => b !== null)) {
      console.warn('ugoira: all frames failed to load — falling back to static');
      frameDelays.value = [];
      bitmaps.value = [];
      return;
    }
    bitmaps.value = results;
    await nextTick();
    resizeCanvas();
    if (!resizeObserver && animCanvas.value) {
      resizeObserver = new ResizeObserver(() => {
        resizeCanvas();
        drawCurrentFrame();
      });
      resizeObserver.observe(animCanvas.value);
    }
    drawCurrentFrame();
    scheduleNextFrame();
  } finally {
    animLoading.value = false;
    framesInFlight = false;
  }
}

function resizeCanvas() {
  const canvas = animCanvas.value;
  if (!canvas) return;
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(rect.width * dpr));
  canvas.height = Math.max(1, Math.round(rect.height * dpr));
}

function drawCurrentFrame() {
  const canvas = animCanvas.value;
  const bmp = bitmaps.value[current.value];
  if (!canvas || !bmp) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const cw = canvas.width;
  const ch = canvas.height;
  ctx.clearRect(0, 0, cw, ch);
  const scale =
    zoomMode.value === 'fill'
      ? Math.max(cw / bmp.width, ch / bmp.height)
      : Math.min(cw / bmp.width, ch / bmp.height);
  const dw = bmp.width * scale;
  const dh = bmp.height * scale;
  ctx.drawImage(bmp, (cw - dw) / 2, (ch - dh) / 2, dw, dh);
}

function closeBitmaps() {
  bitmaps.value.forEach((b) => {
    if (b) b.close();
  });
  bitmaps.value = [];
}

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

/** Guess a mime type from an ArrayBuffer's leading magic bytes so blob URLs
 *  render all stored formats. Raw bytes now arrive as ArrayBuffer (Tauri raw
 *  IPC), so sniff a 12-byte Uint8Array view over the buffer without copying. */
function mimeFromArrayBuffer(buf: ArrayBuffer): string {
  const b = new Uint8Array(buf, 0, Math.min(12, buf.byteLength));
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
  if (isAnimated.value) return; // animated books play continuously; taps do nothing
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
    try {
      frameDelays.value = book.delays ? (JSON.parse(book.delays) as number[]) : [];
    } catch {
      frameDelays.value = [];
    }
    // Animated books always start at frame 0; regular books resume saved progress.
    current.value = isAnimated.value ? 0 : Math.min(readBookProgress(props.id), Math.max(0, count - 1));
  } catch (e) {
    console.error('Failed to load book:', e);
  }
}

/** How many pages on each side of the current page to keep preloaded. Wider
 *  than the old span=2 so back/forward jumps within a few pages are instant,
 *  while the out-of-window eviction below keeps memory bounded for huge books. */
const PREFETCH_SPAN = 10;
// Pages currently being fetched by an in-flight prefetchPages call. Without
// this, two overlapping calls (rapid scrolling fires the watch repeatedly)
// both see a not-yet-assigned page as "unloaded", both fetch it, and the second
// `blobs[p] = createObjectURL(...)` overwrites the first — orphaning the first
// object URL (never revoked, leaks its image bytes for the session).
const prefetchInFlight = new Set<number>();

async function prefetchPages() {
  if (!props.id || pageCount.value == null || isAnimated.value) return;
  const total = pageCount.value;
  const lo = Math.max(0, current.value - PREFETCH_SPAN);
  const hi = Math.min(total - 1, current.value + PREFETCH_SPAN);

  // Revoke pages that fell out of the window so large books don't accumulate
  // hundreds of object URLs. The current page is always inside [lo, hi] so it
  // is never revoked mid-view.
  for (const keyStr of Object.keys(blobs.value)) {
    const key = Number(keyStr);
    if (Number.isNaN(key) || key < lo || key > hi) {
      URL.revokeObjectURL(blobs.value[key]);
      delete blobs.value[key];
    }
  }

  // Collect unloaded pages within the window that no overlapping call is
  // already fetching, then fetch concurrently — safe now that raw IPC returns
  // ArrayBuffer (no giant JSON arrays in flight).
  const targets: number[] = [];
  for (let p = lo; p <= hi; p++) {
    if (blobs.value[p] || prefetchInFlight.has(p)) continue;
    targets.push(p);
    prefetchInFlight.add(p);
  }
  await Promise.all(
    targets.map(async (p) => {
      try {
        const buf = await api.getBookPage(props.id, p);
        const blob = new Blob([buf], { type: mimeFromArrayBuffer(buf) });
        blobs.value[p] = URL.createObjectURL(blob);
      } catch (e) {
        console.warn(`Failed to load page ${p}:`, e);
      } finally {
        prefetchInFlight.delete(p);
      }
    }),
  );
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
  clearPlayTimer();
  resizeObserver?.disconnect();
  resizeObserver = null;
  closeBitmaps();
  clearBlobs();
  if (uiHideTimer) clearTimeout(uiHideTimer);
  if (previousMode.value && previousSeed.value) {
    applyMd3Theme(previousSeed.value, previousMode.value);
  }
});

watch(() => props.id, () => {
  current.value = 0;
  clearBlobs();
  closeBitmaps();
  loadMetadata();
});

watch([current, pageCount], () => {
  if (isAnimated.value) {
    if (bitmaps.value.length === 0) void preloadFrames();
  } else {
    prefetchPages();
  }
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
