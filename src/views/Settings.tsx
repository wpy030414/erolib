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
import { BrandIcon } from '@/components/BrandIcon';
import { UpdateDialog, type UpdateDialogHandle } from '@/components/UpdateDialog';
import { M3eTabs, M3eTab } from '@m3e/react/tabs';
import { M3eSwitch } from '@m3e/react/switch';
import { M3eSelect } from '@m3e/react/select';
import { M3eOption } from '@m3e/react/option';
import { M3eFormField } from '@m3e/react/form-field';
import { M3eButton } from '@m3e/react/button';
import { M3eDialog } from '@m3e/react/dialog';
import {
  mdiBroom, mdiCheckCircle, mdiDatabaseOutline, mdiDeleteForever,
  mdiFolderSyncOutline, mdiGithub, mdiPalette, mdiPlay, mdiRss, mdiStop,
  mdiTranslate, mdiWeb, mdiClose,
} from '@mdi/js';

const GITHUB_URL = 'https://github.com/wpy030414/erolib';
const BILIBILI_URL = 'https://space.bilibili.com/92465406';

const BILIBILI_PATH =
  'M4.977 3.561a1.31 1.31 0 111.818-1.884l2.828 2.728c.08.078.149.163.205.254h4.277a1.32 1.32 0 01.205-.254l2.828-2.728a1.31 1.31 0 011.818 1.884L17.82 4.66h.848A5.333 5.333 0 0124 9.992v7.34a5.333 5.333 0 01-5.333 5.334H5.333A5.333 5.333 0 010 17.333V9.992a5.333 5.333 0 015.333-5.333h.781L4.977 3.56zm.356 3.67a2.667 2.667 0 00-2.666 2.667v7.529a2.667 2.667 0 002.666 2.666h13.334a2.667 2.667 0 002.666-2.666v-7.53a2.667 2.667 0 00-2.666-2.666H5.333zm1.334 5.192a1.333 1.333 0 112.666 0v1.192a1.333 1.333 0 11-2.666 0v-1.192zM16 11.09c-.736 0-1.333.597-1.333 1.333v1.192a1.333 1.333 0 102.666 0v-1.192c0-.736-.597-1.333-1.333-1.333z';

/** md-outlined-text-field 语义对齐：字段外观由 M3eFormField 提供，内嵌原生 input 保持裸样式。 */
const bareInputStyle: React.CSSProperties = {
  background: 'transparent',
  border: 'none',
  outline: 'none',
  font: 'inherit',
  color: 'inherit',
  width: '100%',
  padding: 0,
} as const;

