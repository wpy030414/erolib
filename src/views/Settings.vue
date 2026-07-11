<template>
  <div class="pa-6">
    <!-- Project + author cards (split one row 50/50) -->
    <div class="about-row mb-6">
      <a
        href="https://github.com/wpy030414/erolib"
        target="_blank"
        rel="noreferrer"
        class="md3-card md3-card--outlined about-card"
      >
        <div class="md3-card__header-titles">
          <span class="md3-card__title">{{ t('settings.projectName') }}</span>
          <span class="md3-card__subtitle">v{{ version }}</span>
        </div>
        <span class="md3-card__header-action">
          <MdiIcon :path="mdiGithub" :size="22" />
        </span>
      </a>

      <a
        href="https://space.bilibili.com/92465406"
        target="_blank"
        rel="noreferrer"
        class="md3-card md3-card--outlined about-card"
      >
        <div class="md3-card__header-titles">
          <span class="md3-card__title">{{ t('settings.authorName') }}</span>
          <span class="md3-card__subtitle">&ldquo;Do one thing, and do it well.&rdquo;</span>
        </div>
        <span class="md3-card__header-action">
          <BrandIcon :path="BILIBILI_PATH" fill-rule="evenodd" :size="22" />
        </span>
      </a>
    </div>

    <!-- Tabs -->
    <md-tabs ref="tabsRef" class="mb-4">
      <md-primary-tab value="basic">{{ t('settings.tab.basic') }}</md-primary-tab>
      <md-primary-tab value="sharing">{{ t('settings.tab.sharing') }}</md-primary-tab>
    </md-tabs>

    <!-- Basic tab -->
    <div v-if="tab === 'basic'">
      <!-- Language -->
      <section class="mb-6">
        <div class="d-flex align-center mb-2">
          <MdiIcon :path="mdiTranslate" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.language') }}</h3>
        </div>

        <md-outlined-select
          ref="selectRef"
          :label="t('settings.language')"
          style="max-width: 240px"
        >
          <md-select-option
            v-for="item in localeItems"
            :key="item.value"
            :value="item.value"
            :selected="locale === item.value"
          >
            {{ item.label }}
          </md-select-option>
        </md-outlined-select>
      </section>

      <!-- Theme -->
      <section class="mb-6">
        <div class="d-flex align-center mb-2">
          <MdiIcon :path="mdiPalette" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.theme') }}</h3>
        </div>

        <p class="text-body-2 text-medium-emphasis mb-3">
          {{ t('settings.theme.seed') }}
        </p>
        <div class="d-flex gap-3 mb-4">
          <template v-for="s in themeStore.SEEDS" :key="s.key">
            <button
              class="theme-swatch"
              :class="{ 'theme-swatch--selected': themeStore.seed === s.key }"
              :aria-label="s.key"
              :style="{ backgroundColor: s.color }"
              @click="themeStore.setSeed(s.key)"
            />
          </template>
        </div>

        <div class="d-flex align-center">
          <span class="text-body-2 dark-mode-label">{{ t('settings.theme.dark') }}</span>
          <md-switch
            ref="switchRef"
            :selected="themeStore.mode === 'dark'"
          />
        </div>
      </section>

      <!-- Personal data -->
      <section class="mb-6">
        <div class="d-flex align-center mb-2">
          <MdiIcon :path="mdiDeleteForever" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.reset.title') }}</h3>
        </div>

        <div class="data-row">
          <div class="data-row__text">
            <div class="data-row__title">{{ t('settings.reset.clearCache') }}</div>
            <div class="data-row__sub">{{ t('settings.reset.clearCacheHint') }}</div>
          </div>
          <md-outlined-button :disabled="clearingCache" @click="onClearCache">
            <MdiIcon slot="icon" :path="mdiBroom" :size="20" />
            {{ t('settings.reset.clearCache') }}
          </md-outlined-button>
        </div>

        <div class="data-row">
          <div class="data-row__text">
            <div class="data-row__title">{{ t('settings.reset.clearAll') }}</div>
            <div class="data-row__sub">{{ t('settings.reset.clearAllHint') }}</div>
          </div>
          <md-filled-button :disabled="resetting" @click="onClearAll">
            <MdiIcon slot="icon" :path="mdiDeleteForever" :size="20" />
            {{ resetting ? t('settings.reset.running') : t('settings.reset.clearAll') }}
          </md-filled-button>
        </div>

        <p v-if="resetError" class="mt-3 text-body-2 text-error">{{ resetError }}</p>
      </section>
    </div>

    <!-- Sharing tab -->
    <div v-if="tab === 'sharing'">
      <!-- OPDS Server -->
      <section class="mb-6">
        <div class="d-flex align-center mb-2">
          <MdiIcon :path="mdiWeb" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.opds') }}</h3>
        </div>

        <div class="d-flex align-start gap-4 flex-wrap">
          <md-outlined-text-field
            :value="settingsStore.opdsPort"
            type="number"
            :label="t('settings.port')"
            :disabled="settingsStore.opdsRunning || settingsStore.opdsBusy"
            style="width: 140px"
            @input="settingsStore.saveOpdsPort(($event.target as HTMLInputElement).value)"
          />

          <!-- Single toggle: filled "Start" when stopped, outlined "Stop" when
               running — different background colour signals the two states. -->
          <md-filled-button
            v-if="!settingsStore.opdsRunning"
            :disabled="settingsStore.opdsBusy"
            @click="settingsStore.toggleOpds"
          >
            <MdiIcon slot="icon" :path="mdiPlay" :size="20" />
            {{ t('settings.start') }}
          </md-filled-button>
          <md-outlined-button
            v-else
            :disabled="settingsStore.opdsBusy"
            @click="settingsStore.toggleOpds"
          >
            <MdiIcon slot="icon" :path="mdiStop" :size="20" />
            {{ t('settings.stop') }}
          </md-outlined-button>
        </div>

        <p v-if="settingsStore.opdsRunning && settingsStore.opdsUrl" class="mt-3 text-body-2 text-success d-flex align-center">
          <MdiIcon :path="mdiCheckCircle" :size="16" class="mr-1" />
          <a :href="`${settingsStore.opdsUrl}/opds`" target="_blank" rel="noreferrer">
            {{ settingsStore.opdsUrl }}/opds
          </a>
        </p>
        <p v-if="settingsStore.opdsError" class="mt-3 text-body-2 text-error">{{ settingsStore.opdsError }}</p>
      </section>

      <!-- RSS Server -->
      <section class="mb-6">
        <div class="d-flex align-center mb-2">
          <MdiIcon :path="mdiRss" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.rss') }}</h3>
        </div>

        <div class="d-flex align-start gap-4 flex-wrap">
          <md-outlined-text-field
            :value="settingsStore.rssPort"
            type="number"
            :label="t('settings.port')"
            :disabled="settingsStore.rssRunning || settingsStore.rssBusy"
            style="width: 140px"
            @input="settingsStore.saveRssPort(($event.target as HTMLInputElement).value)"
          />

          <md-filled-button
            v-if="!settingsStore.rssRunning"
            :disabled="settingsStore.rssBusy"
            @click="settingsStore.toggleRss"
          >
            <MdiIcon slot="icon" :path="mdiPlay" :size="20" />
            {{ t('settings.start') }}
          </md-filled-button>
          <md-outlined-button
            v-else
            :disabled="settingsStore.rssBusy"
            @click="settingsStore.toggleRss"
          >
            <MdiIcon slot="icon" :path="mdiStop" :size="20" />
            {{ t('settings.stop') }}
          </md-outlined-button>
        </div>

        <p v-if="settingsStore.rssRunning && settingsStore.rssUrl" class="mt-3 text-body-2 text-success d-flex align-center">
          <MdiIcon :path="mdiCheckCircle" :size="16" class="mr-1" />
          <a :href="`${settingsStore.rssUrl}/rss`" target="_blank" rel="noreferrer">
            {{ settingsStore.rssUrl }}/rss
          </a>
        </p>
        <p v-if="settingsStore.rssError" class="mt-3 text-body-2 text-error">{{ settingsStore.rssError }}</p>
      </section>
    </div>
    <md-dialog ref="clearAllDialogRef" @close="onDialogClose">
      <div slot="headline">{{ t('settings.reset.clearAll') }}</div>
      <form id="clear-all-form" slot="content" method="dialog" class="clear-all-dialog__content">
        <p class="text-body-2 text-medium-emphasis">{{ t('settings.reset.clearAllHint') }}</p>
        <p class="text-body-2 text-error">{{ t('settings.reset.confirmWarn') }}</p>
        <md-outlined-text-field
          :label="t('settings.reset.typeConfirm', { phrase: confirmPhrase })"
          :value="confirmInput"
          @input="onConfirmInput"
        />
      </form>
      <div slot="actions">
        <md-text-button form="clear-all-form" value="cancel">
          {{ t('common.cancel') }}
        </md-text-button>
        <md-filled-button form="clear-all-form" value="ok" :disabled="!confirmMatched">
          {{ t('settings.reset.clearAll') }}
        </md-filled-button>
      </div>
    </md-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import {
  mdiBroom,
  mdiCheckCircle,
  mdiDeleteForever,
  mdiGithub,
  mdiPalette,
  mdiPlay,
  mdiRss,
  mdiStop,
  mdiTranslate,
  mdiWeb,
} from '@mdi/js';
import { api } from '@/services/api';
import { useI18n, LOCALES, LOCALE_LABELS, type Locale } from '@/i18n';
import { useSettingsStore } from '@/stores/settings';
import { useThemeStore } from '@/stores/theme';
import { useToastStore } from '@/stores/toast';
import { getVersion } from '@tauri-apps/api/app';
import MdiIcon from '@/components/MdiIcon.vue';
import BrandIcon from '@/components/BrandIcon.vue';
import { clearThumbs } from '@/services/thumb-cache';

