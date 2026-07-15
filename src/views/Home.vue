<template>
  <div class="pa-6 home-view">
    <div class="home-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 home-header__title">{{ t('nav.home') }}</h2>
      <span class="spacer" />
    </div>

    <!-- Loading state: the aggregate calls are in flight. -->
    <div v-if="loading" class="home-loading">
      <md-circular-progress indeterminate />
    </div>

    <!-- Error state (Library/NiceCat pattern copy). -->
    <div v-else-if="error" class="error-state">
      <p class="error-state__msg">{{ error }}</p>
    </div>

    <template v-else>
      <!-- (A) Hero card: weekly stat (left 60%) + the rotating cover wall
           (right 40%).  The wall is a square rotated 45° CW around its right
           center edge; overflow: hidden on the card clips the diamond's top and
           bottom.  Hovering no longer pauses the scroll — the wall spins freely. -->
      <section class="hero">
        <div class="hero__text">
          <div class="hero__icon" aria-hidden="true">⏱</div>
          <div class="hero__body">
            <div class="hero__label">{{ t('home.weekly') }}</div>
            <h1 class="hero__value">
              {{ reading.value }}{{ t('home.unit.' + reading.unit) }}
            </h1>
          </div>
        </div>
        <div v-if="wallBooks.length" class="hero__wall">
          <WallCover
            :books="wallBooks"
            :cover-map="coverMap"
          />
        </div>
      </section>

      <!-- (B) Recent books — 最近阅读. -->
      <section class="home-section mb-6">
        <h3 class="text-h6 home-section__title">{{ t('home.recently') }}</h3>
        <div v-if="recent.length" class="md3-grid">
          <div v-for="book in recent" :key="book.id">
            <SourceCard
              :id="'home-recent-' + book.id"
              :title="book.title"
              :page-count="book.page_count"
              :subtitle="book.author"
              :cover="coverMap[book.id] ?? null"
              @click="router.push(`/reader/${book.id}`)"
            />
          </div>
        </div>
        <div v-else class="text-body-2 text-medium-emphasis home-empty">
          {{ t('home.noData') }}
        </div>
      </section>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount } from 'vue';
import { useRouter } from 'vue-router';
import { api } from '@/services/api';
import { getThumb, setThumb } from '@/services/thumb-cache';
import { useI18n } from '@/i18n';
import SourceCard from '@/components/SourceCard.vue';
import WallCover from '@/components/WallCover.vue';
import type { Book } from '@/types';

const router = useRouter();
const { t } = useI18n();

const loading = ref(true);
const error = ref<string | null>(null);

const totalMs = ref(0);
const recent = ref<Book[]>([]);
const library = ref<Book[]>([]);

/** Reading time this week, formatted for display:
 *  - 0 ms → "0{unit.minute}"
 *  - < 60 min → integer minutes (sub-minute rounds up to 1), no decimals
 *  - >= 60 min → "x.x{unit.hour}" with one decimal */
const reading = computed(() => {
  const totalMinutes = totalMs.value / 60_000;
  if (totalMinutes >= 60) {
    return { value: (totalMinutes / 60).toFixed(1), unit: 'hour' };
  }
  if (totalMs.value <= 0) {
    return { value: '0', unit: 'minute' };
  }
  // Sub-minute rounds up to 1; otherwise nearest integer, no decimals.
  const minutes = Math.max(1, Math.round(totalMinutes));
  return { value: String(minutes), unit: 'minute' };
});

/* ---- Wall sampling ----
   The wall wants 21 distinct-looking covers across 3 columns × 7 rows.  When
   the library is smaller than 21 we prefer variety (shuffle the whole library
   once per load) then take the first 21 with a wrap, so the same book rarely
   shows up twice in a single column. */

const WALL_SLOTS = 21;

/** Deterministic hash of a string → 32-bit unsigned.  Stable for a given id,
 *  so the shuffle is reproducible across re-renders. */