export default function Settings() {
  const { t, locale, setLocale } = useI18n();
  const themeStore = useThemeStore();
  const settingsStore = useSettingsStore();
  const updateStore = useUpdateStore();
  const toast = useToastStore();

  const [version, setVersion] = useState('0.1.0');
  const [tab, setTab] = useState(() => {
    try { return localStorage.getItem('erolib.settings.tab') === 'sharing' ? 'sharing' : 'basic'; } catch { return 'basic'; }
  });
  const [resetting, setResetting] = useState(false);
  const [clearingCache, setClearingCache] = useState(false);
  const [resetError, setResetError] = useState<string | null>(null);
  const [confirmInput, setConfirmInput] = useState('');
  const [showClearAll, setShowClearAll] = useState(false);
  const updateDialogRef = useRef<UpdateDialogHandle>(null);

  const confirmPhrase = t('settings.reset.confirmPhrase');
  const confirmMatched = confirmInput.trim() === confirmPhrase;

  const customThemeList = Array.from(themeStore.customThemes.values());
  const syncDirName = (() => {
    const parts = settingsStore.syncDir.replace(/\\/g, '/').split('/').filter(Boolean);
    return parts.length ? parts[parts.length - 1] : '';
  })();

  useEffect(() => {
    void getVersion().then((v) => setVersion(v)).catch(() => {});
    void updateStore.check().catch(() => {});
  }, []);

  useEffect(() => {
    try { localStorage.setItem('erolib.settings.tab', tab); } catch { /* ignore */ }
  }, [tab]);

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

  async function pickSyncDir() {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected === 'string' && selected) {
      settingsStore.setSyncDir(selected);
      await settingsStore.syncNow();
    }
  }

  return (
    <div className="pa-6">
      {/* About cards */}
      <div className="about-row mb-6">
        <div className="about-card">
          <div className="md3-card__header-titles">
            <span className="md3-card__title">{t('settings.projectName')}</span>
            <span className="md3-card__subtitle version-line">
              v{version}
              {updateStore.info?.hasUpdate && (
                <span
                  className="update-badge"
                  title={t('settings.update.hasUpdate', { version: updateStore.info.latest })}
                  onClick={() => updateDialogRef.current?.open()}
                >
                  <span className="update-dot" /> {t('settings.update.hasUpdate', { version: updateStore.info.latest })}
                </span>
              )}
              {updateStore.info && !updateStore.info.hasUpdate && (
                <span className="update-badge update-badge--up-to-date" title={t('settings.update.upToDate')}>
                  <span className="update-dot update-dot--success" /> {t('settings.update.upToDate')}
                </span>
              )}
            </span>
          </div>
          <a href={GITHUB_URL} target="_blank" rel="noreferrer" className="md3-card__header-action"><MdiIcon path={mdiGithub} size={22} /></a>
        </div>
        <div className="about-card">
          <div className="md3-card__header-titles">
            <span className="md3-card__title">{t('settings.authorName')}</span>
            <span className="md3-card__subtitle">&ldquo;Do one thing, and do it well.&rdquo;</span>
          </div>
          <a href={BILIBILI_URL} target="_blank" rel="noreferrer" className="md3-card__header-action">
            <BrandIcon path={BILIBILI_PATH} fillRule="evenodd" size={22} brand />
          </a>
        </div>
      </div>

      {/* Tabs — md-tabs 语义对齐：活动指示条滑动切换 */}
      <M3eTabs
        className="mb-4"
        variant="primary"
        onChange={(e) => {
          const i = (e.currentTarget as unknown as { selectedIndex: number }).selectedIndex;
          if (i === 0) setTab('basic'); else if (i === 1) setTab('sharing');
        }}
      >
        <M3eTab selected={tab === 'basic'}>{t('settings.tab.basic')}</M3eTab>
        <M3eTab selected={tab === 'sharing'}>{t('settings.tab.sharing')}</M3eTab>
      </M3eTabs>

      {/* Basic tab */}
      {tab === 'basic' && (
        <>
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiTranslate} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.language')}</h3></div>
            <M3eFormField variant="outlined" style={{ maxWidth: 240 }}>
              <label slot="label" htmlFor="settings-language">{t('settings.language')}</label>
              <M3eSelect
                id="settings-language"
                onChange={(e) => setLocale((e.currentTarget as unknown as { value: string }).value as Locale)}
              >
                {LOCALES.map((l) => <M3eOption key={l} value={l} selected={locale === l}>{LOCALE_LABELS[l]}</M3eOption>)}
              </M3eSelect>
            </M3eFormField>
          </section>

          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiPalette} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.theme')}</h3></div>
            <p className="text-body-2 text-medium-emphasis mb-3">{t('settings.theme.seed')}</p>
            <div className="d-flex gap-3 mb-4">
              {themeStore.SEEDS.map((s) => (
                <button key={s.key} className={`theme-swatch${themeStore.seed === s.key ? ' theme-swatch--selected' : ''}`}
                  style={{ backgroundColor: s.color }} onClick={() => themeStore.setSeed(s.key)} />
              ))}
            </div>
            {customThemeList.length > 0 && (
              <>
                <p className="text-body-2 text-medium-emphasis mb-3">{t('settings.theme.custom')}</p>
                <div className="d-flex gap-3 mb-4 flex-wrap">
                  {customThemeList.map((ct) => (
                    <div key={ct.key} className={`custom-theme-item${themeStore.seed === ct.key ? ' custom-theme-item--selected' : ''}`}>
                      <div className="custom-theme-thumb" style={{ backgroundImage: `url(${ct.thumbnailB64})` }} title={ct.sourceTitle} onClick={() => themeStore.activateCustomTheme(ct.key)} />
                      {themeStore.seed !== ct.key && (
                        <button className="custom-theme-delete" aria-label={t('settings.theme.removeCustom')} onClick={() => themeStore.removeCustomTheme(ct.key)}>
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
              <M3eSwitch
                checked={themeStore.mode === 'dark'}
                onChange={(e) => themeStore.setMode((e.currentTarget as unknown as { checked: boolean }).checked ? 'dark' : 'light')}
              />
            </div>
          </section>

          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiDatabaseOutline} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.reset.title')}</h3></div>
            <div className="d-flex gap-3">
              <M3eButton variant="outlined" disabled={clearingCache} onClick={onClearCache}><MdiIcon path={mdiBroom} size={20} /> {t('settings.reset.clearCache')}</M3eButton>
              <M3eButton variant="filled" disabled={resetting} onClick={() => setShowClearAll(true)}><MdiIcon path={mdiDeleteForever} size={20} /> {resetting ? t('settings.reset.running') : t('settings.reset.clearAll')}</M3eButton>
            </div>
            {resetError && <p className="mt-3 text-body-2 text-error">{resetError}</p>}
          </section>
        </>
      )}

      {/* Sharing tab */}
      {tab === 'sharing' && (
        <>
          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiFolderSyncOutline} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.localSync')}</h3>
              <M3eSwitch
                style={{ marginLeft: 'auto' }}
                aria-label={t('settings.localSync')}
                checked={settingsStore.syncEnabled}
                onChange={(e) => { const on = (e.currentTarget as unknown as { checked: boolean }).checked; settingsStore.setSyncEnabled(on); if (on && settingsStore.syncDir) void settingsStore.syncNow(); }}
              />
            </div>
            <M3eFormField variant="outlined" style={{ width: 280 }}>
              <label slot="label" htmlFor="sync-path">{t('settings.localSync.path')}</label>
              <input id="sync-path" type="text" value={syncDirName} placeholder={t('settings.localSync.pathHint')} title={settingsStore.syncDir}
                readOnly disabled={!settingsStore.syncEnabled || settingsStore.syncBusy} onClick={pickSyncDir}
                style={{ ...bareInputStyle, cursor: 'pointer' }} />
            </M3eFormField>
            {settingsStore.syncStats && (<p className="mt-3 text-body-2 text-success d-flex align-center"><MdiIcon path={mdiCheckCircle} size={16} /> {t('settings.localSync.stats', settingsStore.syncStats)}</p>)}
            {settingsStore.syncBusy && <p className="mt-3 text-body-2 text-medium-emphasis">{t('settings.localSync.syncing')}</p>}
            {settingsStore.syncError && <p className="mt-3 text-body-2 text-error">{settingsStore.syncError}</p>}
          </section>

          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiWeb} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.opds')}</h3></div>
            <div className="d-flex align-start gap-4 flex-wrap">
              <M3eFormField variant="outlined" style={{ width: 140 }}>
                <label slot="label" htmlFor="opds-port">{t('settings.port')}</label>
                <input id="opds-port" type="number" value={settingsStore.opdsPort} disabled={settingsStore.opdsRunning || settingsStore.opdsBusy}
                  onChange={(e) => settingsStore.saveOpdsPort(e.target.value)} style={bareInputStyle} />
              </M3eFormField>
              {!settingsStore.opdsRunning ? (
                <M3eButton variant="filled" disabled={settingsStore.opdsBusy} onClick={() => settingsStore.toggleOpds()}><MdiIcon path={mdiPlay} size={20} /> {t('settings.start')}</M3eButton>
              ) : (
                <M3eButton variant="outlined" disabled={settingsStore.opdsBusy} onClick={() => settingsStore.toggleOpds()}><MdiIcon path={mdiStop} size={20} /> {t('settings.stop')}</M3eButton>
              )}
            </div>
            {settingsStore.opdsRunning && settingsStore.opdsUrl && (<p className="mt-3 text-body-2 text-success"><a href={`${settingsStore.opdsUrl}/opds`} target="_blank" rel="noreferrer">{settingsStore.opdsUrl}/opds</a></p>)}
            {settingsStore.opdsError && <p className="mt-3 text-body-2 text-error">{settingsStore.opdsError}</p>}
          </section>

          <section className="mb-6">
            <div className="d-flex align-center mb-2"><MdiIcon path={mdiRss} size={22} /><h3 className="text-h6" style={{ margin: '0 0 0 8px' }}>{t('settings.rss')}</h3></div>
            <div className="d-flex align-start gap-4 flex-wrap">
              <M3eFormField variant="outlined" style={{ width: 140 }}>
                <label slot="label" htmlFor="rss-port">{t('settings.port')}</label>
                <input id="rss-port" type="number" value={settingsStore.rssPort} disabled={settingsStore.rssRunning || settingsStore.rssBusy}
                  onChange={(e) => settingsStore.saveRssPort(e.target.value)} style={bareInputStyle} />
              </M3eFormField>
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

      {/* Update check / download / install flow (opened from the badge). */}
      <UpdateDialog ref={updateDialogRef} />

      {/* Clear all confirmation dialog — md-dialog 语义对齐：常驻挂载 + open 切换，
          开/合动画由 m3e-dialog 内置（含 Esc/cancel 路径）。 */}
      <M3eDialog open={showClearAll} onClosed={() => setShowClearAll(false)}>
        <div slot="headline">{t('settings.reset.clearAll')}</div>
        <div slot="content">
          <p className="text-body-2 text-error">{t('settings.reset.confirmWarn')}</p>
          <M3eFormField variant="outlined">
            <label slot="label">{t('settings.reset.typeConfirm', { phrase: confirmPhrase })}</label>
            <input type="text" value={confirmInput} onChange={(e) => setConfirmInput(e.target.value)} style={bareInputStyle} />
          </M3eFormField>
        </div>
        <div slot="actions">
          <M3eButton variant="text" onClick={() => setShowClearAll(false)}>{t('common.cancel')}</M3eButton>
          <M3eButton variant="filled" disabled={!confirmMatched} onClick={doClearAll}>{t('settings.reset.clearAll')}</M3eButton>
        </div>
      </M3eDialog>
    </div>
  );
}