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
        <svg
          :width="24"
          :height="24"
          viewBox="0 0 24 24"
          aria-hidden="true"
          focusable="false"
          fill="currentColor"
        >
          <path :d="item.icon" />
        </svg>
      </div>
      <span class="nav-label">{{ t(item.labelKey) }}</span>
    </router-link>
  </nav>
</template>

<script setup lang="ts">
import {
  mdiBookOutline,
  mdiAlphaP,
  mdiAlphaE,
  mdiCogOutline,
} from '@mdi/js';
import { useRoute } from 'vue-router';
import { useI18n } from '@/i18n';

const route = useRoute();
const { t } = useI18n();

interface NavItem {
  path: string;
  labelKey: 'nav.library' | 'nav.pixiv' | 'nav.ehentai' | 'nav.settings';
  icon: string;
}

const navItems: NavItem[] = [
  { path: '/library', labelKey: 'nav.library', icon: mdiBookOutline },
  { path: '/pixiv', labelKey: 'nav.pixiv', icon: mdiAlphaP },
  { path: '/ehentai', labelKey: 'nav.ehentai', icon: mdiAlphaE },
  { path: '/settings', labelKey: 'nav.settings', icon: mdiCogOutline },
];
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
