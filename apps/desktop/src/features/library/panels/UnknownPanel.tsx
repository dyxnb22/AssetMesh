import React from 'react';
import { Badge } from '../../../ui/Badge';
import type { UnknownDetailsDto } from '../types';

interface UnknownPanelProps {
  record: UnknownDetailsDto;
}

export const UnknownPanel: React.FC<UnknownPanelProps> = ({ record }) => {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '8px',
        backgroundColor: 'var(--color-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        padding: '14px',
      }}
      data-testid="unknown-panel"
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <Badge variant="attention">Unknown Module Format</Badge>
        <span style={{ fontSize: '12px', fontWeight: 500 }}>
          Module details format: {record.module}
        </span>
      </div>
      <p style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
        This asset contains module details unrecognized by this desktop client version. General
        metadata remains available.
      </p>
    </div>
  );
};
