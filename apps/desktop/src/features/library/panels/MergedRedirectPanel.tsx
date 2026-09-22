import React from 'react';
import { Badge } from '../../../ui/Badge';
import type { MergedRedirectDto } from '../types';

interface MergedRedirectPanelProps {
  record: MergedRedirectDto;
  onFollowRedirect: (survivingAssetId: string) => void;
}

export const MergedRedirectPanel: React.FC<MergedRedirectPanelProps> = ({
  record,
  onFollowRedirect,
}) => {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '12px',
        backgroundColor: 'var(--color-danger-bg)',
        border: '1px solid var(--color-danger)',
        borderRadius: 'var(--radius-md)',
        padding: '14px',
      }}
      data-testid="merged-redirect-panel"
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <Badge variant="danger">Merged Redirect</Badge>
        <span style={{ fontWeight: 600, color: 'var(--color-danger)' }}>
          This asset was explicitly merged
        </span>
      </div>

      <p style={{ fontSize: '12px', color: 'var(--color-ink)' }}>
        This record is a tombstone redirect. All canonical references and history belong to the
        surviving asset.
      </p>

      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          backgroundColor: 'var(--color-surface)',
          padding: '8px 12px',
          borderRadius: 'var(--radius-sm)',
          border: '1px solid var(--color-border)',
        }}
      >
        <div>
          <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>Surviving Asset ID</div>
          <code style={{ fontSize: '11px' }}>{record.surviving_asset_id}</code>
        </div>
        <button
          onClick={() => onFollowRedirect(record.surviving_asset_id)}
          style={{
            padding: '6px 12px',
            backgroundColor: 'var(--color-mesh)',
            color: '#FFFFFF',
            border: 'none',
            borderRadius: 'var(--radius-sm)',
            cursor: 'pointer',
            fontSize: '12px',
            fontWeight: 500,
          }}
        >
          View Survivor →
        </button>
      </div>
    </div>
  );
};
