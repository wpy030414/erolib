import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import router from './router';
import { readSavedTheme, useThemeStore } from './stores/theme';
import { applyMd3Theme } from './services/md3-theme';

// MD3 design tokens + base utilities, then register Material Web components.
import './styles/tokens.css';
import './styles/md3.css';
import './material-web.ts';

// Apply the saved/derived MD3 theme tokens to :root before mount so the first
// paint is already themed (no white flash) and MWC picks them up immediately.
const initial = readSavedTheme();
applyMd3Theme(initial.seed, initial.mode);

import { applyWindowTitle } from './i18n';

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount('#app');

// Keep the theme store's persisted seed/mode in sync with the tokens that
// actually landed (system-dark default may have been selected above).
const themeStore = useThemeStore();
if (initial.seed !== themeStore.seed || initial.mode !== themeStore.mode) {
  themeStore.setSeed(initial.seed);
  themeStore.setMode(initial.mode);
}

// Apply localized window title once app locale is resolved.
applyWindowTitle();
