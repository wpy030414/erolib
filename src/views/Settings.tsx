import { useEffect, useState, useRef } from 'react';
import { useI18n, LOCALES, LOCALE_LABELS, type Locale } from '@/hooks/useI18n';
import { useThemeStore } from '@/stores/theme';
import { useSettingsStore } from '@/stores/settings';
import { useUpdateStore } from '@/stores/update';
import { useToastStore } from '@/stores/toast';
import { getVersion } from '@tauri-apps/api/app';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { clearThumbs } from '@/services/thumb-cache';
import { api } from '@/services/api';
import { MdiIcon } from '@/components/MdiIcon';
import { UpdateDialog, type UpdateDialogHandle } from '@/components/UpdateDialog';
import { M3eTabs } from '@m3e/react/tabs';
import { M3eSwitch } from '@m3e/react/switch';

import { M3eSelect } from '@m3e/react/select';
import { M3eButton } from '@m3e/react/button';
import { M3eDialog } from '@m3e/react/dialog';
import {
  mdiBroom, mdiCheckCircle, mdiClose, mdiDatabaseOutline, mdiDeleteForever,
  mdiFolderSyncOutline, mdiGithub, mdiPalette, mdiPlay, mdiRss, mdiStop,
  mdiTranslate, mdiWeb,
} from '@mdi/js';

const GITHUB_URL = 'https://github.com/wpy030414/erolib';
const BILIBILI_URL = 'https://space.bilibili.com/92465406';

