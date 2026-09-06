import { forwardRef, useImperativeHandle, useRef, useState } from 'react';
import { mdiCheckCircle } from '@mdi/js';
import { useI18n } from '@/hooks/useI18n';
import { useUpdateStore } from '@/stores/update';
import { formatSpeed } from '@/utils/format';
import { MdiIcon } from './MdiIcon';
import { M3eDialog } from '@m3e/react/dialog';
import { M3eButton } from '@m3e/react/button';
import { M3eCircularProgressIndicator, M3eLinearProgressIndicator } from '@m3e/react/progress-indicator';

export interface UpdateDialogHandle {
  open: () => void;
}

export const UpdateDialog = forwardRef<UpdateDialogHandle>((_, ref) => {
  const { t } = useI18n();
  const store = useUpdateStore();
  const [open, setOpen] = useState(false);

  useImperativeHandle(ref, () => ({
    open: () => setOpen(true),
  }), []);

  function closeDialog() {
    setOpen(false);
  }

  const info = store.info;
  const hasUpdate = info?.hasUpdate;
  const hasDownload = !!store.downloadPath;

  return (
    <M3eDialog open={open} onClosed={closeDialog}>
      <div slot="headline">{t('settings.update.title')}</div>
      <div slot="content" className="update-dialog__content">
        {/* Checking */}
        {store.checking && (
          <div className="update-dialog__center">
            <M3eCircularProgressIndicator indeterminate />
            <p className="text-body-2 mt-2">{t('settings.update.checking')}</p>
          </div>
        )}

        {/* Error — v-else-if 链语义对齐：错误优先于结果展示（下载失败后
            hasUpdate 仍为 true，重开对话框也必须看到错误详情而非版本信息） */}
        {!store.checking && store.error && (
          <p className="text-body-2 text-error">
            {t('settings.update.checkFailed', { error: store.error })}
          </p>
        )}

        {/* Result */}
        {!store.checking && !store.error && info && (
          <>
            <p className="text-body-2 mb-1">
              {t('settings.update.current')} <b>v{info.current}</b>
            </p>
            <p className="text-body-2 mb-3">
              {t('settings.update.latest')} <b>v{info.latest}</b>
            </p>

            {/* Up to date */}
            {!hasUpdate && (
              <p className="text-body-2 text-success d-flex align-center">
                <MdiIcon path={mdiCheckCircle} size={18} />
                <span className="ml-2">{t('settings.update.upToDate')}</span>
              </p>
            )}

            {/* Has update */}
            {hasUpdate && (
              <>
                {info.notes && (
                  <div className="update-dialog__notes text-body-2">{info.notes}</div>
                )}

                {/* Downloading progress */}
                {store.downloading && (
                  <div className="mt-4">
                    <M3eLinearProgressIndicator value={store.progress.percent / 100} />
                    <p className="text-body-2 text-medium-emphasis mt-1">
                      {store.progress.percent}% · {formatSpeed(store.progress.speed, t)}
                    </p>
                  </div>
                )}

                {/* Downloaded, ready to install */}
                {hasDownload && !store.downloading && (
                  <p className="mt-3 text-body-2 text-success d-flex align-center">
                    <MdiIcon path={mdiCheckCircle} size={18} />
                    <span className="ml-2">{t('settings.update.downloadComplete')}</span>
                  </p>
                )}
              </>
            )}
          </>
        )}
      </div>
      <div slot="actions">
        {/* No update / checking / error → single dismiss button */}
        {!hasUpdate && (
          <M3eButton variant="text" onClick={closeDialog}>
            {t('common.confirm')}
          </M3eButton>
        )}

        {/* Has update: before download */}
        {hasUpdate && !hasDownload && (
          <>
            <M3eButton variant="text" onClick={closeDialog}>
              {t('common.cancel')}
            </M3eButton>
            <M3eButton variant="filled" disabled={store.downloading} onClick={() => store.download()}>
              {store.downloading ? t('settings.update.downloading') : t('settings.update.download')}
            </M3eButton>
          </>
        )}

        {/* Downloaded: install options */}
        {hasUpdate && hasDownload && (
          <>
            <M3eButton variant="outlined" onClick={() => store.install()}>
              {t('settings.update.openInstaller')}
            </M3eButton>
            <M3eButton variant="filled" onClick={() => store.quitAndInstall()}>
              {t('settings.update.quitAndInstall')}
            </M3eButton>
          </>
        )}
      </div>
    </M3eDialog>
  );
});