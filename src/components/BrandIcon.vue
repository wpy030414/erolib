<template>
  <svg
    :width="size"
    :height="size"
    :viewBox="viewBox"
    aria-hidden="true"
    focusable="false"
    fill="currentColor"
  >
    <path :d="path" :fill-rule="fillRule" :clip-rule="fillRule" />
  </svg>
</template>

<script setup lang="ts">
/** Renders a brand mark with the SAME conventions as an MDI icon: a square
 *  box, `fill="currentColor"` (so it follows the surrounding color — theme,
 *  active state — exactly like mdi paths). Brand marks that aren't natively
 *  24×24 pass their own viewBox; the svg scales them into the same square. */
withDefaults(
  defineProps<{
    path: string;
    /** Defaults to the MDI 24×24 grid; pass the source svg's own box otherwise. */
    viewBox?: string;
    size?: number;
    /** `evenodd` for marks with holes (e.g. bilibili's eyes). Omit = nonzero. */
    fillRule?: 'nonzero' | 'evenodd';
  }>(),
  {
    viewBox: '0 0 24 24',
    size: 24,
  },
);
</script>
