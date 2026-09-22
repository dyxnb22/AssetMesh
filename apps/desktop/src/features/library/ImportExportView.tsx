import React, { useState } from 'react';
import { getTransport, normalizeDesktopError } from './transport';
import type { ExportReceipt, ImportPreview, ImportReceipt } from './types';

export const ImportExportView: React.FC = () => {
  const transport = getTransport();

  const [activeTab, setActiveTab] = useState<'export' | 'import'>('export');

  // Export State
  const [exportDir, setExportDir] = useState<string>('');
  const [isExporting, setIsExporting] = useState<boolean>(false);
  const [exportReceipt, setExportReceipt] = useState<ExportReceipt | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  // Import State
  const [importDir, setImportDir] = useState<string>('');
  const [isPreviewing, setIsPreviewing] = useState<boolean>(false);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  // What the last preview asked the backend to inspect. Tracked separately from
  // `importPreview.source_dir` — that is the backend's own report and may
  // legitimately differ (it resolves and normalizes the path), so judging
  // staleness against the request rather than the report avoids a false
  // "stale" on a path the backend merely cleaned up.
  const [previewedRequest, setPreviewedRequest] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [isConfirmed, setIsConfirmed] = useState<boolean>(false);
  const [isApplying, setIsApplying] = useState<boolean>(false);
  const [importReceipt, setImportReceipt] = useState<ImportReceipt | null>(null);

  // Browse Directory for Export
  const handleBrowseExport = async () => {
    try {
      const picked = await transport.pickDirectory('Select Directory to Export Bundle');
      if (picked !== null) {
        setExportDir(picked);
      }
    } catch (err) {
      setExportError(normalizeDesktopError(err).message);
    }
  };

  // Run Export
  const handleExecuteExport = async () => {
    if (!exportDir.trim()) {
      setExportError('Please specify a target destination directory.');
      return;
    }
    setIsExporting(true);
    setExportError(null);
    try {
      const receipt = await transport.portableExport(exportDir.trim());
      setExportReceipt(receipt);
    } catch (err) {
      setExportError(normalizeDesktopError(err).message);
    } finally {
      setIsExporting(false);
    }
  };

  // Browse Directory for Import
  const handleBrowseImport = async () => {
    try {
      const picked = await transport.pickDirectory('Select Portable Bundle Directory to Import');
      if (picked !== null) {
        setImportDir(picked);
        // Reset previous preview / receipt
        setImportPreview(null);
        setPreviewedRequest(null);
        setImportReceipt(null);
        setImportError(null);
        setIsConfirmed(false);
      }
    } catch (err) {
      setImportError(normalizeDesktopError(err).message);
    }
  };

  // Run Import Preflight Preview
  const handlePreviewImport = async () => {
    const source = importDir.trim();
    if (!source) {
      setImportError('Please specify the source bundle directory.');
      return;
    }
    setIsPreviewing(true);
    setImportError(null);
    setImportReceipt(null);
    setIsConfirmed(false);
    try {
      const preview = await transport.portableImportPreview(source);
      setImportPreview(preview);
      setPreviewedRequest(source);
      if (!preview.valid && preview.errors.length > 0) {
        setImportError(preview.errors.join('; '));
      }
    } catch (err) {
      setImportError(normalizeDesktopError(err).message);
    } finally {
      setIsPreviewing(false);
    }
  };

  // A preview describes one directory. If the field no longer matches the
  // request that produced it, the confirmation on screen refers to a bundle
  // nobody inspected, so the apply must be refused rather than run against
  // whatever happens to be in the input.
  const previewIsStale =
    importPreview !== null && previewedRequest !== importDir.trim();

  // Run Explicit Apply
  const handleApplyImport = async () => {
    if (!importPreview || !importPreview.valid || !isConfirmed || previewIsStale) {
      return;
    }
    setIsApplying(true);
    setImportError(null);
    try {
      // Apply the directory the backend inspected, not whatever the field says
      // now — `source_dir` is the backend's own report.
      const receipt = await transport.portableImportApply(importPreview.source_dir, importPreview.fingerprint);
      setImportReceipt(receipt);
    } catch (err) {
      setImportError(normalizeDesktopError(err).message);
    } finally {
      setIsApplying(false);
    }
  };

  return (
    <div
      data-testid="import-export-workspace"
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        overflowY: 'auto',
        backgroundColor: 'var(--color-canvas)',
        padding: '24px',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '20px',
          borderBottom: '1px solid var(--color-border)',
          paddingBottom: '16px',
        }}
      >
        <div>
          <h2
            style={{
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--color-ink)',
              margin: '0 0 4px 0',
            }}
          >
            Portable Data (Import / Export)
          </h2>
          <div style={{ fontSize: '13px', color: 'var(--color-muted)' }}>
            Export your entire personal asset ledger to a deterministic portable bundle, or inspect
            and import portable data with mandatory preflight verification.
          </div>
        </div>

        {/* Tab switch */}
        <div
          role="tablist"
          aria-label="Portable Data Tabs"
          style={{
            display: 'flex',
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-sm)',
            padding: '2px',
          }}
        >
          <button
            role="tab"
            data-testid="tab-export"
            aria-selected={activeTab === 'export'}
            onClick={() => setActiveTab('export')}
            style={{
              all: 'unset',
              cursor: 'pointer',
              padding: '6px 14px',
              fontSize: '12px',
              fontWeight: activeTab === 'export' ? 600 : 400,
              backgroundColor: activeTab === 'export' ? 'var(--color-canvas)' : 'transparent',
              color: activeTab === 'export' ? 'var(--color-mesh)' : 'var(--color-muted)',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            Export Bundle
          </button>
          <button
            role="tab"
            data-testid="tab-import"
            aria-selected={activeTab === 'import'}
            onClick={() => setActiveTab('import')}
            style={{
              all: 'unset',
              cursor: 'pointer',
              padding: '6px 14px',
              fontSize: '12px',
              fontWeight: activeTab === 'import' ? 600 : 400,
              backgroundColor: activeTab === 'import' ? 'var(--color-canvas)' : 'transparent',
              color: activeTab === 'import' ? 'var(--color-mesh)' : 'var(--color-muted)',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            Import Bundle
          </button>
        </div>
      </div>

      {/* EXPORT WORKSPACE */}
      {activeTab === 'export' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '20px', maxWidth: '800px' }}>
          <div
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              Export Portable Bundle
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              Select a target directory where the standard <code>assetmesh-export/</code> bundle
              will be created. All canonical assets, media, software, services, relations, and
              activity history will be preserved.
            </p>

            {/* Target Destination Directory */}
            <div style={{ display: 'flex', gap: '8px', marginBottom: '16px' }}>
              <input
                data-testid="export-dir-input"
                type="text"
                aria-label="Target directory"
                placeholder="/path/to/destination-folder"
                value={exportDir}
                onChange={(e) => setExportDir(e.target.value)}
                style={{
                  flex: 1,
                  padding: '8px 12px',
                  fontSize: '13px',
                  fontFamily: 'monospace',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-canvas)',
                  color: 'var(--color-ink)',
                }}
              />
              <button
                data-testid="export-browse-button"
                onClick={handleBrowseExport}
                style={{
                  cursor: 'pointer',
                  padding: '8px 16px',
                  fontSize: '12px',
                  fontWeight: 500,
                  backgroundColor: 'var(--color-canvas)',
                  color: 'var(--color-ink)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                }}
              >
                Browse...
              </button>
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
                {isExporting ? 'Exporting...' : 'Export Bundle'}
              </button>
            </div>

            {/* Error Notice */}
            {exportError && (
              <div
                data-testid="export-error-banner"
                style={{
                  padding: '12px',
                  backgroundColor: 'rgba(182, 59, 50, 0.08)',
                  border: '1px solid var(--color-danger)',
                  borderRadius: 'var(--radius-sm)',
                  color: 'var(--color-danger)',
                  fontSize: '13px',
                }}
              >
                <strong>Export Error:</strong> {exportError}
              </div>
            )}
          </div>

          {/* Export Receipt */}
          {exportReceipt && (
            <div
              data-testid="export-receipt"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '20px',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  marginBottom: '12px',
                }}
              >
                <span
                  style={{
                    backgroundColor: 'var(--color-mesh)',
                    color: '#fff',
                    fontSize: '11px',
                    fontWeight: 600,
                    padding: '2px 8px',
                    borderRadius: '12px',
                  }}
                >
                  ✓ Export Complete
                </span>
                <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                  Receipt: {exportReceipt.target_dir}
                </span>
              </div>

              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))',
                  gap: '12px',
                  marginBottom: '16px',
                }}
              >
                <div
                  style={{
                    padding: '8px 12px',
                    backgroundColor: 'var(--color-canvas)',
                    borderRadius: 'var(--radius-sm)',
                  }}
                >
                  <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>Format & Version</div>
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
                  <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>Created At</div>
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
                  <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>App Version</div>
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
                >
                  Exported Record Breakdown
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
                  {Object.entries(exportReceipt.record_counts).map(([name, count]) => (
                    <span
                      key={name}
                      style={{
                        padding: '4px 10px',
                        backgroundColor: 'var(--color-canvas)',
                        border: '1px solid var(--color-border)',
                        borderRadius: 'var(--radius-sm)',
                        fontSize: '12px',
                        color: 'var(--color-ink)',
                      }}
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
                >
                  Files Written ({exportReceipt.files.length})
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
      )}

      {/* IMPORT WORKSPACE */}
      {activeTab === 'import' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '20px', maxWidth: '800px' }}>
          <div
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              Import Portable Bundle
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              Select an existing portable bundle directory to import. Every bundle undergoes
              mandatory preflight inspection before any mutation can occur.
            </p>

            {/* Source Directory */}
            <div style={{ display: 'flex', gap: '8px', marginBottom: '16px' }}>
              <input
                data-testid="import-dir-input"
                type="text"
                aria-label="Source bundle directory"
                placeholder="/path/to/exported-bundle"
                value={importDir}
                onChange={(e) => {
                  setImportDir(e.target.value);
                  // A receipt belongs to a bundle that has already been applied.
                  // Editing the source must not leave it on screen attached to a
                  // directory that is no longer the one that produced it.
                  setImportReceipt(null);
                }}
                style={{
                  flex: 1,
                  padding: '8px 12px',
                  fontSize: '13px',
                  fontFamily: 'monospace',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-canvas)',
                  color: 'var(--color-ink)',
                }}
              />
              <button
                data-testid="import-browse-button"
                onClick={handleBrowseImport}
                style={{
                  cursor: 'pointer',
                  padding: '8px 16px',
                  fontSize: '12px',
                  fontWeight: 500,
                  backgroundColor: 'var(--color-canvas)',
                  color: 'var(--color-ink)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                }}
              >
                Browse...
              </button>
              <button
                data-testid="preview-import-button"
                disabled={isPreviewing || !importDir.trim()}
                onClick={handlePreviewImport}
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
                {isPreviewing ? 'Inspecting...' : 'Preview Import'}
              </button>
            </div>

            {/* Error Banner */}
            {importError && (
              <div
                data-testid="import-error-banner"
                style={{
                  padding: '12px',
                  backgroundColor: 'rgba(182, 59, 50, 0.08)',
                  border: '1px solid var(--color-danger)',
                  borderRadius: 'var(--radius-sm)',
                  color: 'var(--color-danger)',
                  fontSize: '13px',
                }}
              >
                <strong>Preflight Rejection:</strong> {importError}
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
                    {importPreview.valid ? '✓ Preflight Passed' : '✕ Preflight Failed'}
                  </span>
                  <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                    Manifest Inspection: v{importPreview.version}
                  </span>
                </div>
                <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
                  App version {importPreview.app_version}
                </div>
              </div>

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
                >
                  Dispositions (Dry-Run Prediction)
                </div>
                <table
                  data-testid="import-dispositions-table"
                  style={{
                    width: '100%',
                    borderCollapse: 'collapse',
                    fontSize: '12px',
                    textAlign: 'left',
                  }}
                >
                  <thead>
                    <tr style={{ borderBottom: '1px solid var(--color-border)', color: 'var(--color-muted)' }}>
                      <th style={{ padding: '6px 8px' }}>Entity Type</th>
                      <th style={{ padding: '6px 8px' }}>To Be Created</th>
                      <th style={{ padding: '6px 8px' }}>To Be Updated</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
                      <td style={{ padding: '6px 8px', fontWeight: 500 }}>Assets</td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-mesh)' }}>
                        +{importPreview.dispositions.assets_created}
                      </td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-ink)' }}>
                        {importPreview.dispositions.assets_updated}
                      </td>
                    </tr>
                    <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
                      <td style={{ padding: '6px 8px', fontWeight: 500 }}>Media</td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-mesh)' }}>
                        +{importPreview.dispositions.media_created}
                      </td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-ink)' }}>
                        {importPreview.dispositions.media_updated}
                      </td>
                    </tr>
                    <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
                      <td style={{ padding: '6px 8px', fontWeight: 500 }}>Software</td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-mesh)' }}>
                        +{importPreview.dispositions.software_created}
                      </td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-ink)' }}>
                        {importPreview.dispositions.software_updated}
                      </td>
                    </tr>
                    <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
                      <td style={{ padding: '6px 8px', fontWeight: 500 }}>Services</td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-mesh)' }}>
                        +{importPreview.dispositions.services_created}
                      </td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-ink)' }}>
                        {importPreview.dispositions.services_updated}
                      </td>
                    </tr>
                    <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
                      <td style={{ padding: '6px 8px', fontWeight: 500 }}>Relations</td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-mesh)' }}>
                        +{importPreview.dispositions.relations_created}
                      </td>
                      <td style={{ padding: '6px 8px', color: 'var(--color-ink)' }}>
                        {importPreview.dispositions.relations_updated}
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

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
                      <strong>Preflight no longer applies.</strong> The field now points at a
                      different directory than the one inspected above ({previewedRequest}). Re-run
                      the preview to import it.
                    </div>
                  )}

                  <label
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '8px',
                      fontSize: '13px',
                      color: 'var(--color-ink)',
                      cursor: 'pointer',
                    }}
                  >
                    <input
                      data-testid="import-confirm-checkbox"
                      type="checkbox"
                      checked={isConfirmed}
                      onChange={(e) => setIsConfirmed(e.target.checked)}
                    />
                    <span>
                      I have reviewed the preflight dispositions and explicitly confirm importing
                      this bundle into the local database.
                    </span>
                  </label>

                  <div>
                    <button
                      data-testid="apply-import-button"
                      disabled={!isConfirmed || isApplying || previewIsStale}
                      onClick={handleApplyImport}
                      style={{
                        cursor: !isConfirmed || isApplying || previewIsStale ? 'not-allowed' : 'pointer',
                        opacity: !isConfirmed || isApplying || previewIsStale ? 0.6 : 1,
                        padding: '9px 20px',
                        fontSize: '13px',
                        fontWeight: 600,
                        backgroundColor: 'var(--color-mesh)',
                        color: '#ffffff',
                        border: 'none',
                        borderRadius: 'var(--radius-sm)',
                      }}
                    >
                      {isApplying ? 'Applying Import...' : 'Apply Import'}
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
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '20px',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  marginBottom: '12px',
                }}
              >
                <span
                  style={{
                    backgroundColor: 'var(--color-mesh)',
                    color: '#fff',
                    fontSize: '11px',
                    fontWeight: 600,
                    padding: '2px 8px',
                    borderRadius: '12px',
                  }}
                >
                  ✓ Import Successfully Applied
                </span>
                <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                  Applied from: {importReceipt.source_dir}
                </span>
              </div>

              <p style={{ fontSize: '13px', color: 'var(--color-ink)', margin: '0 0 12px 0' }}>
                All records have been atomically merged and updated into canonical database state at{' '}
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
                >
                  Assets: +{importReceipt.report.assets_created} created,{' '}
                  {importReceipt.report.assets_updated} updated
                </span>
                <span
                  style={{
                    padding: '4px 10px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                  }}
                >
                  Media: +{importReceipt.report.media_created} created
                </span>
                <span
                  style={{
                    padding: '4px 10px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                  }}
                >
                  Software: +{importReceipt.report.software_created} created
                </span>
                <span
                  style={{
                    padding: '4px 10px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                  }}
                >
                  Services: +{importReceipt.report.services_created} created
                </span>
                <span
                  style={{
                    padding: '4px 10px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                  }}
                >
                  Relations: +{importReceipt.report.relations_created} created
                </span>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
