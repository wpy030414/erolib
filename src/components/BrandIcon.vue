<template>
  <svg
    :width="size"
    :height="size"
    :viewBox="viewBox"
    aria-hidden="true"
    focusable="false"
    fill="currentColor"
  >
    <g :transform="brand ? inset : undefined">
      <path :d="path" :fill-rule="fillRule" :clip-rule="fillRule" />
    </g>
  </svg>
</template>

<script setup lang="ts">
import { computed } from 'vue';

/** Renders a brand mark with the SAME conventions as an MDI icon: a square
 *  box, `fill="currentColor"` (so it follows the surrounding color — theme,
 *  active state — exactly like mdi paths). Brand marks that aren't natively
 *  24×24 pass their own viewBox; the svg scales them into the same square.
 *
 *  When `brand` is true the path is scaled down to 75% and centred so that
 *  edge-to-edge artwork reads at the same visual weight as native MDI icons
 *  (which have 1–2 px of built-in breathing room).  Set `brand` false (the
 *  default) for MDI paths that are already correctly sized. */
const props = withDefaults(
  defineProps<{
    path: string;
    /** Defaults to the MDI 24×24 grid; pass the source svg's own box otherwise. */
    viewBox?: string;
    size?: number;
    /** `evenodd` for marks with holes (e.g. bilibili's eyes). Omit = nonzero. */
    fillRule?: 'nonzero' | 'evenodd';
    /** Apply an 0.75× inset so edge-to-edge artwork matches MDI's native padding. */
    brand?: boolean;
  }>(),
  {
    viewBox: '0 0 24 24',
    size: 24,
    brand: false,
  },
);

const SCALE = 0.75;

const inset = computed(() => {
  const parts = props.viewBox.split(/\s+/).map(Number);
  const w = parts[2] ?? 24;
  const h = parts[3] ?? 24;
  const tx = (w - w * SCALE) / 2;
  const ty = (h - h * SCALE) / 2;
  return `translate(${tx}, ${ty}) scale(${SCALE})`;
});
</script>