export default function Settings() {
  const { t, locale, setLocale } = useI18n();
  const themeStore = useThemeStore();
  const settingsStore = useSettingsStore();
  const updateStore = useUpdateStore();
  const toast = useToastStore();
  const updateDialogRef = useRef<UpdateDialogHandle>(null);
  const [version, setVersion] = useState('0.1.0');
  const [tab, setTab] = useState(0);
  const [resetting, setResetting] = useState(false);
  const [clearingCache, setClearingCache] = useState(false);
  const [resetError, setResetError] = useState<string | null>(null);
  const [confirmInput, setConfirmInput] = useState('');
  const [clearAllOpen, setClearAllOpen] = useState(false);
  const confirmPhrase = t('settings.reset.confirmPhrase');
  const confirmMatched = confirmInput.trim() === confirmPhrase;

  useEffect(() => {
    void getVersion().then((v) => setVersion(v)).catch(() => {});
    void updateStore.check().catch(() => {});
  }, []);

  async function onClearCache() {
    setClearingCache(true); setResetError(null);
    try { await clearThumbs(); toast.addToast('success', t('settings.reset.cacheToast')); }
    catch (e) { setResetError(String(e)); }
    finally { setClearingCache(false); }
  }

  async function doClearAll() {
    setResetting(true); setResetError(null);
    try {
      await settingsStore.stopOpds(); await settingsStore.stopRss();
      await api.resetAppData(); await clearThumbs();
      const keysToRemove: string[] = [];
      for (let i = 0; i < window.localStorage.length; i++) {
        const key = window.localStorage.key(i);
        if (key?.startsWith('erolib.')) keysToRemove.push(key);
      }
      keysToRemove.forEach((key) => window.localStorage.removeItem(key));
      settingsStore.reset();
      toast.addToast('success', t('settings.reset.toast'));
      window.location.reload();
    } catch (e) { setResetError(String(e)); setResetting(false); }
  }

  const customThemeList = Array.from(themeStore.customThemes.values());
  const syncDirName = (() => {
    const parts = settingsStore.syncDir.replace(/\\/g, '/').split('/').filter(Boolean);
    return parts.length ? parts[parts.length - 1] : '';
  })();

  return (
    <div className="pa-6">
      {/* About cards */}
      <div className="about-row mb-6">
        <div className="md3-card md3-card--outlined about-card">
          <div className="md3-card__header-titles">
            <span className="md3-card__title">{t('settings.projectName')}</span>
            <span className="md3-card__subtitle version-line">
              v{version}
              {updateStore.info?.hasUpdate && (
                <span className="update-badge" onClick={() => updateDialogRef.current?.open()}>
                  <span className="update-dot" /> {t('settings.update.hasUpdate', { version: updateStore.info.latest })}
                </span>
              )}
              {updateStore.info && !updateStore.info.hasUpdate && (
                <span className="update-badge update-badge--up-to-date">
                  <span className="update-dot update-dot--success" /> {t('settings.update.upToDate')}
                </span>
              )}
            </span>
          </div>
          <a href={GITHUB_URL} target="_blank" rel="noreferrer" className="md3-card__header-action"><MdiIcon path={mdiGithub} size={22} /></a>
        </div>
        <div className="md3-card md3-card--outlined about-card">
          <div className="md3-card__header-titles">
            <span className="md3-card__title">{t('settings.authorName')}</span>
            <span className="md3-card__subtitle">&ldquo;Do one thing, and do it well.&rdquo;</span>
          </div>
          <a href={BILIBILI_URL} target="_blank" rel="noreferrer" className="md3-card__header-action">
            <MdiIcon path={mdiGithub} size={22} />
          </a>
        </div>
      </div>

      {/* Tabs */}
      <M3eTabs style={{ marginBottom: 16 }}>
        <M3eTabs.Tab onClick={() => setTab(0)} active={tab === 0}>{t('settings.tab.basic')}</M3eTabs.Tab>
        <M3eTabs.Tab onClick={() => setTab(1)} active={tab === 1}>{t('settings.tab.sharing')}</M3eTabs.Tab>
      </M3eTabs>

      {/* Basic tab */}
      {tab === 0 && (
        <>
          {/* Language */}
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiTranslate} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.language')}</h3></div>
            <M3eSelect value={locale} onChange={(e: any) => setLocale(e.target.value as Locale)} style={{ maxWidth: 240 }}>
              {LOCALES.map((l) => <option key={l} value={l}>{LOCALE_LABELS[l]}</option>)}
            </M3eSelect>
          </section>

          {/* Theme */}
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiPalette} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.theme')}</h3></div>
            <p className="text-body-2 text-medium-emphasis mb-3">{t('settings.theme.seed')}</p>
            <div className="d-flex gap-3 mb-4">
              {themeStore.SEEDS.map((s) => (
                <button key={s.key}
                  className={`theme-swatch${themeStore.seed === s.key ? ' theme-swatch--selected' : ''}`}
                  style={{ backgroundColor: s.color }}
                  onClick={() => themeStore.setSeed(s.key)}
                />
              ))}
            </div>
            {customThemeList.length > 0 && (
              <>
                <p className="text-body-2 text-medium-emphasis mb-3">{t('settings.theme.custom')}</p>
                <div className="d-flex gap-3 mb-4 flex-wrap">
                  {customThemeList.map((ct) => (
                    <div key={ct.key} className={`custom-theme-item${themeStore.seed === ct.key ? ' custom-theme-item--selected' : ''}`}>
                      <div className="custom-theme-thumb" style={{ backgroundImage: `url(${ct.thumbnailB64})` }} onClick={() => themeStore.activateCustomTheme(ct.key)} />
                      {themeStore.seed !== ct.key && (
                        <button className="custom-theme-delete" onClick={() => themeStore.removeCustomTheme(ct.key)}>
                          <svg width={12} height={12} viewBox="0 0 24 24" fill="currentColor"><path d={mdiClose} /></svg>
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              </>
            )}
            <div className="d-flex align-center">
              <span className="dark-mode-label">{t('settings.theme.dark')}</span>
              <M3eSwitch selected={themeStore.mode === 'dark'} onChange={(e: any) => themeStore.setMode(e.target.selected ? 'dark' : 'light')} />
            </div>
          </section>

          {/* Reset */}
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiDatabaseOutline} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.reset.title')}</h3></div>
            <div className="d-flex gap-3">
              <M3eButton variant="outlined" disabled={clearingCache} onClick={onClearCache}>
                <MdiIcon path={mdiBroom} size={20} /> {t('settings.reset.clearCache')}
              </M3eButton>
              <M3eButton variant="filled" disabled={resetting} onClick={() => { setConfirmInput(''); setClearAllOpen(true); }}>
                <MdiIcon path={mdiDeleteForever} size={20} /> {resetting ? t('settings.reset.running') : t('settings.reset.clearAll')}
              </M3eButton>
            </div>
            {resetError && <p className="mt-3 text-body-2 text-error">{resetError}</p>}
          </section>
        </>
      )}

      {/* Sharing tab */}
      {tab === 1 && (
        <>
          {/* Local sync */}
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiFolderSyncOutline} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.localSync')}</h3>
              <div style={{ marginLeft: 'auto' }}><M3eSwitch selected={settingsStore.syncEnabled} onChange={(e: any) => settingsStore.setSyncEnabled(e.target.selected)} /></div>
            </div>
            <M3eTextField
              value={syncDirName}
              label={t('settings.localSync.path')}
              placeholder={t('settings.localSync.pathHint')}
              title={settingsStore.syncDir}
              disabled={!settingsStore.syncEnabled || settingsStore.syncBusy}
              onClick={async () => {
                const selected = await openDialog({ directory: true, multiple: false });
                if (typeof selected === 'string' && selected) { settingsStore.setSyncDir(selected); }
              }}
              style={{ width: 280, cursor: 'pointer' }}
            />
            {settingsStore.syncStats && (<p className="mt-3 text-body-2 text-success d-flex align-center"><MdiIcon path={mdiCheckCircle} size={16} /> {t('settings.localSync.stats', settingsStore.syncStats)}</p>)}
            {settingsStore.syncBusy && <p className="mt-3 text-body-2 text-medium-emphasis">{t('settings.localSync.syncing')}</p>}
            {settingsStore.syncError && <p className="mt-3 text-body-2 text-error">{settingsStore.syncError}</p>}
          </section>

          {/* OPDS */}
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiWeb} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.opds')}</h3></div>
            <div className="d-flex gap-4 flex-wrap">
              <M3eTextField value={settingsStore.opdsPort} type="number" label={t('settings.port')} disabled={settingsStore.opdsRunning || settingsStore.opdsBusy} onChange={(e: any) => settingsStore.saveOpdsPort(e.target.value)} style={{ width: 140 }} />
              {!settingsStore.opdsRunning ? (
                <M3eButton variant="filled" disabled={settingsStore.opdsBusy} onClick={() => settingsStore.toggleOpds()}><MdiIcon path={mdiPlay} size={20} /> {t('settings.start')}</M3eButton>
              ) : (
                <M3eButton variant="outlined" disabled={settingsStore.opdsBusy} onClick={() => settingsStore.toggleOpds()}><MdiIcon path={mdiStop} size={20} /> {t('settings.stop')}</M3eButton>
              )}
            </div>
            {settingsStore.opdsRunning && settingsStore.opdsUrl && (<p className="mt-3 text-body-2 text-success"><a href={`${settingsStore.opdsUrl}/opds`} target="_blank" rel="noreferrer">{settingsStore.opdsUrl}/opds</a></p>)}
            {settingsStore.opdsError && <p className="mt-3 text-body-2 text-error">{settingsStore.opdsError}</p>}
          </section>

          {/* RSS */}
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiRss} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.rss')}</h3></div>
            <div className="d-flex gap-4 flex-wrap">
              <M3eTextField value={settingsStore.rssPort} type="number" label={t('settings.port')} disabled={settingsStore.rssRunning || settingsStore.rssBusy} onChange={(e: any) => settingsStore.saveRssPort(e.target.value)} style={{ width: 140 }} />
              {!settingsStore.rssRunning ? (
                <M3eButton variant="filled" disabled={settingsStore.rssBusy} onClick={() => settingsStore.toggleRss()}><MdiIcon path={mdiPlay} size={20} /> {t('settings.start')}</M3eButton>
              ) : (
                <M3eButton variant="outlined" disabled={settingsStore.rssBusy} onClick={() => settingsStore.toggleRss()}><MdiIcon path={mdiStop} size={20} /> {t('settings.stop')}</M3eButton>
              )}
            </div>
            {settingsStore.rssRunning && settingsStore.rssUrl && (<p className="mt-3 text-body-2 text-success"><a href={`${settingsStore.rssUrl}/rss`} target="_blank" rel="noreferrer">{settingsStore.rssUrl}/rss</a></p>)}
            {settingsStore.rssError && <p className="mt-3 text-body-2 text-error">{settingsStore.rssError}</p>}
          </section>
        </>
      )}

      {/* Reset confirmation dialog */}
      <M3eDialog open={clearAllOpen} onClosed={() => { if (confirmMatched) doClearAll(); setClearAllOpen(false); }}>
        <div slot="headline">{t('settings.reset.clearAll')}</div>
        <div slot="content" className="clear-all-dialog__content">
          <p className="text-body-2 text-error">{t('settings.reset.confirmWarn')}</p>
          <M3eTextField label={t('settings.reset.typeConfirm', { phrase: confirmPhrase })} value={confirmInput} onChange={(e: any) => setConfirmInput(e.target.value)} style={{ width: '100%' }} />
        </div>
        <div slot="actions">
          <M3eButton variant="text" onClick={() => setClearAllOpen(false)}>{t('common.cancel')}</M3eButton>
          <M3eButton variant="filled" disabled={!confirmMatched} onClick={() => { if (confirmMatched) { doClearAll(); setClearAllOpen(false); } }}>{t('settings.reset.clearAll')}</M3eButton>
        </div>
      </M3eDialog>

      <UpdateDialog ref={updateDialogRef} />
    </div>
  );
}