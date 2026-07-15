<template>
  <!-- A 3×7 grid of square cover tiles packed into a column group that is
       rotated 45° CW around its right-center edge. The rotation pins the group's
       right boundary on the 40%-of-card hinge; the columns then swing left and
       up/down into the 60% text area, showing its left-half diamond. Each column
       scrolls vertically; odd columns move down, even columns up, looping
       seamlessly with no blank gap between cycles. -->
  <div class="wall">
    <div class="wall__cols">
      <div
        v-for="(col, ci) in columns"
        :key="ci"
        class="wall__col"
        :class="ci % 2 === 0 ? 'wall__col--down' : 'wall__col--up'"
      >
        <div class="wall__track">
          <template v-for="dup in 2" :key="dup">
            <div
              v-for="(tile, ri) in col"
              :key="`${ci}-${dup}-${ri}-${tile.id}`"
              class="wall__tile"
            >
              <img
                v-if="tile.src"
                :src="tile.src"
                :alt="tile.title"
                class="wall__img"
                loading="lazy"
              />
            </div>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { Book } from '@/types';

const props = defineProps<{
  /** The 21 (or fewer, cycled) library books the wall is to display. */
  books: Book[];
  /** id → objectURL for a loaded cover; absent key means "still loading". */
  coverMap: Record<string, string | null>;
}>();

/** 3 columns × 7 rows. Split the input books into three 7-tile stacks. */
const COLS = 3;
const ROWS = 7;

interface Tile {
  id: string;
  title: string;
  src: string | null;
}

const columns = computed<Tile[][]>(() => {
  const out: Tile[][] = Array.from({ length: COLS }, () => [] as Tile[]);
  const n = props.books.length;
  if (n === 0) return out;
  for (let r = 0; r < ROWS; r++) {
    for (let c = 0; c < COLS; c++) {
      const book = props.books[(r * COLS + c) % n];
      out[c].push({
        id: book.id,
        title: book.original_filename || book.title,
        src: props.coverMap[book.id] ?? null,
      });
    }
  }
  return out;
});
</script>

<style scoped>
/* The wall itself sits in the hero card's 40% right slot.  It is the rotation
   pivot's anchor: the column group inside is rotated 45° CW around this
   element's right-center point.  The wall has no overflow clip of its own —
   only the hero card's overflow: hidden trims the diamond.  Square: width =
   100% of the slot, height = width via aspect-ratio.  The wall is positioned
   at the slot's right edge (right:0) and vertically centered (top:50%;
   translateY(-50%)).  cqw resolves against the slot's container-type:size, so
   --tile = 1/3 of the slot width (note 100cqw = 100% of the container's
   width, NOT cqw / 100). */
.wall {
  --tile: calc(100cqw / 3);
  position: absolute;
  right: 0;
  top: 50%;
  width: 100%;
  aspect-ratio: 1 / 1;
  transform: translateY(-50%);
}

.wall__cols {
  /* Transform origin at the right-center edge of the 40% slot: the columns
     rotate around it, keeping the right boundary pinned on the 60/40 split. */
  position: absolute;
  right: 0;
  top: 50%;
  height: 0;
  transform-origin: right center;
  transform: rotate(45deg);
  display: flex;
  flex-direction: row;
  align-items: flex-start;
  gap: 0;
}

/* A column is one tile wide and 14 tiles tall (7 original + 7 duplicated).
   Overflow hides everything except the middle 7-tile window.  clip-path
   carves the column into a perfect square, in case the rounded column corners
   peek through. */
.wall__col {
  flex: 0 0 auto;
  width: var(--tile);
  height: calc(var(--tile) * 7);
  overflow: hidden;
}

/* Tracks hold two duplicated 7-tile columns stacked (14 tall).  We animate
   translateY by -50% → one set passes exactly into where the other was,
   looping seamlessly. */
.wall__track {
  display: flex;
  flex-direction: column;
  gap: 0;
  height: calc(var(--tile) * 14);
  width: var(--tile);
}

.wall__col--down .wall__track {
  animation: wall-down 40s linear infinite;
}

.wall__col--up .wall__track {
  animation: wall-up 40s linear infinite;
}

.wall__tile {
  width: var(--tile);
  height: var(--tile);
  flex: 0 0 auto;
}

.wall__img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  opacity: 0.5;
}

/* Down: scroll the duplicated set so one set's height (-50% of 14 = 7) moves
   exactly one cycle.  Using % avoids var() in @keyframes, which some browsers
   refuse. */
@keyframes wall-down {
  from {
    transform: translateY(0);
  }
  to {
    transform: translateY(-50%);
  }
}

/* Up: reverse. */
@keyframes wall-up {
  from {
    transform: translateY(-50%);
  }
  to {
    transform: translateY(0);
  }
}
</style>
