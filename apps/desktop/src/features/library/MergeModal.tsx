import React, { useState, useEffect, useCallback } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type {
  DesktopError,
  DuplicateCandidateDto,
  MergePreviewDto,
  MutationReceiptDto,
} from './types';

interface MergeModalProps {
  candidate: DuplicateCandidateDto;
  isOpen: boolean;
  onClose: () => void;
  onMerged: (receipt: MutationReceiptDto, winnerId: string) => void;
}

export const MergeModal: React.FC<MergeModalProps> = ({
  candidate,
  isOpen,
  onClose,
  onMerged,
}) => {
  // No winner is preselected: the completion condition for P5-08 is that the
  // UI must not pick a winner on the user's behalf. `null` means "the user has
  // not chosen yet", and nothing downstream — preview, confirmation, apply —
  // runs until they do.
  const [winnerId, setWinnerId] = useState<string | null>(null);
  const loserId =
    winnerId === null
      ? null
      : winnerId === candidate.left.id
        ? candidate.right.id
        : candidate.left.id;

  const [preview, setPreview] = useState<MergePreviewDto | null>(null);
  const [loadingPreview, setLoadingPreview] = useState<boolean>(false);
  const [merging, setMerging] = useState<boolean>(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [confirmed, setConfirmed] = useState<boolean>(false);

  const transport = getTransport();

  const loadPreview = useCallback(async (winner: string, loser: string) => {
    setLoadingPreview(true);
    setError(null);
    setConfirmed(false);
    try {
      const res = await transport.mergePreview(winner, loser);
      setPreview(res);
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
      setPreview(null);
    } finally {
      setLoadingPreview(false);
    }
  }, [transport]);

  // A reused modal must not keep a previous review's choice: opening it on a
  // different candidate pair starts from "no survivor chosen".
  useEffect(() => {
    setWinnerId(null);
    setPreview(null);
    setConfirmed(false);
    setError(null);
  }, [candidate.left.id, candidate.right.id]);

  useEffect(() => {
    // Only once the user has chosen. A preview is a statement about one
    // (winner, loser) pair, so firing it against a default would reinstate the
    // automatic choice the spec forbids.
    if (isOpen && winnerId !== null && loserId !== null) {
      loadPreview(winnerId, loserId);
    }
  }, [isOpen, winnerId, loserId, loadPreview]);

  if (!isOpen) return null;

  const handleSelectWinner = (id: string) => {
    if (id !== winnerId) {
      setWinnerId(id);
    }
  };

  const winnerAsset = winnerId === null ? null
    : winnerId === candidate.left.id ? candidate.left : candidate.right;
  const loserAsset = loserId === null ? null
    : loserId === candidate.left.id ? candidate.left : candidate.right;

  const handleExecuteMerge = async () => {
    if (!preview || !preview.can_merge || merging) return;
    if (winnerId === null || loserId === null) return;
    setMerging(true);
    setError(null);

    try {
      const receipt = await transport.mergeApply({
        winner_id: winnerId,
        loser_id: loserId,
        expected_winner_revision: preview.winner_revision,
        expected_loser_revision: preview.loser_revision,
      });
      onMerged(receipt, winnerId);
      onClose();
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setMerging(false);
    }
  };

  return (
    <div
      data-testid="merge-modal-backdrop"
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.5)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
        padding: '20px',
      }}
    >
      <div
        data-testid="merge-modal"
        style={{
          backgroundColor: 'var(--color-surface)',
          borderRadius: 'var(--radius-md)',
          border: '1px solid var(--color-border)',
          width: '100%',
          maxWidth: '680px',
          maxHeight: '90vh',
          display: 'flex',
          flexDirection: 'column',
          boxShadow: '0 8px 30px rgba(0, 0, 0, 0.2)',
          overflow: 'hidden',
        }}
      >
        {/* Modal Header */}
        <div
          style={{
            padding: '16px 20px',
            borderBottom: '1px solid var(--color-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            backgroundColor: 'var(--color-canvas)',
          }}
        >
          <div>
            <h3 style={{ margin: 0, fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)' }}>
              Explicit Merge Review
            </h3>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginTop: '2px' }}>
              Select which asset survives. Loser becomes a permanent redirect tombstone.
            </div>
          </div>
          <button
            type="button"
            data-testid="close-merge-modal"
            onClick={onClose}
            disabled={merging}
            style={{
              background: 'none',
              border: 'none',
              fontSize: '18px',
              cursor: merging ? 'not-allowed' : 'pointer',
              color: 'var(--color-muted)',
              lineHeight: 1,
            }}
          >
            ✕
          </button>
        </div>

        {/* Modal Body */}
        <div
          style={{
            padding: '20px',
            overflowY: 'auto',
            display: 'flex',
            flexDirection: 'column',
            gap: '16px',
          }}
        >
          {/* Winner Selection Cards */}
          <div>
            <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)', marginBottom: '8px' }}>
              1. Choose Surviving Asset (Winner):
            </div>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginBottom: '8px' }}>
              Nothing is preselected — the survivor is yours to choose.
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
              {/* Left Candidate Card */}
              <div
                data-testid="merge-option-left"
                onClick={() => handleSelectWinner(candidate.left.id)}
                style={{
                  padding: '12px 14px',
                  borderRadius: 'var(--radius-sm)',
                  border: `2px solid ${winnerId === candidate.left.id ? 'var(--color-mesh)' : 'var(--color-border)'}`,
                  backgroundColor: winnerId === candidate.left.id ? 'var(--color-canvas)' : 'var(--color-surface)',
                  cursor: 'pointer',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '6px',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <label
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px',
                      fontWeight: 600,
                      fontSize: '13px',
                      cursor: 'pointer',
                    }}
                  >
                    <input
                      type="radio"
                      name="winner-selection"
                      checked={winnerId === candidate.left.id}
                      onChange={() => handleSelectWinner(candidate.left.id)}
                      data-testid="radio-select-left"
                    />
                    {candidate.left.name}
                  </label>
                  <Badge variant={winnerId === candidate.left.id ? 'mesh' : 'muted'}>
                    {winnerId === candidate.left.id
                      ? 'Survivor'
                      : winnerId === null
                        ? 'Not selected'
                        : 'Loser'}
                  </Badge>
                </div>
                <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                  <Badge variant="muted">{candidate.left.kind}</Badge>
                  <span style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{candidate.left.lifecycle}</span>
                </div>
                {candidate.left.subtitle && (
                  <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>{candidate.left.subtitle}</div>
                )}
              </div>

              {/* Right Candidate Card */}
              <div
                data-testid="merge-option-right"
                onClick={() => handleSelectWinner(candidate.right.id)}
                style={{
                  padding: '12px 14px',
                  borderRadius: 'var(--radius-sm)',
                  border: `2px solid ${winnerId === candidate.right.id ? 'var(--color-mesh)' : 'var(--color-border)'}`,
                  backgroundColor: winnerId === candidate.right.id ? 'var(--color-canvas)' : 'var(--color-surface)',
                  cursor: 'pointer',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '6px',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <label
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px',
                      fontWeight: 600,
                      fontSize: '13px',
                      cursor: 'pointer',
                    }}
                  >
                    <input
                      type="radio"
                      name="winner-selection"
                      checked={winnerId === candidate.right.id}
                      onChange={() => handleSelectWinner(candidate.right.id)}
                      data-testid="radio-select-right"
                    />
                    {candidate.right.name}
                  </label>
                  <Badge variant={winnerId === candidate.right.id ? 'mesh' : 'muted'}>
                    {winnerId === candidate.right.id
                      ? 'Survivor'
                      : winnerId === null
                        ? 'Not selected'
                        : 'Loser'}
                  </Badge>
                </div>
                <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                  <Badge variant="muted">{candidate.right.kind}</Badge>
                  <span style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{candidate.right.lifecycle}</span>
                </div>
                {candidate.right.subtitle && (
                  <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>{candidate.right.subtitle}</div>
                )}
              </div>
            </div>
          </div>

          {/* Evidence Checklist */}
          <div
            style={{
              padding: '10px 14px',
              backgroundColor: 'var(--color-canvas)',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid var(--color-border)',
            }}
          >
            <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-muted)', marginBottom: '4px' }}>
              Deterministic Evidence:
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
              {candidate.evidence_labels.map((lbl, idx) => (
                <span
                  key={idx}
                  data-testid="merge-evidence-tag"
                  style={{
                    fontSize: '11px',
                    padding: '2px 8px',
                    backgroundColor: 'var(--color-surface)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    color: 'var(--color-ink)',
                  }}
                >
                  ✓ {lbl}
                </span>
              ))}
            </div>
          </div>

          {/* Merge Preview Results */}
          <div>
            <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)', marginBottom: '8px' }}>
              2. Merge Impact Preview:
            </div>

            {winnerId === null ? (
              <div
                data-testid="merge-choice-required"
                style={{
                  padding: '16px',
                  backgroundColor: 'var(--color-canvas)',
                  border: '1px dashed var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  color: 'var(--color-muted)',
                  fontSize: '12px',
                  textAlign: 'center',
                }}
              >
                Choose a surviving asset above to see what this merge would move.
              </div>
            ) : loadingPreview ? (
              <div
                data-testid="merge-preview-loading"
                style={{ padding: '16px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '12px' }}
              >
                Analyzing field conflicts and relations impact...
              </div>
            ) : preview ? (
              <div
                data-testid="merge-preview-container"
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '10px',
                  backgroundColor: 'var(--color-canvas)',
                  padding: '14px',
                  borderRadius: 'var(--radius-sm)',
                  border: `1px solid ${preview.can_merge ? 'var(--color-border)' : 'var(--color-danger, #d32f2f)'}`,
                }}
              >
                {/* Conflict Banner */}
                {!preview.can_merge && (
                  <div
                    data-testid="merge-conflict-alert"
                    style={{
                      padding: '10px 12px',
                      backgroundColor: 'var(--color-danger-subtle, #ffebee)',
                      border: '1px solid var(--color-danger, #d32f2f)',
                      borderRadius: 'var(--radius-sm)',
                      color: 'var(--color-danger, #d32f2f)',
                      fontSize: '12px',
                    }}
                  >
                    <div style={{ fontWeight: 600, marginBottom: '4px' }}>
                      ⚠ Merge Blocked: Cannot merge due to field conflicts
                    </div>
                    <ul style={{ margin: '0 0 0 16px', padding: 0 }}>
                      {preview.conflicts.map((c, i) => (
                        <li key={i}>{c}</li>
                      ))}
                    </ul>
                  </div>
                )}

                {preview.can_merge && (
                  <div
                    data-testid="merge-ready-alert"
                    style={{
                      padding: '8px 12px',
                      backgroundColor: 'var(--color-success-subtle, #e8f5e9)',
                      border: '1px solid var(--color-success, #2e7d32)',
                      borderRadius: 'var(--radius-sm)',
                      color: 'var(--color-success, #2e7d32)',
                      fontSize: '12px',
                      fontWeight: 500,
                    }}
                  >
                    ✓ No blocking conflicts. Merge is safe and lossless.
                  </div>
                )}

                {/* Impact details */}
                <div style={{ fontSize: '12px', display: 'flex', flexDirection: 'column', gap: '6px' }}>
                  <div data-testid="impact-lifecycle">
                    <strong>Survivor:</strong> {winnerAsset?.name} ({winnerAsset?.id}) remains active.
                  </div>
                  <div data-testid="impact-tombstone">
                    <strong>Tombstone:</strong> {loserAsset?.name} ({loserAsset?.id}) will be marked merged and redirect to {winnerAsset?.name}.
                  </div>
                  <div data-testid="impact-tags">
                    <strong>Tags Transfer:</strong>{' '}
                    {preview.transferred_tags.length > 0
                      ? `+${preview.transferred_tags.length} tag(s) added to winner: ${preview.transferred_tags.map((t) => `#${t}`).join(', ')}`
                      : 'None'}
                  </div>
                  <div data-testid="impact-relations">
                    <strong>Relations:</strong> {preview.transferred_relations_count} relation(s) re-pointed to winner; {preview.redundant_relations_count} redundant relation(s) dropped.
                  </div>
                  {preview.notes.map((n, i) => (
                    <div key={i} style={{ color: 'var(--color-muted)' }}>
                      ℹ {n}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>

          {/* Error Banner */}
          {error && (
            <div
              data-testid="merge-error-banner"
              style={{
                padding: '10px 14px',
                backgroundColor: 'var(--color-danger-subtle, #ffebee)',
                border: '1px solid var(--color-danger, #d32f2f)',
                borderRadius: 'var(--radius-sm)',
                color: 'var(--color-danger, #d32f2f)',
                fontSize: '12px',
              }}
            >
              <strong>Error [{error.category}]:</strong> {error.message}
            </div>
          )}

          {/* Confirmation Checkbox */}
          {preview?.can_merge && winnerAsset && loserAsset && (
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '12px' }}>
              <input
                id="merge-confirm-checkbox"
                data-testid="merge-confirm-checkbox"
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
                style={{ cursor: 'pointer' }}
              />
              <label htmlFor="merge-confirm-checkbox" style={{ cursor: 'pointer', color: 'var(--color-ink)' }}>
                I confirm merging <strong>{loserAsset.name}</strong> into <strong>{winnerAsset.name}</strong>.
              </label>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div
          style={{
            padding: '14px 20px',
            borderTop: '1px solid var(--color-border)',
            backgroundColor: 'var(--color-canvas)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'flex-end',
            gap: '10px',
          }}
        >
          <button
            type="button"
            data-testid="cancel-merge-button"
            onClick={onClose}
            disabled={merging}
            style={{
              padding: '6px 14px',
              fontSize: '12px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              cursor: merging ? 'not-allowed' : 'pointer',
              color: 'var(--color-ink)',
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            data-testid="execute-merge-button"
            disabled={!preview || !preview.can_merge || !confirmed || merging}
            onClick={handleExecuteMerge}
            style={{
              padding: '6px 16px',
              fontSize: '12px',
              fontWeight: 500,
              backgroundColor:
                !preview || !preview.can_merge || !confirmed || merging
                  ? 'var(--color-border)'
                  : 'var(--color-mesh)',
              color: '#FFFFFF',
              border: 'none',
              borderRadius: 'var(--radius-sm)',
              cursor:
                !preview || !preview.can_merge || !confirmed || merging
                  ? 'not-allowed'
                  : 'pointer',
            }}
          >
            {merging ? 'Merging...' : 'Execute Merge'}
          </button>
        </div>
      </div>
    </div>
  );
};