const { t, locale, setLocale } = useI18n();
const settingsStore = useSettingsStore();
const themeStore = useThemeStore();
const toastStore = useToastStore();

const version = ref('0.1.0');

/** B站 brand mark (single 24×24 path, evenodd for the eye holes). Rendered
 *  via BrandIcon with currentColor so it matches the GitHub mdi icon's colour. */
const BILIBILI_PATH =
  'M4.977 3.561a1.31 1.31 0 111.818-1.884l2.828 2.728c.08.078.149.163.205.254h4.277a1.32 1.32 0 01.205-.254l2.828-2.728a1.31 1.31 0 011.818 1.884L17.82 4.66h.848A5.333 5.333 0 0124 9.992v7.34a5.333 5.333 0 01-5.333 5.334H5.333A5.333 5.333 0 010 17.333V9.992a5.333 5.333 0 015.333-5.333h.781L4.977 3.56zm.356 3.67a2.667 2.667 0 00-2.666 2.667v7.529a2.667 2.667 0 002.666 2.666h13.334a2.667 2.667 0 002.666-2.666v-7.53a2.667 2.667 0 00-2.666-2.666H5.333zm1.334 5.192a1.333 1.333 0 112.666 0v1.192a1.333 1.333 0 11-2.666 0v-1.192zM16 11.09c-.736 0-1.333.597-1.333 1.333v1.192a1.333 1.333 0 102.666 0v-1.192c0-.736-.597-1.333-1.333-1.333z';

