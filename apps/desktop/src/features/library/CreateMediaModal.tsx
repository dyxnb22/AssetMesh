import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type { DesktopError } from './types';

interface CreateMediaModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated: (newAssetId: string) => void;
}

export const CreateMediaModal: React.FC<CreateMediaModalProps> = ({
  isOpen,
  onClose,
  onCreated,
}) => {
  const [title, setTitle] = useState('');
  const [mediaType, setMediaType] = useState('anime');
  const [summary, setSummary] = useState('');
  const [status, setStatus] = useState('planned');
  const [rating, setRating] = useState<number | ''>('');
  const [year, setYear] = useState<number | ''>('');
  const [platform, setPlatform] = useState('');
  const [progressUnit, setProgressUnit] = useState('episodes');
  const [progressCurrent, setProgressCurrent] = useState<number | ''>('');
  const [progressTotal, setProgressTotal] = useState<number | ''>('');
  const [notes, setNotes] = useState('');
  const [tagsInput, setTagsInput] = useState('');

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !submitting) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose, submitting]);

  if (!isOpen) return null;

  const transport = getTransport();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (submitting) return;

    if (!title.trim()) {
      setError({
        category: 'invalid_input',
        message: 'Media title must not be empty',
      });
      return;
    }

    setSubmitting(true);
    setError(null);

    const tags = tagsInput
      .split(',')
      .map((t) => t.trim().toLowerCase())
      .filter((t) => t.length > 0);

    try {
      const receipt = await transport.mediaCommand({
        action: 'create',
        title: title.trim(),
        media_type: mediaType,
        summary: summary.trim() || undefined,
        status: status || undefined,
        rating: rating !== '' ? Number(rating) : undefined,
        year: year !== '' ? Number(year) : undefined,
        platform: platform.trim() || undefined,
        progress_unit: progressUnit.trim() || undefined,
        progress_current: progressCurrent !== '' ? Number(progressCurrent) : undefined,
        progress_total: progressTotal !== '' ? Number(progressTotal) : undefined,
        notes: notes.trim() || undefined,
        tags,
      });

      const newId = receipt.asset_ids[0];
      onCreated(newId);
      onClose();
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Add Media Asset"
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.45)',
        backdropFilter: 'blur(2px)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 110,
        padding: '24px',
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget && !submitting) {
          onClose();
        }
      }}
    >
      <div
        style={{
          width: '100%',
          maxWidth: '540px',
          maxHeight: '88vh',
          backgroundColor: 'var(--color-surface)',
          borderRadius: 'var(--radius-lg)',
          border: '1px solid var(--color-border)',
          boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            padding: '16px 20px',
            borderBottom: '1px solid var(--color-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Badge variant="mesh">Media</Badge>
            <h3 style={{ margin: 0, fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)' }}>
              Add Media Asset
            </h3>
          </div>
          <button
            onClick={onClose}
            disabled={submitting}
            aria-label="Close add media modal"
            style={{
              background: 'none',
              border: 'none',
              cursor: submitting ? 'not-allowed' : 'pointer',
              fontSize: '16px',
              color: 'var(--color-muted)',
            }}
          >
            ✕
          </button>
        </div>

        <form
          onSubmit={handleSubmit}
          data-testid="create-media-form"
          style={{
            padding: '20px',
            overflowY: 'auto',
            display: 'flex',
            flexDirection: 'column',
            gap: '14px',
          }}
        >
          {error && (
            <div
              role="alert"
              data-testid="create-media-error"
              style={{
                padding: '10px 14px',
                backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
                border: '1px solid var(--color-danger)',
                borderRadius: 'var(--radius-md)',
                color: 'var(--color-danger)',
                fontSize: '12px',
              }}
            >
              <div style={{ fontWeight: 600, marginBottom: '2px' }}>{error.category}</div>
              <div>{error.message}</div>
            </div>
          )}

          <div>
            <label
              htmlFor="create-title"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Title *
            </label>
            <input
              id="create-title"
              data-testid="media-create-title-input"
              type="text"
              required
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              disabled={submitting}
              placeholder="e.g. Frieren: Beyond Journey's End"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-type"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Media Type
              </label>
              <select
                id="create-type"
                data-testid="media-create-type-select"
                value={mediaType}
                onChange={(e) => setMediaType(e.target.value)}
                disabled={submitting}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface)',
                  boxSizing: 'border-box',
                }}
              >
                <option value="anime">Anime</option>
                <option value="movie">Movie</option>
                <option value="tv">TV</option>
                <option value="game">Game</option>
              </select>
            </div>

            <div>
              <label
                htmlFor="create-status"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Status
              </label>
              <select
                id="create-status"
                data-testid="media-create-status-select"
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                disabled={submitting}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface)',
                  boxSizing: 'border-box',
                }}
              >
                <option value="planned">Planned</option>
                <option value="in_progress">In Progress</option>
                <option value="completed">Completed</option>
                <option value="paused">Paused</option>
                <option value="dropped">Dropped</option>
              </select>
            </div>
          </div>

          <div>
            <label
              htmlFor="create-summary"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Summary
            </label>
            <input
              id="create-summary"
              data-testid="media-create-summary-input"
              type="text"
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              disabled={submitting}
              placeholder="Short tagline or premise"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-rating"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Rating (0-10)
              </label>
              <input
                id="create-rating"
                data-testid="media-create-rating-input"
                type="number"
                min="0"
                max="10"
                step="0.5"
                value={rating}
                onChange={(e) => setRating(e.target.value === '' ? '' : Number(e.target.value))}
                disabled={submitting}
                placeholder="e.g. 9.5"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="create-year"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Year
              </label>
              <input
                id="create-year"
                data-testid="media-create-year-input"
                type="number"
                value={year}
                onChange={(e) => setYear(e.target.value === '' ? '' : Number(e.target.value))}
                disabled={submitting}
                placeholder="e.g. 2023"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="create-platform"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Platform
              </label>
              <input
                id="create-platform"
                data-testid="media-create-platform-input"
                type="text"
                value={platform}
                onChange={(e) => setPlatform(e.target.value)}
                disabled={submitting}
                placeholder="e.g. Crunchyroll"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '10px' }}>
            <span style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-muted)', display: 'block', marginBottom: '8px' }}>
              Progress (Optional)
            </span>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '12px' }}>
              <div>
                <label
                  htmlFor="create-unit"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Unit
                </label>
                <input
                  id="create-unit"
                  data-testid="media-create-unit-input"
                  type="text"
                  value={progressUnit}
                  onChange={(e) => setProgressUnit(e.target.value)}
                  disabled={submitting}
                  placeholder="episodes, chapters"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="create-current"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Current
                </label>
                <input
                  id="create-current"
                  data-testid="media-create-current-input"
                  type="number"
                  min="0"
                  step="any"
                  value={progressCurrent}
                  onChange={(e) => setProgressCurrent(e.target.value === '' ? '' : Number(e.target.value))}
                  disabled={submitting}
                  placeholder="0"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="create-total"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Total
                </label>
                <input
                  id="create-total"
                  data-testid="media-create-total-input"
                  type="number"
                  min="0"
                  step="any"
                  value={progressTotal}
                  onChange={(e) => setProgressTotal(e.target.value === '' ? '' : Number(e.target.value))}
                  disabled={submitting}
                  placeholder="28"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                  }}
                />
              </div>
            </div>
          </div>

          <div>
            <label
              htmlFor="create-tags"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Tags (comma separated)
            </label>
            <input
              id="create-tags"
              data-testid="media-create-tags-input"
              type="text"
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
              disabled={submitting}
              placeholder="anime, fantasy, masterpiece"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="create-notes"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Notes
            </label>
            <textarea
              id="create-notes"
              data-testid="media-create-notes-input"
              rows={3}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              disabled={submitting}
              placeholder="Personal reflections or initial notes"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
                fontFamily: 'inherit',
              }}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '12px' }}>
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >
              Cancel
            </button>
            <button
              type="submit"
              data-testid="submit-create-media-button"
              disabled={submitting}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-mesh)',
                color: '#fff',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                fontWeight: 500,
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >
              {submitting ? 'Creating...' : 'Create Media Asset'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
