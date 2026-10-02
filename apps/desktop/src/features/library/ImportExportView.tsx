import React, { useState } from 'react';
import { getTransport } from './transport';
import { t } from '../../i18n';
import { ExportWorkspace } from './portable/ExportWorkspace';
import { ImportWorkspace } from './portable/ImportWorkspace';
import { useExportBundle } from './portable/useExportBundle';
import { useImportBundle } from './portable/useImportBundle';
import './portable/PortableWorkspace.css';

export const ImportExportView: React.FC = () => {
  const transport = getTransport();
  const [activeTab, setActiveTab] = useState<'export' | 'import'>('export');
  // Workflow hooks stay mounted so changing tabs preserves drafts and receipts.
  const exportWorkflow = useExportBundle(transport);
  const importWorkflow = useImportBundle(transport);
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
          >{t('Portable Data (Import / Export)')}</h2>
          <div style={{ fontSize: '13px', color: 'var(--color-muted)' }}>{t('Export your entire personal asset ledger to a deterministic portable bundle, or inspect and import portable data with mandatory preflight verification.')}</div>
        </div>

        {/* Tab switch */}
        <div
          role="tablist"
          aria-label={t('Portable Data Tabs')}
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
            className="portable-tab"
          >{t('Export Bundle')}</button>
          <button
            role="tab"
            data-testid="tab-import"
            aria-selected={activeTab === 'import'}
            onClick={() => setActiveTab('import')}
            className="portable-tab"
          >{t('Import Bundle')}</button>
        </div>
      </div>

      {activeTab === 'export' && <ExportWorkspace workflow={exportWorkflow} />}
      {activeTab === 'import' && <ImportWorkspace workflow={importWorkflow} />}
    </div>
  );
};