// Persisted tab (defaults to 'basic' when absent/invalid).
const initialSettingsTab = (() => {
  try {
    return localStorage.getItem('erolib.settings.tab') === 'sharing' ? 'sharing' : 'basic';
  } catch {
    return 'basic';
  }
})();
const tab = ref<'basic' | 'sharing'>(initialSettingsTab);
const resetting = ref(false);
const clearingCache = ref(false);
const resetError = ref<string | null>(null);

const localeItems = computed(() =>
  LOCALES.map((l) => ({ value: l, label: LOCALE_LABELS[l] })),
);

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    // fallback
  }
});

async function onClearCache() {
  clearingCache.value = true;
  resetError.value = null;
  try {
    await clearThumbs();
    toastStore.addToast('success', t('settings.reset.cacheToast'));
  } catch (e) {
    resetError.value = String(e);
  } finally {
    clearingCache.value = false;
  }
}

/** Phrase the user must type to enable the destructive confirm button. */
const confirmPhrase = computed(() => t('settings.reset.confirmPhrase'));
const confirmInput = ref('');
const confirmMatched = computed(
  () => confirmInput.value.trim() === confirmPhrase.value,
);
type MdDialogEl = HTMLElement & { show: () => void; returnValue: string };
const clearAllDialogRef = ref<MdDialogEl | null>(null);

function onConfirmInput(e: Event) {
  confirmInput.value = (e.target as HTMLInputElement).value;
}

/** Open the MD3 confirmation dialog instead of window.confirm — the user must
 *  type the confirm phrase before the destructive button is enabled. */
function onClearAll() {
  confirmInput.value = '';
  clearAllDialogRef.value?.show();
}

function onDialogClose() {
  if (clearAllDialogRef.value?.returnValue === 'ok') {
    void doClearAll();
  }
}

async function doClearAll() {
  resetting.value = true;
  resetError.value = null;
  try {
    // 0. Stop the sharing servers first so their bound ports are released
    //    before the backend data dir is wiped.
    await settingsStore.stopOpds();
    await settingsStore.stopRss();
    // 1. Backend data dir: books, DB, cover files, login sessions, tasks.
    await api.resetAppData();
    // 2. Frontend IndexedDB cover cache.
    await clearThumbs();
    // 3. Frontend user settings in localStorage (theme, ports, tabs, EX…).
    const keysToRemove: string[] = [];
    for (let i = 0; i < window.localStorage.length; i++) {
      const key = window.localStorage.key(i);
      if (key?.startsWith('erolib.')) keysToRemove.push(key);
    }
    keysToRemove.forEach((key) => window.localStorage.removeItem(key));
    settingsStore.reset();
    toastStore.addToast('success', t('settings.reset.toast'));
    window.location.reload();
  } catch (e) {
    resetError.value = String(e);
    resetting.value = false;
  }
}

