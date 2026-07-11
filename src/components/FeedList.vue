<template>
  <div>
    <div v-if="feed.items.length" class="md3-grid">
      <slot />
    </div>
    <div v-else-if="!feed.loading" class="text-center text-medium-emphasis mt-8">
      {{ texts.empty }}
    </div>
    <div v-if="feed.end && feed.items.length" class="feed-end text-center text-medium-emphasis">
      {{ texts.end }}
    </div>
    <div v-if="feed.loading" class="feed-loading text-center text-medium-emphasis">
      <md-circular-progress indeterminate />
      <span>{{ texts.loadingMore }}</span>
    </div>
    <div ref="sentinel" class="feed-sentinel" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useInfiniteSentinel } from '@/composables/useInfiniteSentinel';

/**
 * Browse-grid scaffolding shared by every feed (Pixiv recommend/following/
 * bookmark/search, EHentai results). Renders the responsive card grid (via the
 * default slot — the caller does its own `v-for` over `feed.items` inside it),
 * the empty / end / loading-more states, and the lazy-load sentinel.
 *
 * `md-circular-progress indeterminate` is safe here — the known WKWebView
 * stutter only affects the *determinate* variant under per-page updates (see
 * AGENTS.md); the per-feed progress ring on SourceCard stays hand-rolled SVG.
 * The sentinel arms itself on mount, so a freshly-shown feed (tab switch, post
 * login, ex toggle) auto-loads its first page. */
defineProps<{
  feed: { items: unknown[]; loading: boolean; end: boolean };
  texts: { empty: string; end: string; loadingMore: string };
}>();

const emit = defineEmits<{ (e: 'load-more'): void }>();

const sentinel = ref<HTMLElement | null>(null);
useInfiniteSentinel(sentinel, () => emit('load-more'));
</script>

<style scoped>
.feed-sentinel {
  height: 1px;
}

.feed-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px 0;
}

.feed-end {
  padding: 20px 0;
  font-size: 13px;
}
</style>
