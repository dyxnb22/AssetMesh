import { t } from '../../../i18n';
import type { useExportBundle } from './useExportBundle';

export function ExportWorkspace({ workflow }: { workflow: ReturnType<typeof useExportBundle> }) {
  const { includeApiKeys, setIncludeApiKeys, exportDir, setExportDir, isExporting, exportReceipt, exportError, handleBrowseExport, handleExecuteExport } = workflow;
  return (
    <div className="portable-workspace">
      <div
        className="portable-card"
      >
        <h3
          className="portable-section-title"
        >{t('Export Portable Bundle')}</h3>
        <p className="portable-description">{t('Select a target directory for the portable library bundle. Assets, module details, relations and history are exported according to the choice below.')}</p>

        <p style={{ fontSize: '13px', color: 'var(--color-muted)' }}>{t('API key assets and their related data are excluded by default. Recovery backups always preserve the full library.')}</p>
        <label style={{ fontSize: '13px' }}><input type="checkbox" checked={includeApiKeys} disabled={isExporting} onChange={(event) => setIncludeApiKeys(event.target.checked)} /> {t('Include API key values (not encrypted)')}</label>
        {includeApiKeys && <p role="status" style={{ fontSize: '13px', color: 'var(--color-muted)' }}>{t('Keep exports containing API keys on encrypted storage.')}</p>}
        {/* Target Destination Directory */}
        <div className="portable-directory">
          <input
            data-testid="export-dir-input"
            type="text"
            aria-label={t('Target directory')}
            placeholder={t('/path/to/destination-folder')}
            value={exportDir}
            onChange={(e) => setExportDir(e.target.value)}
            className="portable-directory-input"
          />
          <button
            data-testid="export-browse-button"
            onClick={handleBrowseExport}
            className="portable-browse"
          >{t('Browse...')}</button>
          <button
            data-testid="execute-export-button"
            disabled={isExporting || !exportDir.trim()}
            onClick={handleExecuteExport}
            style={{
              cursor: isExporting || !exportDir.trim() ? 'not-allowed' : 'pointer',
              opacity: isExporting || !exportDir.trim() ? 0.6 : 1,
              padding: '8px 18px',
              fontSize: '12px',
              fontWeight: 600,
              backgroundColor: 'var(--color-mesh)',
              color: '#ffffff',
              border: 'none',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            {isExporting ? t('Exporting...') : t('Export Bundle')}
          </button>
        </div>

        {/* Error Notice */}
        {exportError && (
          <div
            data-testid="export-error-banner"
            className="portable-error"
          >
            <strong>{t('Export Error:')}</strong> {exportError}
          </div>
        )}
      </div>

      {/* Export Receipt */}
      {exportReceipt && (
        <div
          data-testid="export-receipt"
          className="portable-card portable-card-success"
        >
          <div
            className="portable-receipt-heading"
          >
            <span
              className="portable-receipt-badge"
            >{t('✓ Export Complete')}</span>
            <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>{t('Receipt: ')}{exportReceipt.target_dir}
            </span>
          </div>

          <div
            className="portable-metrics"
          >
            <div
              style={{
                padding: '8px 12px',
                backgroundColor: 'var(--color-canvas)',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{t('Format & Version')}</div>
              <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                {exportReceipt.format} (v{exportReceipt.version})
              </div>
            </div>
            <div
              style={{
                padding: '8px 12px',
                backgroundColor: 'var(--color-canvas)',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{t('Created At')}</div>
              <div style={{ fontSize: '12px', fontFamily: 'monospace', color: 'var(--color-ink)' }}>
                {exportReceipt.created_at.slice(0, 19).replace('T', ' ')}
              </div>
            </div>
            <div
              style={{
                padding: '8px 12px',
                backgroundColor: 'var(--color-canvas)',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{t('App Version')}</div>
              <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                v{exportReceipt.app_version}
              </div>
            </div>
          </div>

          {/* Record Counts */}
          <div style={{ marginBottom: '16px' }}>
            <div
              style={{
                fontSize: '12px',
                fontWeight: 600,
                color: 'var(--color-muted)',
                marginBottom: '8px',
                textTransform: 'uppercase',
              }}
            >{t('Exported Record Breakdown')}</div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
              {Object.entries(exportReceipt.record_counts).map(([name, count]) => (
                <span
                  key={name}
                  className="portable-record-count"
                >
                  <strong style={{ textTransform: 'capitalize' }}>{name.replace('_', ' ')}:</strong>{' '}
                  {count}
                </span>
              ))}
            </div>
          </div>

          {/* Files written */}
          <div>
            <div
              style={{
                fontSize: '12px',
                fontWeight: 600,
                color: 'var(--color-muted)',
                marginBottom: '6px',
                textTransform: 'uppercase',
              }}
            >{t('Files Written (')}{exportReceipt.files.length})
            </div>
            <ul
              data-testid="export-files-list"
              style={{
                margin: 0,
                padding: '0 0 0 18px',
                fontSize: '12px',
                fontFamily: 'monospace',
                color: 'var(--color-muted)',
              }}
            >
              {exportReceipt.files.map((file) => (
                <li key={file}>{file}</li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}
