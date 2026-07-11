<template>
  <nav class="nav-rail">
    <router-link
      v-for="item in navItems"
      :key="item.path"
      :to="item.path"
      class="nav-item"
      :class="{ active: route.path === item.path }"
    >
      <div class="nav-item__icon">
        <BrandIcon
          :path="item.icon"
          :view-box="item.viewBox"
          :fill-rule="item.fillRule"
          :size="24"
        />
      </div>
      <span class="nav-label">{{ navLabel(item) }}</span>
    </router-link>
  </nav>
</template>

<script setup lang="ts">
import {
  mdiBookOutline,
  mdiClipboardList,
  mdiCogOutline,
} from '@mdi/js';
import { useRoute } from 'vue-router';
import BrandIcon from '@/components/BrandIcon.vue';
import { useI18n } from '@/i18n';
import { useEhentaiBrowseStore } from '@/stores/ehentai-browse';

const route = useRoute();
const { t } = useI18n();
// Read EX mode so the nav rail flips between "EHentai" and "EXHentai" with
// the toggle in the view header. Instantiating the store here also registers
// its task://progress listener at app start so gallery downloads keep updating
// even before the EHentai view is first opened.
const ehStore = useEhentaiBrowseStore();

interface NavItem {
  path: string;
  labelKey: 'nav.library' | 'nav.pixiv' | 'nav.ehentai' | 'nav.tasks' | 'nav.settings';
  /** Icon path (mdi or brand), rendered monochrome via currentColor. */
  icon: string;
  /** Override the 24×24 viewBox for brand marks drawn on another grid. */
  viewBox?: string;
  /** `evenodd` for paths with holes. */
  fillRule?: 'nonzero' | 'evenodd';
}

/** Pixiv brand mark (single 24×24 path). The source svg is solid black, so we
 *  inline the path and let `fill="currentColor"` recolour it — an <img> would
 *  be invisible in dark mode. */
const PIXIV_PATH =
  'M4.935 0A4.924 4.924 0 0 0 0 4.935v14.13A4.924 4.924 0 0 0 4.935 24h14.13A4.924 4.924 0 0 0 24 19.065V4.935A4.924 4.924 0 0 0 19.065 0zm7.81 4.547c2.181 0 4.058.676 5.399 1.847a6.118 6.118 0 0 1 2.116 4.66c.005 1.854-.88 3.476-2.257 4.563-1.375 1.092-3.225 1.697-5.258 1.697-2.314 0-4.46-.842-4.46-.842v2.718c.397.116 1.048.365.635.779H5.79c-.41-.41.19-.65.644-.779V7.666c-1.053.81-1.593 1.51-1.868 2.031.32 1.02-.284.969-.284.969l-1.09-1.73s3.868-4.39 9.553-4.39zm-.19.971c-1.423-.003-3.184.473-4.27 1.244v8.646c.988.487 2.484.832 4.26.832h.01c1.596 0 2.98-.593 3.93-1.533.952-.948 1.486-2.183 1.492-3.683-.005-1.54-.504-2.864-1.42-3.86-.918-.992-2.274-1.645-4.002-1.646Z';

/** EHentai "EH" monogram — the source favicon is 8 pixel blocks on a 140×120
 *  grid; unioned here into one path. BrandIcon scales it into the 24px square
 *  and fills it with currentColor, matching every other nav icon. */
const EHENTAI_PATH =
  'M50 50 h20 v20 h-20 Z M20 100 h30 v20 h-30 Z M0 0 h20 v120 h-20 Z M20 0 h30 v20 h-30 Z M80 0 h20 v120 h-20 Z M120 0 h20 v120 h-20 Z M100 50 h20 v20 h-20 Z M20 50 h20 v20 h-20 Z';

const navItems: NavItem[] = [
  { path: '/library', labelKey: 'nav.library', icon: mdiBookOutline },
  { path: '/pixiv', labelKey: 'nav.pixiv', icon: PIXIV_PATH },
  { path: '/ehentai', labelKey: 'nav.ehentai', icon: EHENTAI_PATH, viewBox: '0 0 140 120' },
  { path: '/tasks', labelKey: 'nav.tasks', icon: mdiClipboardList },
  { path: '/settings', labelKey: 'nav.settings', icon: mdiCogOutline },
];

/** The EHentai nav label follows EX mode; everything else is a plain lookup. */
function navLabel(item: NavItem): string {
  if (item.path === '/ehentai') {
    return ehStore.ex ? t('nav.exhentai') : t('nav.ehentai');
  }
  return t(item.labelKey);
}
</script>

<style scoped>
.nav-rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex-shrink: 0;
  width: 84px;
  height: 100%;
  padding-top: 24px;
  background: var(--md-sys-color-surface);
  border-right: 1px solid var(--md-sys-color-outline-variant);
}

.nav-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  width: 100%;
  padding: 4px 0;
  text-decoration: none;
}

.nav-item__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: var(--md-sys-shape-corner-full);
  color: var(--md-sys-color-on-surface-variant);
  transition:
    background-color 0.15s ease,
    color 0.15s ease;
}

.nav-item.active .nav-item__icon {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.nav-item:hover:not(.active) .nav-item__icon {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}

.nav-label {
  font-size: var(--md-sys-typescale-label-medium-size);
  font-weight: 500;
  line-height: var(--md-sys-typescale-label-medium-line-height);
  letter-spacing: var(--md-sys-typescale-label-medium-tracking);
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
}

.nav-item.active .nav-label {
  color: var(--md-sys-color-on-secondary-container);
}
</style>
