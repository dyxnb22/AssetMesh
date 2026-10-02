import { t } from '../../../i18n';
import { ImportDispositions } from './ImportDispositions';
import type { useImportBundle } from './useImportBundle';

export function ImportWorkspace({ workflow }: { workflow: ReturnType<typeof useImportBundle> }) {
  const { importDir, changeDirectory, isPreviewing, importPreview, importError, isApplying, importReceipt, previewedRequest, previewIsStale, handleBrowseImport, handlePreviewImport, handleApplyImport } = workflow;
  return (
    <div className="portable-workspace">
      <div
        className="portable-card"
      >
        <h3
          className="portable-section-title"
        >{t('Import Portable Bundle')}</h3>
        <p className="portable-description">{t('Select an existing portable bundle directory to import. Every bundle undergoes mandatory preflight inspection before any mutation can occur.')}</p>

        {/* Source Directory */}
        <div className="portable-directory">
          <input
            data-testid="import-dir-input"
            disabled={isApplying}
            type="text"
            aria-label={t('Source bundle directory')}
            placeholder={t('/path/to/exported-bundle')}
            value={importDir}
            onChange={(e) => changeDirectory(e.target.value)}
            className="portable-directory-input"
          />
          <button
            data-testid="import-browse-button"
            disabled={isApplying || isPreviewing}
            onClick={handleBrowseImport}
            className="portable-browse"
          >{t('Browse...')}</button>
          <button
            data-testid="preview-import-button"
            disabled={isApplying || isPreviewing || !importDir.trim()}
            onClick={() => void handlePreviewImport()}
            style={{
              cursor: isPreviewing || !importDir.trim() ? 'not-allowed' : 'pointer',
              opacity: isPreviewing || !importDir.trim() ? 0.6 : 1,
              padding: '8px 18px',
              fontSize: '12px',
              fontWeight: 600,
              backgroundColor: 'var(--color-surface)',
              color: 'var(--color-ink)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            {isPreviewing ? t('Inspecting...') : t('Preview Import')}
          </button>
        </div>

        {/* Error Banner */}
        {importError && (
          <div
            data-testid="import-error-banner"
            className="portable-error"
          >
            <strong>{t('Preflight Rejection:')}</strong> {importError}
          </div>
        )}
      </div>

      {/* Preflight Preview Details */}
      {importPreview && (
        <div
          data-testid="import-preview-container"
          style={{
            backgroundColor: 'var(--color-surface)',
            border: `1px solid ${importPreview.valid ? 'var(--color-border)' : 'var(--color-danger)'}`,
            borderRadius: 'var(--radius-md)',
            padding: '20px',
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: '16px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <span
                style={{
                  backgroundColor: importPreview.valid
                    ? 'var(--color-mesh)'
                    : 'var(--color-danger)',
                  color: '#fff',
                  fontSize: '11px',
                  fontWeight: 600,
                  padding: '2px 8px',
                  borderRadius: '12px',
                }}
              >
                {importPreview.valid ? t('✓ Preflight Passed') : t('✕ Preflight Failed')}
              </span>
            </div>
          </div>
          <p data-testid="import-summary">{t('Import summary: {added} new assets, {updated} updated assets, {conflicts} conflicts.', {
            added: importPreview.dispositions.assets_created,
            updated: importPreview.dispositions.assets_updated,
            conflicts: importPreview.errors.length,
          })}</p>
          <details className="quiet-details"><summary>{t('Technical details')}</summary>
            <p>{t('Manifest Inspection: v')}{importPreview.version} · {t('App version ')}{importPreview.app_version}</p>
            {/* Dispositions Table */}
            <div style={{ marginBottom: '20px' }}>
              <div
                style={{
                  fontSize: '12px',
                  fontWeight: 600,
                  color: 'var(--color-muted)',
                  marginBottom: '8px',
                  textTransform: 'uppercase',
                }}
              >{t('Dispositions (Dry-Run Prediction)')}</div>
              <ImportDispositions report={importPreview.dispositions} />
            </div>

          </details>

          {/* Confirmation and Apply action */}
          {importPreview.valid && (
            <div
              style={{
                borderTop: '1px solid var(--color-border)',
                paddingTop: '16px',
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
              }}
            >
              {previewIsStale && (
                <div
                  data-testid="import-stale-notice"
                  style={{
                    padding: '10px 12px',
                    backgroundColor: 'rgba(182, 59, 50, 0.08)',
                    border: '1px solid var(--color-danger)',
                    borderRadius: 'var(--radius-sm)',
                    color: 'var(--color-danger)',
                    fontSize: '12px',
                  }}
                >
                  <strong>{t('Preflight no longer applies.')}</strong>{t(' The field now points at a different directory than the one inspected above (')}{previewedRequest}{t('). Re-run the preview to import it.')}</div>
              )}

              <div>
                <button
                  data-testid="apply-import-button"
                  disabled={isApplying || isPreviewing || previewIsStale}
                  onClick={handleApplyImport}
                  style={{
                    cursor: isApplying || previewIsStale ? 'not-allowed' : 'pointer',
                    opacity: isApplying || previewIsStale ? 0.6 : 1,
                    padding: '9px 20px',
                    fontSize: '13px',
                    fontWeight: 600,
                    backgroundColor: 'var(--color-mesh)',
                    color: '#ffffff',
                    border: 'none',
                    borderRadius: 'var(--radius-sm)',
                  }}
                >
                  {isApplying ? t('Applying Import...') : t('Apply Import')}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Import Success Receipt */}
      {importReceipt && (
        <div
          data-testid="import-receipt"
          className="portable-card portable-card-success"
        >
          <div
            className="portable-receipt-heading"
          >
            <span
              className="portable-receipt-badge"
            >{t('✓ Import Successfully Applied')}</span>
            <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>{t('Applied from: ')}{importReceipt.source_dir}
            </span>
          </div>

          <p style={{ fontSize: '13px', color: 'var(--color-ink)', margin: '0 0 12px 0' }}>{t('All records have been atomically merged and updated into canonical database state at')}{' '}
            <code>{importReceipt.applied_at.slice(0, 19).replace('T', ' ')}</code>.
          </p>

          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
            <span
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '12px',
              }}
            >{t('Assets: +')}{importReceipt.report.assets_created}{t(' created,')}{' '}
              {importReceipt.report.assets_updated}{t(' updated')}</span>
            <span
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '12px',
              }}
            >{t('Media: +')}{importReceipt.report.media_created}{t(' created')}</span>
            <span
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '12px',
              }}
            >{t('Software: +')}{importReceipt.report.software_created}{t(' created')}</span>
            <span
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '12px',
              }}
            >{t('Services: +')}{importReceipt.report.services_created}{t(' created')}</span>
            <span style={{ padding: '4px 10px', backgroundColor: 'var(--color-canvas)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', fontSize: '12px' }}>
              {t('Information: +')}{importReceipt.report.info_created}{t(' created')}
            </span>
            <span
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '12px',
              }}
            >{t('Relations: +')}{importReceipt.report.relations_created}{t(' created')}</span>
          </div>
        </div>
      )}
    </div>
  );
}
