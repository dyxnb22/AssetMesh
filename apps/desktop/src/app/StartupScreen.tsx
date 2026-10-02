import { lazy } from 'react';
import { Badge } from '../ui/Badge';
import { t } from '../i18n';
import { DeferredFeature } from './DeferredFeature';
import type { AppStatus } from '../features/library/types';

const BackupSettings = lazy(() => import('../features/library/BackupSettings').then((m) => ({ default: m.BackupSettings })));

export function StartupScreen({ status, restorePending, setupBusy, retrySetup, chooseDatabaseFolder }: {
  status: AppStatus;
  restorePending: boolean;
  setupBusy: boolean;
  retrySetup: (dbPath?: string) => Promise<void>;
  chooseDatabaseFolder: () => Promise<void>;
}) {
  if (status.status === 'loading') {
    return (
      <div
        role="status"
        aria-live="polite"
        style={{
          display: 'flex',
          height: '100vh',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: 'var(--color-canvas)',
          color: 'var(--color-muted)',
        }}
      >
        <div style={{ textAlign: 'center' }}>
          <div
            style={{
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--color-ink)',
              marginBottom: '8px',
            }}
          >
            AssetMesh
          </div>
          <div>{t('Opening library and verifying state...')}</div>
        </div>
      </div>
    );
  }

  if (restorePending) {
    return <main style={{ padding: 40, maxWidth: 700, margin: 'auto' }}>
      <h2>{t('Recovery is ready')}</h2>
      <p>{t('Recovery is ready. Quit and reopen AssetMesh to use the restored library. The previous library is preserved.')}</p>
    </main>;
  }

  if (status.status === 'setup_failure') {
    return (
      <div
        role="alert"
        style={{
          display: 'flex',
          height: '100vh',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: 'var(--color-canvas)',
          padding: '24px',
        }}
      >
        <div
          style={{
            maxWidth: '480px',
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-lg)',
            padding: '24px',
            boxShadow: '0 4px 12px rgba(0,0,0,0.05)',
          }}
        >
          <Badge variant="attention" className="mb-2">
            {t('Setup Required')}
          </Badge>
          <h2
            style={{
              fontSize: '16px',
              fontWeight: 600,
              margin: '8px 0 12px',
              color: 'var(--color-ink)',
            }}
          >
            {t('Unable to initialize database')}
          </h2>
          <p
            style={{
              color: 'var(--color-muted)',
              marginBottom: '16px',
              wordBreak: 'break-word',
            }}
          >
            {t(status.message)}
          </p>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              onClick={() => retrySetup()}
              disabled={setupBusy}
              style={{
                padding: '6px 14px',
                backgroundColor: 'var(--color-mesh)',
                color: '#FFFFFF',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                cursor: 'pointer',
                fontWeight: 500,
              }}
            >
              {t('Retry')}
            </button>
            <button
              onClick={chooseDatabaseFolder}
              disabled={setupBusy}
              style={{
                padding: '6px 14px',
                backgroundColor: 'transparent',
                color: 'var(--color-ink)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                cursor: 'pointer',
                fontWeight: 500,
              }}
            >
              {t('Choose another folder…')}
            </button>
          </div>
          <DeferredFeature><BackupSettings recoveryOnly /></DeferredFeature>
        </div>
      </div>
    );
  }

  if (status.status === 'corrupt_failure') {
    return (
      <div
        role="alert"
        style={{
          display: 'flex',
          height: '100vh',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: 'var(--color-canvas)',
          padding: '24px',
        }}
      >
        <div
          style={{
            maxWidth: '480px',
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-lg)',
            padding: '24px',
            boxShadow: '0 4px 12px rgba(0,0,0,0.05)',
          }}
        >
          <Badge variant="danger" className="mb-2">
            {t('Corrupt Data')}
          </Badge>
          <h2
            style={{
              fontSize: '16px',
              fontWeight: 600,
              margin: '8px 0 12px',
              color: 'var(--color-danger)',
            }}
          >
            {t('Database Integrity Verification Failed')}
          </h2>
          <p
            style={{
              color: 'var(--color-muted)',
              marginBottom: '16px',
              wordBreak: 'break-word',
            }}
          >
            {t(status.message)}
          </p>
          <p style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
            {t(
              'AssetMesh refused to load this database because migrations or checksums do not match expected canonical definitions.'
            )}
          </p>
          <DeferredFeature><BackupSettings recoveryOnly /></DeferredFeature>
        </div>
      </div>
    );
  }

  return null;
}
