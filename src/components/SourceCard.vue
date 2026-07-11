<template>
  <div class="book-card" :class="{ 'book-card--busy': isBusy }">
    <div class="book-cover-wrap">
      <img v-if="cover" :src="cover" class="book-cover" :alt="title" loading="lazy" decoding="async" />
      <div v-else class="book-placeholder">{{ title.charAt(0).toUpperCase() }}</div>
      <div class="book-pages-badge">{{ pageCount }}</div>
      <div v-if="isBusy" class="book-cover-mask">
        <div v-if="hasProgress" class="progress-ring-wrap">
          <svg class="progress-ring" viewBox="0 0 36 36">
            <circle class="ring-track" cx="18" cy="18" :r="RING_R" />
            <circle
              class="ring-fill"
              cx="18"
              cy="18"
              :r="RING_R"
              :stroke-dasharray="RING_CIRCUM"
              :stroke-dashoffset="ringOffset"
            />
          </svg>
          <span class="progress-text">{{ status?.progressCurrent }}/{{ status?.progressTotal }}</span>
        </div>
        <svg v-else class="spinner" style="color: #fff" viewBox="0 0 50 50" aria-hidden="true">
          <circle class="spinner-track" cx="25" cy="25" r="20" />
          <circle class="spinner-arc" cx="25" cy="25" r="20" />
        </svg>
      </div>
    </div>
    <div class="md3-card__content">
      <span v-if="isNew" class="new-dot" aria-hidden="true" />
      <div class="md3-card__title text-subtitle-2"><span class="title-inner">{{ title }}</span></div>
      <div v-if="subtitle" class="md3-card__subtitle text-body-2 text-truncate">
        {{ subtitle }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { CardStatus } from '@/types';

/**
 * Unified display card for Library, Pixiv, and EHentai grids. Source-specific
 * fields are normalised by the caller: `title`/`pageCount` come straight off
 * the item, `subtitle` is the per-source secondary line (Pixiv `work.author`,
 * EHentai `item.uploader`, Library `book.author`), and `status` is the local
 * browse state. Omitting `status` (Library cards) yields a plain card with no
 * red "new" dot and no download overlay.
 *
 * No emits: the root `.book-card` receives native `@click` (and any other
 * listeners / `id`) via Vue's attribute fallthrough, so callers dispatch state
 * themselves. Scoped CSS is the (byte-identical) merged set from the former
 * PixivCard / EHentaiCard. */
const props = defineProps<{
  title: string;
  pageCount: number;
  subtitle?: string;
  cover: string | null;
  status?: CardStatus;
}>();

const ACTIVE = ['pending', 'running', 'paused'];

const isLocal = computed(() => !!props.status?.localBookId);
const isBusy = computed(
  () => !!props.status?.taskId && ACTIVE.includes(props.status?.taskStatus ?? ''),
);
/** New = a browse card (status given) that is neither downloaded nor
 *  downloading → shows the red dot. Library cards pass status=undefined, so
 *  they never get a dot. */
const isNew = computed(() => props.status !== undefined && !isLocal.value && !isBusy.value);

// Progress ring. SVG is far lighter than md-circular-progress determinate,
// which stuttered badly under per-page progress updates. Render the ring only
// once we know the page total; before that, an indeterminate spinner.
const hasProgress = computed(() => (props.status?.progressTotal ?? 0) > 1);
const RING_R = 16;
const RING_CIRCUM = 2 * Math.PI * RING_R;
const ringOffset = computed(() => {
  const total = props.status?.progressTotal ?? 0;
  const ratio = total > 1 ? Math.min(1, (props.status?.progressCurrent ?? 0) / total) : 0;
  return RING_CIRCUM * (1 - ratio);
});
</script>

<style scoped>
/* While downloading the card is non-interactive (click is a no-op). */
.book-card--busy {
  pointer-events: none;
}

/* Overlay + progress shown over the cover while a download is in flight. */
.book-cover-mask {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
}

.progress-ring-wrap {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.progress-ring {
  width: 44px;
  height: 44px;
  transform: rotate(-90deg);
}

.progress-ring .ring-track {
  fill: none;
  stroke: rgba(255, 255, 255, 0.3);
  stroke-width: 3;
}

.progress-ring .ring-fill {
  fill: none;
  stroke: #fff;
  stroke-width: 3;
  stroke-linecap: round;
  transition: stroke-dashoffset 0.2s ease;
}

.progress-text {
  position: absolute;
  color: #fff;
  font: 500 10px / 1 var(--md-sys-typescale-font);
  font-variant-numeric: tabular-nums;
}

/* Red "new" dot at the top-left of the title's first character. Anchored to
   the content box — NOT the title, which is `text-truncate` (overflow:hidden)
   and would clip the dot. Rendered above the card background, below the title. */
.md3-card__content {
  position: relative;
}

.md3-card__title {
  position: relative;
  z-index: 2;
}

.new-dot {
  position: absolute;
  top: 11px;
  left: 11px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--md-sys-color-error, #e23b2e);
  z-index: 1;
  pointer-events: none;
}
</style>
