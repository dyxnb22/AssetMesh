import React from 'react';
import { Badge } from '../../../ui/Badge';
import type { SoftwareRecordDto } from '../types';
import { t } from '../../../i18n';

interface SoftwarePanelProps {
  record: SoftwareRecordDto;
  onEdit?: () => void;
}

export const SoftwarePanel: React.FC<SoftwarePanelProps> = ({ record, onEdit }) => {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '12px',
        backgroundColor: 'var(--color-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        padding: '14px',
      }}
      data-testid="software-panel"
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span style={{ fontSize: '11px', textTransform: 'uppercase', color: 'var(--color-muted)', fontWeight: 600 }}>{t('Software Details')}</span>
          <Badge variant="muted">{t(record.category)}</Badge>
        </div>
        {onEdit && (
          <button
            type="button"
            data-testid="edit-software-button"
            onClick={onEdit}
            style={{
              background: 'none',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              padding: '2px 8px',
              fontSize: '11px',
              cursor: 'pointer',
              color: 'var(--color-ink)',
            }}
          >{t('✎ Edit')}</button>
        )}
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: '10px', fontSize: '12px' }}>
        {record.version && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>{t('Version')}</div>
            <code style={{ fontSize: '12px', color: 'var(--color-ink)' }}>{record.version}</code>
          </div>
        )}

        {record.architecture && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>{t('Architecture')}</div>
            <div>{record.architecture}</div>
          </div>
        )}
      </div>

      {record.purpose && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>{t('Purpose')}</div>
          <p style={{ color: 'var(--color-ink)', fontWeight: 500 }}>{record.purpose}</p>
        </div>
      )}

      {(record.executable_path || record.install_location) && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          {record.executable_path && (
            <div style={{ marginBottom: '6px' }}>
              <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>{t('Executable Path')}</div>
              <code style={{ fontSize: '11px', wordBreak: 'break-all', backgroundColor: 'var(--color-canvas)', padding: '2px 4px', borderRadius: 'var(--radius-sm)' }}>
                {record.executable_path}
              </code>
            </div>
          )}
          {record.install_location && (
            <div>
              <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>{t('Install Location')}</div>
              <code style={{ fontSize: '11px', wordBreak: 'break-all', backgroundColor: 'var(--color-canvas)', padding: '2px 4px', borderRadius: 'var(--radius-sm)' }}>
                {record.install_location}
              </code>
            </div>
          )}
        </div>
      )}

      {record.notes && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>{t('Notes')}</div>
          <p style={{ whiteSpace: 'pre-wrap', color: 'var(--color-ink)' }}>{record.notes}</p>
        </div>
      )}

      {(record.installed_at || record.discovered_at) && (
        <div style={{ display: 'flex', gap: '16px', borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '11px', color: 'var(--color-muted)' }}>
          {record.installed_at && <div>{t('Installed: ')}{record.installed_at.slice(0, 10)}</div>}
          {record.discovered_at && <div>{t('Discovered: ')}{record.discovered_at.slice(0, 10)}</div>}
        </div>
      )}
    </div>
  );
};