// MDC tab sync
type MdTabs = HTMLElement;
const tabsRef = ref<MdTabs | null>(null);
function onTabChange() {
  if (tabsRef.value) {
    const idx = (tabsRef.value as unknown as { activeTabIndex: number }).activeTabIndex;
    tab.value = idx === 0 ? 'basic' : 'sharing';
  }
}
function syncTabs() {
  if (tabsRef.value) {
    (tabsRef.value as unknown as { activeTabIndex: number }).activeTabIndex = tab.value === 'basic' ? 0 : 1;
  }
}
watch(tab, (v) => {
  syncTabs();
  try {
    localStorage.setItem('erolib.settings.tab', v);
  } catch {
    // ignore storage errors
  }
});

// MDC select/switch sync
type SelectElement = HTMLElement & { value: string };
type SwitchElement = HTMLElement & { selected: boolean };
const selectRef = ref<SelectElement | null>(null);
const switchRef = ref<SwitchElement | null>(null);

function onLocaleChange() {
  if (selectRef.value) {
    setLocale(selectRef.value.value as Locale);
  }
}
function onDarkChange() {
  if (switchRef.value) {
    themeStore.setMode(switchRef.value.selected ? 'dark' : 'light');
  }
}

onMounted(() => {
  syncTabs();
  tabsRef.value?.addEventListener('change', onTabChange);
  selectRef.value?.addEventListener('change', onLocaleChange);
  switchRef.value?.addEventListener('change', onDarkChange);
});

onBeforeUnmount(() => {
  tabsRef.value?.removeEventListener('change', onTabChange);
  selectRef.value?.removeEventListener('change', onLocaleChange);
  switchRef.value?.removeEventListener('change', onDarkChange);
});
</script>

<style scoped>
.dark-mode-label {
  margin-right: 16px;
}

.theme-swatch {
  width: 40px;
  height: 40px;
  padding: 0;
  border: 2px solid transparent;
  border-radius: 50%;
  cursor: pointer;
  transition:
    transform 0.15s ease,
    box-shadow 0.15s ease,
    border-color 0.15s ease;
}

.theme-swatch:hover {
  transform: scale(1.05);
}

.theme-swatch--selected {
  border-color: var(--md-sys-color-on-surface);
  box-shadow: 0 0 0 2px var(--md-sys-color-surface), 0 0 0 4px var(--md-sys-color-on-surface);
}

.about-row {
  display: flex;
  gap: 16px;
}

.about-card {
  flex: 1 1 0;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px;
  text-decoration: none;
  background: var(--md-sys-color-surface);
  border-radius: var(--md-sys-shape-corner-medium);
  border: 1px solid var(--md-sys-color-outline-variant);
  transition: background-color 0.15s ease;
}

.about-card:hover {
  background: var(--md-sys-color-surface-container);
}

.about-card .md3-card__header-titles {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}

.about-card .md3-card__title {
  font: 500 var(--md-sys-typescale-title-large-size) / var(--md-sys-typescale-title-large-line-height) var(--md-sys-typescale-font);
  letter-spacing: var(--md-sys-typescale-title-large-tracking);
  color: var(--md-sys-color-on-surface);
}

.about-card .md3-card__subtitle {
  font: 400 var(--md-sys-typescale-body-medium-size) / var(--md-sys-typescale-body-medium-line-height) var(--md-sys-typescale-font);
  letter-spacing: var(--md-sys-typescale-body-medium-tracking);
  color: var(--md-sys-color-on-surface-variant);
}

.about-card .md3-card__header-action {
  display: inline-flex;
  flex-shrink: 0;
  margin-left: auto;
  color: var(--md-sys-color-on-surface-variant);
}

.data-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 0;
}

.data-row__text {
  min-width: 0;
}

.data-row__title {
  font: 500 var(--md-sys-typescale-body-large-size) / var(--md-sys-typescale-body-large-line-height) var(--md-sys-typescale-font);
  color: var(--md-sys-color-on-surface);
}

.data-row__sub {
  font: 400 var(--md-sys-typescale-body-medium-size) / var(--md-sys-typescale-body-medium-line-height) var(--md-sys-typescale-font);
  color: var(--md-sys-color-on-surface-variant);
}

.clear-all-dialog__content {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: min(360px, 80vw);
}

.clear-all-dialog__content > p {
  margin: 0;
}

.clear-all-dialog__content md-outlined-text-field {
  width: 100%;
}
</style>