function hashU32(s: string): number {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

const shuffledLibrary = computed(() => {
  // Sort by hashed id for a stable-but-even spread.
  return [...library.value].sort(
    (a, b) => hashU32(a.id) - hashU32(b.id),
  );
});

/** Final 21 (or fewer) books the wall will render.  Empty → wall is hidden. */
const wallBooks = computed<Book[]>(() => {
  const src = shuffledLibrary.value;
  if (src.length === 0) return [];
  const out: Book[] = [];
  for (let i = 0; i < WALL_SLOTS; i++) out.push(src[i % src.length]);
  return out;
});

/* ---- Cover loading ---- */

/** id → objectURL for a loaded cover, null if failed, absent if pending. */
const coverMap = reactive<Record<string, string | null>>({});
const disposals: Array<() => void> = [];

async function loadCover(book: Book): Promise<(() => void) | void> {
  if (book.id in coverMap) return;
  coverMap[book.id] = null;
  let alive = true;
  let url: string | null = null;
  try {
    const key = book.source_post_id || book.id;
    let blob = await getThumb(key);
    if (!blob) {
      const bytes = await api.getBookCoverThumb(book.id);
      if (!alive) return;
      blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
      void setThumb(key, blob);
    }
    if (!alive) return;
    url = URL.createObjectURL(blob);
    coverMap[book.id] = url;
  } catch {
    /* leave null placeholder */
  }
  const dispose = () => {
    alive = false;
    if (url) URL.revokeObjectURL(url);
  };
  disposals.push(dispose);
  return dispose;
}

/** When hovering the hero, dim the text slightly for legibility. */
onMounted(async () => {
  try {
    const [ms, rec, lib] = await Promise.all([
      api.getWeeklyReadingMs(),
      api.listRecentBooks(12),
      api.listBooks(),
    ]);
    totalMs.value = ms;
    recent.value = rec;
    library.value = lib;
    await Promise.all([
      ...recent.value.map(loadCover),
      ...wallBooks.value.map(loadCover),
    ]);
  } catch (e) {
    error.value = t('common.error', { message: String(e) });
  } finally {
    loading.value = false;
  }
});

onBeforeUnmount(() => {
  for (const d of disposals) d();
});
</script>

<style scoped>
.home-header__title {
  margin: 0;
  white-space: nowrap;
}

/* Other route pages have an icon-btn (height 40px) in their header row so the
   flex container is naturally tall enough. Home has only the title + spacer,
   so we pin the row to the same minimum height so the title sits at the same
   vertical position across the app. */
.home-header {
  min-height: 40px;
}

.home-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 48px 0;
}

/* ---- Hero card: 40% wall slot; min-height content-driven by text.
   Wall is a square (aspect-ratio 1/1) anchored right, so it must be no taller
   than the slot: slot's height = card height; wall's side = slot width; if the
   slot aspect is landscape, side < slot height and the wall floats centered in
   the slot's vertical center.  overflow: hidden clips the diamond's top &
   bottom, and also clips the wall's left half-spill into the text area only to
   the diamond silhouette since the rotation is 45° around the slot's right
   center. */
.hero {
  display: flex;
  align-items: stretch;
  border-radius: var(--md-sys-shape-corner-large);
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
  overflow: hidden;
  position: relative;
  min-height: 160px;
  margin-bottom: 24px;
}

.hero__text {
  flex: 0 0 60%;
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 24px;
  z-index: 1;
}

.hero__icon {
  font-size: 40px;
  line-height: 1;
}

.hero__body {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.hero__label {
  font: var(--md-sys-typescale-body-medium-font, 400 0.875rem / 1.25rem var(--md-sys-typescale-font));
  letter-spacing: var(--md-sys-typescale-body-medium-tracking, 0.015625rem);
  color: var(--md-sys-color-on-primary-container);
  opacity: 0.85;
}

.hero__value {
  margin: 0;
  font: var(--md-sys-typescale-display-small-font, 400 2.25rem / 2.75rem var(--md-sys-typescale-font));
  letter-spacing: var(--md-sys-typescale-display-small-tracking, 0);
  color: var(--md-sys-color-on-primary-container);
}

/* The wall's slot: 40% of the hero's width, full height but capped to the slot's
   height so the rotated wall stays centered.  container-type: size lets the
   WallCover inside use cqw against this slot's width — that's what sizes the
   square wall. */
.hero__wall {
  flex: 0 0 40%;
  position: relative;
  height: 100%;
  container-type: size;
  z-index: 0;
}

/* ---- Section scaffolding ---- */
.home-section__title {
  margin: 0 0 12px 0;
  color: var(--md-sys-color-on-surface);
}

.home-empty {
  padding: 8px 0;
}

.error-state {
  text-align: center;
  color: var(--md-sys-color-error);
  margin-top: 16px;
  padding: 12px 16px;
  background: var(--md-sys-color-error-container);
  border-radius: 8px;
}

.error-state__msg {
  margin: 0;
  font-size: 0.875rem;
  word-break: break-all;
}
</style>
