import React, { useState } from 'react';
import { Badge } from '../../../ui/Badge';
import type { MediaRecordDto } from '../types';

interface MediaPanelProps {
  record: MediaRecordDto;
  isEditable?: boolean;
  onEditMetadata?: () => void;
  onTransitionStatus?: (status: string) => Promise<void>;
  onUpdateProgress?: (progress: { unit?: string; current?: number; total?: number }) => Promise<void>;
  onRate?: (rating: number) => Promise<void>;
}

export const MediaPanel: React.FC<MediaPanelProps> = ({
  record,
  isEditable = false,
  onEditMetadata,
  onTransitionStatus,
  onUpdateProgress,
  onRate,
}) => {
  const [editingProgress, setEditingProgress] = useState(false);
  const [progressCurrent, setProgressCurrent] = useState<number | ''>(record.progress?.current ?? 0);
  const [progressTotal, setProgressTotal] = useState<number | ''>(record.progress?.total ?? '');
  const [progressUnit, setProgressUnit] = useState<string>(record.progress?.unit ?? 'episodes');

  const [editingRating, setEditingRating] = useState(false);
  const [ratingInput, setRatingInput] = useState<number | ''>(record.rating ?? '');

  const [busy, setBusy] = useState(false);

  const handleStatusTransition = async (newStatus: string) => {
    if (!onTransitionStatus || busy) return;
    setBusy(true);
    try {
      await onTransitionStatus(newStatus);
    } finally {
      setBusy(false);
    }
  };

  const handleSaveProgress = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!onUpdateProgress || busy) return;
    setBusy(true);
    try {
      await onUpdateProgress({
        unit: progressUnit.trim() || undefined,
        current: progressCurrent !== '' ? Number(progressCurrent) : undefined,
        total: progressTotal !== '' ? Number(progressTotal) : undefined,
      });
      setEditingProgress(false);
    } finally {
      setBusy(false);
    }
  };

  const handleSaveRating = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!onRate || busy || ratingInput === '') return;
    setBusy(true);
    try {
      await onRate(Number(ratingInput));
      setEditingRating(false);
    } finally {
      setBusy(false);
    }
  };

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
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span
            style={{
              fontSize: '11px',
              textTransform: 'uppercase',
              color: 'var(--color-muted)',
              fontWeight: 600,
            }}
          >
            Media Details
          </span>
          <Badge variant="muted">{record.media_type}</Badge>
        </div>
        {isEditable && onEditMetadata && (
          <button
            type="button"
            data-testid="edit-media-metadata-button"
            onClick={onEditMetadata}
            style={{
              background: 'none',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              padding: '2px 8px',
              fontSize: '11px',
              cursor: 'pointer',
              color: 'var(--color-ink)',
            }}
          >
            ✎ Edit Details
          </button>
        )}
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))',
          gap: '10px',
          fontSize: '12px',
        }}
      >
        <div>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Status</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
            <span style={{ fontWeight: 500 }} data-testid="media-status-text">
              {record.status}
            </span>
            {isEditable && onTransitionStatus && (
              <div style={{ display: 'flex', gap: '4px' }}>
                {record.status === 'planned' && (
                  <button
                    type="button"
                    data-testid="media-action-start"
                    disabled={busy}
                    onClick={() => handleStatusTransition('in_progress')}
                    style={{
                      padding: '2px 8px',
                      fontSize: '11px',
                      backgroundColor: 'var(--color-mesh)',
                      color: '#fff',
                      border: 'none',
                      borderRadius: 'var(--radius-sm)',
                      cursor: busy ? 'not-allowed' : 'pointer',
                    }}
                  >
                    ▶ Start
                  </button>
                )}
                {record.status === 'in_progress' && (
                  <>
                    <button
                      type="button"
                      data-testid="media-action-pause"
                      disabled={busy}
                      onClick={() => handleStatusTransition('paused')}
                      style={{
                        padding: '2px 8px',
                        fontSize: '11px',
                        backgroundColor: 'var(--color-canvas)',
                        border: '1px solid var(--color-border)',
                        borderRadius: 'var(--radius-sm)',
                        cursor: busy ? 'not-allowed' : 'pointer',
                      }}
                    >
                      ⏸ Pause
                    </button>
                    <button
                      type="button"
                      data-testid="media-action-complete"
                      disabled={busy}
                      onClick={() => handleStatusTransition('completed')}
                      style={{
                        padding: '2px 8px',
                        fontSize: '11px',
                        backgroundColor: 'var(--color-mesh)',
                        color: '#fff',
                        border: 'none',
                        borderRadius: 'var(--radius-sm)',
                        cursor: busy ? 'not-allowed' : 'pointer',
                      }}
                    >
                      ✓ Complete
                    </button>
                    <button
                      type="button"
                      data-testid="media-action-drop"
                      disabled={busy}
                      onClick={() => handleStatusTransition('dropped')}
                      style={{
                        padding: '2px 8px',
                        fontSize: '11px',
                        backgroundColor: 'var(--color-canvas)',
                        color: 'var(--color-danger)',
                        border: '1px solid var(--color-danger)',
                        borderRadius: 'var(--radius-sm)',
                        cursor: busy ? 'not-allowed' : 'pointer',
                      }}
                    >
                      ✕ Drop
                    </button>
                  </>
                )}
                {record.status === 'paused' && (
                  <>
                    <button
                      type="button"
                      data-testid="media-action-resume"
                      disabled={busy}
                      onClick={() => handleStatusTransition('in_progress')}
                      style={{
                        padding: '2px 8px',
                        fontSize: '11px',
                        backgroundColor: 'var(--color-mesh)',
                        color: '#fff',
                        border: 'none',
                        borderRadius: 'var(--radius-sm)',
                        cursor: busy ? 'not-allowed' : 'pointer',
                      }}
                    >
                      ▶ Resume
                    </button>
                    <button
                      type="button"
                      data-testid="media-action-complete"
                      disabled={busy}
                      onClick={() => handleStatusTransition('completed')}
                      style={{
                        padding: '2px 8px',
                        fontSize: '11px',
                        backgroundColor: 'var(--color-canvas)',
                        border: '1px solid var(--color-border)',
                        borderRadius: 'var(--radius-sm)',
                        cursor: busy ? 'not-allowed' : 'pointer',
                      }}
                    >
                      ✓ Complete
                    </button>
                    <button
                      type="button"
                      data-testid="media-action-drop"
                      disabled={busy}
                      onClick={() => handleStatusTransition('dropped')}
                      style={{
                        padding: '2px 8px',
                        fontSize: '11px',
                        backgroundColor: 'var(--color-canvas)',
                        color: 'var(--color-danger)',
                        border: '1px solid var(--color-danger)',
                        borderRadius: 'var(--radius-sm)',
                        cursor: busy ? 'not-allowed' : 'pointer',
                      }}
                    >
                      ✕ Drop
                    </button>
                  </>
                )}
                {record.status === 'dropped' && (
                  <button
                    type="button"
                    data-testid="media-action-resume"
                    disabled={busy}
                    onClick={() => handleStatusTransition('in_progress')}
                    style={{
                      padding: '2px 8px',
                      fontSize: '11px',
                      backgroundColor: 'var(--color-canvas)',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      cursor: busy ? 'not-allowed' : 'pointer',
                    }}
                  >
                    ↺ Restart
                  </button>
                )}
                {record.status === 'completed' && (
                  <button
                    type="button"
                    data-testid="media-action-reopen"
                    disabled={busy}
                    onClick={() => handleStatusTransition('in_progress')}
                    style={{
                      padding: '2px 8px',
                      fontSize: '11px',
                      backgroundColor: 'var(--color-canvas)',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      cursor: busy ? 'not-allowed' : 'pointer',
                    }}
                  >
                    ↺ Reopen
                  </button>
                )}
              </div>
            )}
          </div>
        </div>

        <div>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Rating</div>
          {editingRating ? (
            <form onSubmit={handleSaveRating} style={{ display: 'flex', gap: '4px', alignItems: 'center' }}>
              <input
                type="number"
                min="0"
                max="10"
                step="0.5"
                data-testid="media-rating-input"
                value={ratingInput}
                onChange={(e) => setRatingInput(e.target.value === '' ? '' : Number(e.target.value))}
                style={{ width: '60px', padding: '2px 4px', fontSize: '12px' }}
                autoFocus
              />
              <button
                type="submit"
                data-testid="save-rating-button"
                disabled={busy}
                style={{ padding: '2px 6px', fontSize: '11px' }}
              >
                Save
              </button>
              <button
                type="button"
                onClick={() => setEditingRating(false)}
                style={{ padding: '2px 6px', fontSize: '11px' }}
              >
                Cancel
              </button>
            </form>
          ) : (
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span
                style={{ fontWeight: 600, color: 'var(--color-mesh)' }}
                data-testid="media-rating-text"
              >
                {record.rating != null ? `★ ${record.rating} / 10` : 'Unrated'}
              </span>
              {isEditable && onRate && (
                <button
                  type="button"
                  data-testid="edit-rating-button"
                  onClick={() => {
                    setRatingInput(record.rating ?? '');
                    setEditingRating(true);
                  }}
                  style={{
                    background: 'none',
                    border: 'none',
                    cursor: 'pointer',
                    fontSize: '11px',
                    color: 'var(--color-muted)',
                    textDecoration: 'underline',
                  }}
                >
                  Edit
                </button>
              )}
            </div>
          )}
        </div>

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

      <div
        style={{
          borderTop: '1px solid var(--color-border-subtle)',
          paddingTop: '8px',
          fontSize: '12px',
        }}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: '4px',
          }}
        >
          <div style={{ color: 'var(--color-muted)' }}>Progress</div>
          {isEditable && onUpdateProgress && !editingProgress && (
            <button
              type="button"
              data-testid="edit-progress-button"
              onClick={() => {
                setProgressCurrent(record.progress?.current ?? 0);
                setProgressTotal(record.progress?.total ?? '');
                setProgressUnit(record.progress?.unit ?? 'episodes');
                setEditingProgress(true);
              }}
              style={{
                background: 'none',
                border: 'none',
                cursor: 'pointer',
                fontSize: '11px',
                color: 'var(--color-muted)',
                textDecoration: 'underline',
              }}
            >
              Edit Progress
            </button>
          )}
        </div>

        {editingProgress ? (
          <form
            onSubmit={handleSaveProgress}
            data-testid="media-progress-form"
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: '6px',
              backgroundColor: 'var(--color-canvas)',
              padding: '8px',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flexWrap: 'wrap' }}>
              <label style={{ fontSize: '11px' }}>
                Current:
                <input
                  type="number"
                  min="0"
                  step="any"
                  data-testid="media-progress-current-input"
                  value={progressCurrent}
                  onChange={(e) =>
                    setProgressCurrent(e.target.value === '' ? '' : Number(e.target.value))
                  }
                  style={{ width: '60px', marginLeft: '4px', padding: '2px 4px', fontSize: '11px' }}
                />
              </label>
              <label style={{ fontSize: '11px' }}>
                Total:
                <input
                  type="number"
                  min="0"
                  step="any"
                  data-testid="media-progress-total-input"
                  value={progressTotal}
                  onChange={(e) =>
                    setProgressTotal(e.target.value === '' ? '' : Number(e.target.value))
                  }
                  style={{ width: '60px', marginLeft: '4px', padding: '2px 4px', fontSize: '11px' }}
                />
              </label>
              <label style={{ fontSize: '11px' }}>
                Unit:
                <input
                  type="text"
                  data-testid="media-progress-unit-input"
                  value={progressUnit}
                  onChange={(e) => setProgressUnit(e.target.value)}
                  style={{ width: '70px', marginLeft: '4px', padding: '2px 4px', fontSize: '11px' }}
                />
              </label>
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '6px' }}>
              <button
                type="button"
                onClick={() => setEditingProgress(false)}
                disabled={busy}
                style={{ padding: '2px 8px', fontSize: '11px' }}
              >
                Cancel
              </button>
              <button
                type="submit"
                data-testid="save-progress-button"
                disabled={busy}
                style={{
                  padding: '2px 8px',
                  fontSize: '11px',
                  backgroundColor: 'var(--color-mesh)',
                  color: '#fff',
                  border: 'none',
                  borderRadius: 'var(--radius-sm)',
                }}
              >
                Save Progress
              </button>
            </div>
          </form>
        ) : (
          <div data-testid="media-progress-text">
            {record.progress ? (
              <span>
                {record.progress.current}
                {record.progress.total != null ? ` / ${record.progress.total}` : ''}{' '}
                <span style={{ color: 'var(--color-muted)' }}>{record.progress.unit || 'units'}</span>
              </span>
            ) : (
              <span style={{ color: 'var(--color-muted)' }}>No progress recorded</span>
            )}
          </div>
        )}
      </div>

      {record.notes && (
        <div
          style={{
            borderTop: '1px solid var(--color-border-subtle)',
            paddingTop: '8px',
            fontSize: '12px',
          }}
        >
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Notes</div>
          <p style={{ whiteSpace: 'pre-wrap', color: 'var(--color-ink)' }}>{record.notes}</p>
        </div>
      )}

      {(record.started_at || record.completed_at) && (
        <div
          style={{
            display: 'flex',
            gap: '16px',
            borderTop: '1px solid var(--color-border-subtle)',
            paddingTop: '8px',
            fontSize: '11px',
            color: 'var(--color-muted)',
          }}
        >
          {record.started_at && <div>Started: {record.started_at.slice(0, 10)}</div>}
          {record.completed_at && <div>Completed: {record.completed_at.slice(0, 10)}</div>}
        </div>
      )}
    </div>
  );
};
