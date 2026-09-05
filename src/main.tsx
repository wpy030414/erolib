import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { readSavedTheme } from './stores/theme';
import { applyMd3Theme, applyArgbTheme, argbFromHex } from './services/md3-theme';

// MD3 design tokens + base utilities
import './styles/tokens.css';
import './styles/md3.css';

// Apply the saved/derived MD3 theme tokens to :root before mount so the first
// paint is already themed (no white flash).
const initial = readSavedTheme();
if (initial.seed.startsWith('custom:')) {
  try {
    const raw = window.localStorage.getItem('erolib.customThemes');
    if (raw) {
      const map = JSON.parse(raw) as Record<string, { seedColorHex: string }>;
      const ct = map[initial.seed];
      if (ct) {
        applyArgbTheme(argbFromHex(ct.seedColorHex), initial.mode === 'dark');
      } else {
        applyMd3Theme('pink', initial.mode);
      }
    } else {
      applyMd3Theme('pink', initial.mode);
    }
  } catch {
    applyMd3Theme('pink', initial.mode);
  }
} else {
  applyMd3Theme(initial.seed, initial.mode);
}

ReactDOM.createRoot(document.getElementById('app')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);