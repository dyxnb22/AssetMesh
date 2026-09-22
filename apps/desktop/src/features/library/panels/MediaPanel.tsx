import React from 'react';
import { Badge } from '../../../ui/Badge';
import type { MediaRecordDto } from '../types';

interface MediaPanelProps {
  record: MediaRecordDto;
}

export const MediaPanel: React.FC<MediaPanelProps> = ({ record }) => {
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
      data-testid="media-panel"
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontSize: '11px', textTransform: 'uppercase', color: 'var(--color-muted)', fontWeight: 600 }}>
          Media Details
        </span>
        <Badge variant="muted">{record.media_type}</Badge>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: '10px', fontSize: '12px' }}>
        <div>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Status</div>
          <div style={{ fontWeight: 500 }}>{record.status}</div>
        </div>

        {record.rating != null && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Rating</div>
            <div style={{ fontWeight: 600, color: 'var(--color-mesh)' }}>
              ★ {record.rating} / 10
            </div>
          </div>
        )}

        {record.year != null && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Year</div>
            <div>{record.year}</div>
          </div>
        )}

        {record.platform && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Platform</div>
            <div>{record.platform}</div>
          </div>
        )}
      </div>

      {record.progress && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Progress</div>
          <div>
            {record.progress.current}
            {record.progress.total != null ? ` / ${record.progress.total}` : ''}{' '}
            <span style={{ color: 'var(--color-muted)' }}>{record.progress.unit || 'units'}</span>
          </div>
        </div>
      )}

      {record.notes && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Notes</div>
          <p style={{ whiteSpace: 'pre-wrap', color: 'var(--color-ink)' }}>{record.notes}</p>
        </div>
      )}

      {(record.started_at || record.completed_at) && (
        <div style={{ display: 'flex', gap: '16px', borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '11px', color: 'var(--color-muted)' }}>
          {record.started_at && <div>Started: {record.started_at.slice(0, 10)}</div>}
          {record.completed_at && <div>Completed: {record.completed_at.slice(0, 10)}</div>}
        </div>
      )}
    </div>
  );
};
