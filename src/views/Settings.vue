<template>
  <div class="pa-6">
    <h2 class="text-h5 mb-6">{{ t('nav.settings') }}</h2>

    <section class="mb-6">
      <div class="d-flex align-center mb-3">
        <MdiIcon :path="mdiWeb" :size="22" class="mr-2" />
        <h3 class="text-h6">{{ t('settings.opds') }}</h3>
      </div>

      <div class="d-flex align-start gap-4 flex-wrap">
        <md-outlined-text-field
          :value="opdsPort"
          type="number"
          :label="t('settings.port')"
          style="width: 140px"
          @input="settingsStore.saveOpdsPort(($event.target as HTMLInputElement).value)"
        />

        <md-filled-button
          :disabled="opdsBusy"
          @click="startOpds"
        >
          <MdiIcon slot="icon" :path="mdiPlay" :size="20" />
          {{ t('settings.start') }}
        </md-filled-button>

        <md-outlined-button
          :disabled="opdsBusy"
          @click="stopOpds"
        >
          <MdiIcon slot="icon" :path="mdiStop" :size="20" />
          {{ t('settings.stop') }}
        </md-outlined-button>
      </div>

      <p v-if="opdsUrl" class="mt-3 text-body-2">
        {{ t('settings.serverRunning') }}
        <a :href="t('settings.opdsUrl', { url: opdsUrl })" target="_blank" rel="noreferrer">
          {{ t('settings.opdsUrl', { url: opdsUrl }) }}
        </a>
      </p>
      <p v-if="opdsError" class="mt-3 text-body-2 text-error">{{ opdsError }}</p>
    </section>

    <md-divider />

    <section class="mb-6">
      <div class="d-flex align-center mb-3">
        <MdiIcon :path="mdiRss" :size="22" class="mr-2" />
        <h3 class="text-h6">{{ t('settings.rss') }}</h3>
      </div>

      <div class="d-flex align-start gap-4 flex-wrap">
        <md-outlined-text-field
          :value="rssPort"
          type="number"
          :label="t('settings.rssPort')"
          style="width: 140px"
          @input="settingsStore.saveRssPort(($event.target as HTMLInputElement).value)"
        />

        <md-filled-button
          :disabled="rssBusy"
          @click="startRss"
        >
          <MdiIcon slot="icon" :path="mdiPlay" :size="20" />
          {{ t('settings.start') }}
        </md-filled-button>

        <md-outlined-button
          :disabled="rssBusy"
          @click="stopRss"
        >
          <MdiIcon slot="icon" :path="mdiStop" :size="20" />
          {{ t('settings.stop') }}
        </md-outlined-button>
      </div>

      <p v-if="rssUrl" class="mt-3 text-body-2">
        {{ t('settings.serverRunning') }}
        <a :href="t('settings.rssUrl', { url: rssUrl })" target="_blank" rel="noreferrer">
          {{ t('settings.rssUrl', { url: rssUrl }) }}
        </a>
      </p>
      <p v-if="rssError" class="mt-3 text-body-2 text-error">{{ rssError }}</p>
    </section>

    <md-divider />

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

    <section class="mb-6">
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
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import {
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
import MdiIcon from '@/components/MdiIcon.vue';

const { t, locale, setLocale } = useI18n();
const settingsStore = useSettingsStore();
const themeStore = useThemeStore();

// OPDS server state
const opdsPort = computed({
  get: () => settingsStore.opdsPort,
  set: (value: string) => settingsStore.saveOpdsPort(value),
});
const opdsUrl = ref<string | null>(null);
const opdsError = ref<string | null>(null);
const opdsBusy = ref(false);

// RSS server state
const rssPort = computed({
  get: () => settingsStore.rssPort,
  set: (value: string) => settingsStore.saveRssPort(value),
});
const rssUrl = ref<string | null>(null);
const rssError = ref<string | null>(null);
const rssBusy = ref(false);

const localeItems = computed(() =>
  LOCALES.map((l) => ({ value: l, label: LOCALE_LABELS[l] })),
);

async function startOpds() {
  try {
    opdsError.value = null;
    opdsBusy.value = true;
    opdsUrl.value = await api.startOpdsServer(Number(opdsPort.value));
  } catch (e) {
    opdsError.value = String(e);
  } finally {
    opdsBusy.value = false;
  }
}

async function stopOpds() {
  opdsBusy.value = true;
  try {
    await api.stopOpdsServer();
    opdsUrl.value = null;
  } catch (e) {
    opdsError.value = String(e);
  } finally {
    opdsBusy.value = false;
  }
}

async function startRss() {
  try {
    rssError.value = null;
    rssBusy.value = true;
    rssUrl.value = await api.startRssServer(Number(rssPort.value));
  } catch (e) {
    rssError.value = String(e);
  } finally {
    rssBusy.value = false;
  }
}

async function stopRss() {
  rssBusy.value = true;
  try {
    await api.stopRssServer();
    rssUrl.value = null;
  } catch (e) {
    rssError.value = String(e);
  } finally {
    rssBusy.value = false;
  }
}

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
  selectRef.value?.addEventListener('change', onLocaleChange);
  switchRef.value?.addEventListener('change', onDarkChange);
});

onBeforeUnmount(() => {
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
</style>
