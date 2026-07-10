<template>
  <div class="pa-6">
    <!-- Author card (always visible) -->
    <a
      href="https://github.com/wpy030414"
      target="_blank"
      rel="noreferrer"
      class="md3-card md3-card--outlined author-card mb-6"
    >
      <span class="md3-card__header-avatar">
        <MdiIcon :path="mdiAccountCircle" :size="26" />
      </span>
      <div class="md3-card__header-titles">
        <span class="md3-card__title">{{ t('settings.aboutAuthor') }}</span>
        <span class="md3-card__subtitle">{{ t('settings.authorName') }} · v{{ version }}</span>
      </div>
      <span class="md3-card__header-action">
        <MdiIcon :path="mdiGithub" :size="22" />
      </span>
    </a>

    <!-- Tabs -->
    <md-tabs ref="tabsRef" class="mb-4">
      <md-primary-tab value="basic">{{ t('settings.tab.basic') }}</md-primary-tab>
      <md-primary-tab value="sharing">{{ t('settings.tab.sharing') }}</md-primary-tab>
    </md-tabs>

    <!-- Basic tab -->
    <div v-if="tab === 'basic'">
      <!-- Language -->
      <section class="mb-6">
        <div class="d-flex align-center mb-3">
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

      <md-divider />

      <!-- Theme -->
      <section class="mb-6 mt-6">
        <div class="d-flex align-center mb-3">
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

      <md-divider />

      <!-- Reset -->
      <section class="mb-6 mt-6">
        <div class="d-flex align-center mb-3">
          <MdiIcon :path="mdiDeleteForever" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.reset.title') }}</h3>
        </div>

        <p class="text-body-2 text-medium-emphasis mb-3">
          {{ t('settings.reset.hint') }}
        </p>

        <md-outlined-button
          :disabled="resetting"
          @click="onReset"
        >
          <MdiIcon slot="icon" :path="mdiDeleteForever" :size="20" />
          {{ resetting ? t('settings.reset.running') : t('settings.reset.button') }}
        </md-outlined-button>

        <p v-if="resetError" class="mt-3 text-body-2 text-error">{{ resetError }}</p>
      </section>
    </div>

    <!-- Sharing tab -->
    <div v-if="tab === 'sharing'">
      <!-- OPDS Server -->
      <section class="mb-6">
        <div class="d-flex align-center mb-3">
          <MdiIcon :path="mdiWeb" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.opds') }}</h3>
        </div>

        <div class="d-flex align-start gap-4 flex-wrap">
          <md-outlined-text-field
            :value="settingsStore.opdsPort"
            type="number"
            :label="t('settings.port')"
            style="width: 140px"
            @input="settingsStore.saveOpdsPort(($event.target as HTMLInputElement).value)"
          />

          <md-filled-button
            :disabled="settingsStore.opdsBusy"
            @click="startOpds"
          >
            <MdiIcon slot="icon" :path="mdiPlay" :size="20" />
            {{ t('settings.start') }}
          </md-filled-button>

          <md-outlined-button
            :disabled="settingsStore.opdsBusy"
            @click="stopOpds"
          >
            <MdiIcon slot="icon" :path="mdiStop" :size="20" />
            {{ t('settings.stop') }}
          </md-outlined-button>
        </div>

        <p class="mt-3 text-body-2 text-success d-flex align-center">
          <MdiIcon :path="mdiCheckCircle" :size="16" class="mr-1" />
          {{ t('settings.running') }}
        </p>
        <p v-if="settingsStore.opdsUrl" class="text-body-2">
          <a :href="t('settings.opdsUrl', { url: settingsStore.opdsUrl })" target="_blank" rel="noreferrer">
            {{ t('settings.opdsUrl', { url: settingsStore.opdsUrl }) }}
          </a>
        </p>
        <p v-if="opdsError" class="mt-3 text-body-2 text-error">{{ opdsError }}</p>
      </section>

      <md-divider />

      <!-- RSS Server -->
      <section class="mb-6 mt-6">
        <div class="d-flex align-center mb-3">
          <MdiIcon :path="mdiRss" :size="22" class="mr-2" />
          <h3 class="text-h6">{{ t('settings.rss') }}</h3>
        </div>

        <div class="d-flex align-start gap-4 flex-wrap">
          <md-outlined-text-field
            :value="settingsStore.rssPort"
            type="number"
            :label="t('settings.rssPort')"
            style="width: 140px"
            @input="settingsStore.saveRssPort(($event.target as HTMLInputElement).value)"
          />

          <md-filled-button
            :disabled="settingsStore.rssBusy"
            @click="startRss"
          >
            <MdiIcon slot="icon" :path="mdiPlay" :size="20" />
            {{ t('settings.start') }}
          </md-filled-button>

          <md-outlined-button
            :disabled="settingsStore.rssBusy"
            @click="stopRss"
          >
            <MdiIcon slot="icon" :path="mdiStop" :size="20" />
            {{ t('settings.stop') }}
          </md-outlined-button>
        </div>

        <p class="mt-3 text-body-2 text-success d-flex align-center">
          <MdiIcon :path="mdiCheckCircle" :size="16" class="mr-1" />
          {{ t('settings.running') }}
        </p>
        <p v-if="settingsStore.rssUrl" class="text-body-2">
          <a :href="t('settings.rssUrl', { url: settingsStore.rssUrl })" target="_blank" rel="noreferrer">
            {{ t('settings.rssUrl', { url: settingsStore.rssUrl }) }}
          </a>
        </p>
        <p v-if="rssError" class="mt-3 text-body-2 text-error">{{ rssError }}</p>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import {
  mdiAccountCircle,
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

const { t, locale, setLocale } = useI18n();
const settingsStore = useSettingsStore();
const themeStore = useThemeStore();
const toastStore = useToastStore();

const version = ref('0.1.0');

// Persisted tab (defaults to 'basic' when absent/invalid).
const initialSettingsTab = (() => {
  try {
    return localStorage.getItem('erolib.settings.tab') === 'sharing' ? 'sharing' : 'basic';
  } catch {
    return 'basic';
  }
})();
const tab = ref<'basic' | 'sharing'>(initialSettingsTab);
const opdsError = ref<string | null>(null);
const rssError = ref<string | null>(null);
const resetting = ref(false);
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

async function startOpds() {
  try {
    opdsError.value = null;
    settingsStore.opdsBusy = true;
    const url = await api.startOpdsServer(Number(settingsStore.opdsPort));
    settingsStore.opdsUrl = url;
    settingsStore.opdsRunning = true;
  } catch (e) {
    opdsError.value = String(e);
  } finally {
    settingsStore.opdsBusy = false;
  }
}

async function stopOpds() {
  settingsStore.opdsBusy = true;
  try {
    await api.stopOpdsServer();
    settingsStore.opdsUrl = null;
    settingsStore.opdsRunning = false;
  } catch (e) {
    opdsError.value = String(e);
  } finally {
    settingsStore.opdsBusy = false;
  }
}

async function startRss() {
  try {
    rssError.value = null;
    settingsStore.rssBusy = true;
    const url = await api.startRssServer(Number(settingsStore.rssPort));
    settingsStore.rssUrl = url;
    settingsStore.rssRunning = true;
  } catch (e) {
    rssError.value = String(e);
  } finally {
    settingsStore.rssBusy = false;
  }
}

async function stopRss() {
  settingsStore.rssBusy = true;
  try {
    await api.stopRssServer();
    settingsStore.rssUrl = null;
    settingsStore.rssRunning = false;
  } catch (e) {
    rssError.value = String(e);
  } finally {
    settingsStore.rssBusy = false;
  }
}

async function onReset() {
  const confirmed = window.confirm(t('settings.reset.confirm'));
  if (!confirmed) return;

  resetting.value = true;
  resetError.value = null;
  try {
    const keysToRemove: string[] = [];
    for (let i = 0; i < window.localStorage.length; i++) {
      const key = window.localStorage.key(i);
      if (key?.startsWith('erolib.')) {
        keysToRemove.push(key);
      }
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

.author-card {
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

.author-card:hover {
  background: var(--md-sys-color-surface-container);
}

.author-card .md3-card__header-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
  flex-shrink: 0;
}

.author-card .md3-card__header-titles {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}

.author-card .md3-card__title {
  font: 500 var(--md-sys-typescale-title-large-size) / var(--md-sys-typescale-title-large-line-height) var(--md-sys-typescale-font);
  letter-spacing: var(--md-sys-typescale-title-large-tracking);
  color: var(--md-sys-color-on-surface);
}

.author-card .md3-card__subtitle {
  font: 400 var(--md-sys-typescale-body-medium-size) / var(--md-sys-typescale-body-medium-line-height) var(--md-sys-typescale-font);
  letter-spacing: var(--md-sys-typescale-body-medium-tracking);
  color: var(--md-sys-color-on-surface-variant);
}

.author-card .md3-card__header-action {
  display: inline-flex;
  flex-shrink: 0;
  margin-left: auto;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
