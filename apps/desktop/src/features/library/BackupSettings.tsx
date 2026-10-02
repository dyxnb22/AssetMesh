import React, { useCallback, useEffect, useState } from 'react';
import { localeTag, useT } from '../../i18n';
import { getTransport, normalizeDesktopError } from './transport';
import type { BackupEntry, BackupStatus } from './types';
import { captureBackupPreferences } from './backup-preferences';
import './Settings.css';

const reasonLabels: Record<string, string> = {
  automatic: 'Automatic snapshot', manual: 'Manual snapshot',
  before_import: 'Before bulk import', before_merge: 'Before asset merge',
  before_history_cleanup: 'Before routine history cleanup',
  before_migration: 'Before database upgrade', before_restore: 'Before restore',
  weekly_export: 'Weekly portable export', portable_export: 'Portable export',
};

function storageSize(bytes: number): string {
  return `${new Intl.NumberFormat(localeTag(), { maximumFractionDigits: 1 }).format(bytes / 1_048_576)} MB`;
}

export const BackupSettings: React.FC<{ recoveryOnly?: boolean }> = ({ recoveryOnly = false }) => {
  const t = useT();
  const transport = getTransport();
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [source, setSource] = useState('');
  const [preview, setPreview] = useState<BackupEntry | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [recoveryOpen, setRecoveryOpen] = useState(recoveryOnly);

  const refresh = useCallback(async () => {
    try {
      setStatus(await transport.backupStatus());
      setError('');
    } catch (reason) { setError(normalizeDesktopError(reason).message); }
  }, [transport]);
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 60_000);
    return () => clearInterval(timer);
  }, [refresh]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    setNotice('');
    try { await action(); await refresh(); }
    catch (reason) { setError(normalizeDesktopError(reason).message); }
    finally { setBusy(false); }
  };
  const inspect = (path: string) => run(async () => {
    setSource(path);
    setPreview(null);
    setConfirmed(false);
    setPreview(await transport.backupPreview(path));
  });
  const disabled = busy || status?.restore_pending;

  return (
    <section data-testid="backup-settings" className="settings-backup">
      <h3>{t('Backup & Recovery')}</h3>
      {!recoveryOnly && <p className="settings-muted">{t('Automatic backups are on. Older copies are cleaned up automatically.')}</p>}
      {status ? (
        <div className="settings-backup-status">
          <p data-testid="backup-last-success">
            {t('Last successful backup:')} {' '}
            {status.entries[0] ? new Date(status.entries[0].created_at).toLocaleString(localeTag()) : t('No backup yet')}
          </p>
          <span className="settings-muted" data-testid="backup-storage-usage">
            {t('{count} copies · {size}', { count: status.entries.length, size: storageSize(status.storage_bytes) })}
          </span>
        </div>
      ) : !error && <p className="settings-muted" role="status">{t('Loading backups…')}</p>}
      {status?.last_error && <p role="alert" className="settings-error">{t('Automatic backup failed:')} {status.last_error}</p>}
      {!!status?.issues.length && <p role="status" className="settings-muted">
        {t('{count} backups could not be listed. Other backups remain available; the affected folders have been preserved.', { count: status.issues.length })}
      </p>}
      {status && status.storage_bytes > status.budget_bytes && <p role="status" className="settings-muted">
        {t('Protected recovery points and unmanaged files are preserved even if they exceed the storage budget.')}
      </p>}
      {status?.entries.some((entry) => entry.contains_api_keys) && <p data-testid="backup-sensitive-notice" className="settings-muted">
        {t('These backups contain API key values and are not encrypted. Keep external copies on encrypted storage.')}
      </p>}
      {status?.restore_pending && <p role="status" className="settings-notice">{t('Recovery is ready. Quit and reopen AssetMesh to use the restored library. The previous library is preserved.')}</p>}
      {error && <div role="alert" className="settings-error">
        {error} <button type="button" className="native-button" disabled={busy} onClick={() => void refresh()}>{t('Try again')}</button>
      </div>}
      {notice && <p role="status" className="settings-notice">{notice}</p>}

      {!recoveryOnly && <div className="settings-backup-actions">
        <button type="button" className="native-button" disabled={disabled} onClick={() => void run(async () => {
          const target = await transport.pickDirectory(t('Choose a folder for an external backup copy'));
          if (target) {
            await transport.backupSavePreferences(captureBackupPreferences());
            const saved = await transport.backupExportCopy(target);
            setNotice(t('Backup copy saved to {path}', { path: saved }));
          }
        })}>{t('Save a backup copy…')}</button>
        <span className="settings-muted">{t('Save to another disk to protect against device loss.')}</span>
      </div>}

      <details className="settings-disclosure" open={recoveryOpen} onToggle={(event) => setRecoveryOpen(event.currentTarget.open)}>
        <summary>{t('Restore backup')}</summary>
        <div className="settings-disclosure-body">
          <button type="button" className="native-button" disabled={disabled} onClick={() => void run(async () => {
            const path = await transport.pickDirectory(t('Choose an AssetMesh backup folder'));
            if (path) {
              setSource(path); setPreview(null); setConfirmed(false);
              setPreview(await transport.backupPreview(path));
            }
          })}>{t('Choose backup to restore')}</button>
          {preview && !status?.restore_pending && <div data-testid="backup-restore-preview" className="settings-restore-preview">
            <strong>{t('Restore preview')}</strong>
            <p>{new Date(preview.created_at).toLocaleString(localeTag())} · {t('{count} assets', { count: preview.asset_count })}</p>
            <p className="settings-muted">{t('A new library will be created. Your current library and backup will be preserved. Saved filters, theme and language will be restored after reopening the app.')}</p>
            {preview.contains_api_keys && <p>{t('This backup includes API key values.')}</p>}
            <label><input type="checkbox" checked={confirmed} disabled={busy} onChange={(event) => setConfirmed(event.target.checked)} /> {t('Use this backup when I reopen AssetMesh')}</label>
            <div className="settings-backup-actions">
              <button type="button" className="native-button" disabled={!confirmed || busy || source !== preview.source_dir} onClick={() => void run(async () => {
                await transport.backupRestore(preview.source_dir, preview.fingerprint);
                setPreview(null); setConfirmed(false);
                window.dispatchEvent(new Event('assetmesh-restore-pending'));
              })}>{t('Restore into a new library')}</button>
            </div>
          </div>}
          <div className="settings-backup-history">
            {status?.entries.map((entry) => <div key={entry.id} className="settings-backup-entry">
              <div>
                <time dateTime={entry.created_at}>{new Date(entry.created_at).toLocaleString(localeTag())}</time>
                <span className="settings-muted">{t(reasonLabels[entry.reason] ?? entry.reason)} · {t('{count} assets', { count: entry.asset_count })}</span>
              </div>
              <button type="button" className="native-button" disabled={disabled} onClick={() => void inspect(entry.source_dir)}>{t('Preview restore')}</button>
            </div>)}
          </div>
        </div>
      </details>

      {!recoveryOnly && <details className="settings-disclosure settings-backup-details">
        <summary>{t('Backup details')}</summary>
        <div className="settings-disclosure-body settings-muted">
          <p>{t('Changes are backed up every 30 minutes and on normal exit. Unchanged data does not create extra copies.')}</p>
          <p>{t('Keep 5 recent snapshots, one per day for 7 days, 3 operation backups and 2 portable copies, within a 256 MB budget. The latest recovery points are always kept.')}</p>
          <p>{t('Routine media history keeps up to 10000 entries from the last 180 days. Cleanup requires a recovery backup; imports, merges and other significant history are preserved.')}</p>
          {status && <p className="settings-backup-path">{t('Backup folder:')} {status.directory}</p>}
          {status?.issues.map((issue) => <p key={issue.source_dir} className="settings-backup-path">
            {issue.source_dir}: {issue.message}
          </p>)}
          <button type="button" className="native-button" disabled={disabled} onClick={() => void run(async () => {
            await transport.backupSavePreferences(captureBackupPreferences());
            await transport.backupCreate();
            setNotice(t('Backup completed.'));
          })}>{t('Back up now')}</button>
        </div>
      </details>}
    </section>
  );
};
