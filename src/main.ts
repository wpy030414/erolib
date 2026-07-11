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

// Card title marquee: hover a truncated title to scroll it (2rem/s, pause ~3s
// at the end, then loop). Measures overflow on first hover and only enables the
// animation when the title actually overflows — short titles stay still.
function setupTitleMarquee() {
  document.addEventListener('mouseover', (e) => {
    const title = (e.target as HTMLElement).closest('.md3-card__title') as HTMLElement | null;
    if (!title || title.dataset.marqueeReady === '1') return;
    title.dataset.marqueeReady = '1';
    const inner = title.querySelector<HTMLElement>('.title-inner');
    if (!inner) return;
    const overflow = inner.scrollWidth - title.clientWidth;
    if (overflow <= 0) return;
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    const scrollSec = overflow / (2 * rem); // 2rem/s
    title.style.setProperty('--title-scroll', `${-overflow}px`);
    title.style.setProperty('--title-dur', `${scrollSec + 3}s`);
    title.classList.add('is-marquee');
  });
}
setupTitleMarquee();
