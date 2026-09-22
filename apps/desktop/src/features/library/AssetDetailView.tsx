import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { ExternalRefsPanel } from './panels/ExternalRefsPanel';
import { MediaPanel } from './panels/MediaPanel';
import { MergedRedirectPanel } from './panels/MergedRedirectPanel';
import { ServicePanel } from './panels/ServicePanel';
import { SoftwarePanel } from './panels/SoftwarePanel';
import { UnknownPanel } from './panels/UnknownPanel';
import { getTransport, normalizeDesktopError } from './transport';
import type {
  AssetDetailDto,
  DesktopError,
  MutationReceiptDto,
  SoftwareRecordDto,
  UnknownDetailsDto,
} from './types';

interface AssetDetailViewProps {
  assetId: string;
  onClose?: () => void;
  onFollowRedirect?: (survivingAssetId: string) => void;
  onSelectTag?: (tag: string) => void;
  onAssetUpdated?: (receipt: MutationReceiptDto) => void;
}

export const AssetDetailView: React.FC<AssetDetailViewProps> = ({
  assetId,
  onClose,
  onFollowRedirect,
  onSelectTag,
  onAssetUpdated,
}) => {
  const [detail, setDetail] = useState<AssetDetailDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<DesktopError | null>(null);

  // Mutation Pattern State
  const [isEditingSoftware, setIsEditingSoftware] = useState(false);
  const [draftPurpose, setDraftPurpose] = useState('');
  const [draftNotes, setDraftNotes] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [mutationError, setMutationError] = useState<DesktopError | null>(null);
  const [isConflict, setIsConflict] = useState(false);
  const [receiptNotice, setReceiptNotice] = useState<string | null>(null);

  const transport = getTransport();

  useEffect(() => {
    let cancelled = false;

    const fetchDetail = async () => {
      setLoading(true);
      setError(null);
      setIsEditingSoftware(false);
      setMutationError(null);
      setIsConflict(false);
      setReceiptNotice(null);

      try {
        const result = await transport.getAsset(assetId);
        if (!cancelled) {
          setDetail(result);
        }
      } catch (err: unknown) {
        if (!cancelled) {
          setError(normalizeDesktopError(err));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    fetchDetail();

    return () => {
      cancelled = true;
    };
  }, [assetId, transport]);

  if (loading) {
    return (
      <div
        role="status"
        aria-live="polite"
        style={{
          padding: '32px',
          textAlign: 'center',
          color: 'var(--color-muted)',
        }}
      >
        Loading asset details...
      </div>
    );
  }

  if (error || !detail) {
    return (
      <div
        role="alert"
        style={{
          padding: '24px',
          backgroundColor: 'var(--color-danger-bg)',
          borderRadius: 'var(--radius-md)',
          margin: '16px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '8px' }}>
          <Badge variant="danger">{error?.category || 'Error'}</Badge>
          <span style={{ fontWeight: 600, color: 'var(--color-danger)' }}>
            Failed to load asset details
          </span>
        </div>
        <p style={{ fontSize: '12px', color: 'var(--color-ink)' }}>
          {error?.message || 'Asset could not be found.'}
        </p>
      </div>
    );
  }

  const isSoftware = detail?.details.module === 'software';
  const canEditSoftware = isSoftware && detail?.lifecycle === 'active';

  const handleStartEditSoftware = () => {
    if (!detail || !isSoftware) return;
    const sw = detail.details as SoftwareRecordDto;
    setDraftPurpose(sw.purpose || '');
    setDraftNotes(sw.notes || '');
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);
    setIsEditingSoftware(true);
  };

  const handleCancelEditSoftware = () => {
    setIsEditingSoftware(false);
    setMutationError(null);
    setIsConflict(false);
  };

  const handleSaveSoftware = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!detail || !isSoftware || submitting) return;

    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.softwareCommand({
        action: 'update_metadata',
        asset_id: detail.id,
        expected_revision: detail.revision,
        purpose: draftPurpose.trim() || undefined,
        notes: draftNotes.trim() || undefined,
      });

      // Mandatory Read-Back: UI never assumes success without reading back canonical state
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsEditingSoftware(false);
      setReceiptNotice(
        receipt.changed
          ? `Saved successfully (rev ${receipt.revision})`
          : 'No changes detected'
      );
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      const norm = normalizeDesktopError(err);
      if (norm.category === 'stale_revision') {
        setIsConflict(true);
      }
      setMutationError(norm);
    } finally {
      setSubmitting(false);
    }
  };

  const handleReloadLatest = async () => {
    if (!detail) return;
    try {
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsConflict(false);
      setMutationError(null);
      setIsEditingSoftware(false);
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    }
  };

  // Exhaustive typed module detail renderer
  const renderModuleDetails = () => {
    switch (detail.details.module) {
      case 'media':
        return <MediaPanel record={detail.details} />;
      case 'software':
        if (isEditingSoftware) {
          return (
            <form
              onSubmit={handleSaveSoftware}
              data-testid="software-edit-form"
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '14px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span
                  style={{
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    color: 'var(--color-muted)',
                    fontWeight: 600,
                  }}
                >
                  Edit Software Metadata
                </span>
                <Badge variant="mesh">rev {detail.revision}</Badge>
              </div>

              <div>
                <label
                  htmlFor="software-purpose"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Purpose
                </label>
                <input
                  id="software-purpose"
                  data-testid="software-purpose-input"
                  type="text"
                  value={draftPurpose}
                  onChange={(e) => setDraftPurpose(e.target.value)}
                  disabled={submitting}
                  placeholder="e.g. CLI tool for git repository management"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="software-notes"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Notes
                </label>
                <textarea
                  id="software-notes"
                  data-testid="software-notes-input"
                  rows={3}
                  value={draftNotes}
                  onChange={(e) => setDraftNotes(e.target.value)}
                  disabled={submitting}
                  placeholder="Personal usage notes, configuration tips, etc."
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontFamily: 'inherit',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '4px' }}>
                <button
                  type="button"
                  data-testid="cancel-edit-button"
                  onClick={handleCancelEditSoftware}
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    cursor: submitting ? 'not-allowed' : 'pointer',
                    color: 'var(--color-ink)',
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  data-testid="save-software-button"
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-mesh)',
                    color: '#FFFFFF',
                    border: 'none',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    fontWeight: 500,
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  {submitting ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </form>
          );
        }
        return (
          <SoftwarePanel
            record={detail.details}
            onEdit={canEditSoftware ? handleStartEditSoftware : undefined}
          />
        );
      case 'services':
        return <ServicePanel record={detail.details} />;
      case 'merged_redirect':
        return (
          <MergedRedirectPanel
            record={detail.details}
            onFollowRedirect={(survivorId) => onFollowRedirect?.(survivorId)}
          />
        );
      default:
        return <UnknownPanel record={detail.details as UnknownDetailsDto} />;
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '16px',
        padding: '20px',
        overflowY: 'auto',
        maxHeight: '100%',
      }}
      data-testid="asset-detail-view"
    >
      {/* Header */}
      <div>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginBottom: '8px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexWrap: 'wrap' }}>
            <Badge variant="mesh">{detail.kind}</Badge>
            {detail.lifecycle !== 'active' && (
              <Badge variant={detail.lifecycle === 'archived' ? 'attention' : 'danger'}>
                {detail.lifecycle}
              </Badge>
            )}
            <span
              style={{
                fontSize: '11px',
                fontFamily: 'var(--font-mono)',
                color: 'var(--color-muted)',
                backgroundColor: 'var(--color-canvas)',
                padding: '2px 6px',
                borderRadius: 'var(--radius-sm)',
              }}
              title="Optimistic concurrency revision"
            >
              rev {detail.revision}
            </span>
          </div>

          {onClose && (
            <button
              onClick={onClose}
              aria-label="Close detail view"
              style={{
                background: 'none',
                border: 'none',
                cursor: 'pointer',
                fontSize: '18px',
                color: 'var(--color-muted)',
                padding: '2px 6px',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              ✕
            </button>
          )}
        </div>

        <h2
          style={{
            fontSize: '20px',
            fontWeight: 600,
            color: 'var(--color-ink)',
            wordBreak: 'break-word',
          }}
        >
          {detail.name}
        </h2>
        {detail.summary && (
          <p style={{ color: 'var(--color-muted)', fontSize: '13px', marginTop: '4px' }}>
            {detail.summary}
          </p>
        )}
      </div>

      {/* Lifecycle Notice for Archived Assets */}
      {detail.lifecycle === 'archived' && (
        <div
          style={{
            padding: '10px 14px',
            backgroundColor: 'var(--color-attention-bg)',
            border: '1px solid var(--color-attention)',
            borderRadius: 'var(--radius-md)',
            fontSize: '12px',
            color: 'var(--color-attention)',
          }}
        >
          <strong>Archived Asset:</strong> This asset is preserved in read-only state.
          {detail.archived_at && ` Archived on ${detail.archived_at.slice(0, 10)}.`}
        </div>
      )}

      {/* Receipt Notice */}
      {receiptNotice && (
        <div
          data-testid="mutation-receipt-badge"
          style={{
            padding: '8px 12px',
            backgroundColor: 'var(--color-canvas)',
            border: '1px solid var(--color-mesh)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-mesh)',
            fontSize: '12px',
            fontWeight: 500,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <span>✓ {receiptNotice}</span>
          <button
            type="button"
            onClick={() => setReceiptNotice(null)}
            aria-label="Dismiss notice"
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              color: 'var(--color-muted)',
              fontSize: '14px',
            }}
          >
            ✕
          </button>
        </div>
      )}

      {/* Edit Conflict Banner (Stale Revision) - No auto retry */}
      {isConflict && (
        <div
          role="alert"
          data-testid="mutation-conflict-banner"
          style={{
            padding: '12px 14px',
            backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-danger)',
            fontSize: '12px',
            display: 'flex',
            flexDirection: 'column',
            gap: '8px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Badge variant="danger">stale_revision</Badge>
            <strong>Edit Conflict Detected</strong>
          </div>
          <div>
            {mutationError?.message ||
              `This asset has been modified elsewhere (expected rev ${detail.revision}). Your edits have been preserved, but were not saved.`}
          </div>
          <div>
            <button
              type="button"
              data-testid="reload-latest-button"
              onClick={handleReloadLatest}
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                cursor: 'pointer',
                fontSize: '11px',
                fontWeight: 500,
                color: 'var(--color-ink)',
              }}
            >
              Discard Draft & Reload Latest
            </button>
          </div>
        </div>
      )}

      {/* Mutation Error / Validation Banner */}
      {!isConflict && mutationError && (
        <div
          role="alert"
          data-testid={
            mutationError.category === 'invalid_input'
              ? 'mutation-validation-banner'
              : 'mutation-error-banner'
          }
          style={{
            padding: '10px 14px',
            backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-danger)',
            fontSize: '12px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '4px' }}>
            <Badge variant="danger">{mutationError.category}</Badge>
            <strong>
              {mutationError.category === 'invalid_input'
                ? 'Validation Error'
                : 'Mutation Failed'}
            </strong>
          </div>
          <div>{mutationError.message}</div>
        </div>
      )}

      {/* Typed Module Details */}
      {renderModuleDetails()}

      {/* Canonical Metadata Card */}
      <div
        style={{
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
          borderRadius: 'var(--radius-md)',
          padding: '14px',
          fontSize: '12px',
          display: 'flex',
          flexDirection: 'column',
          gap: '8px',
        }}
      >
        <div
          style={{
            fontSize: '11px',
            textTransform: 'uppercase',
            color: 'var(--color-muted)',
            fontWeight: 600,
            marginBottom: '2px',
          }}
        >
          Canonical Identity
        </div>

        <div>
          <div style={{ color: 'var(--color-muted)', fontSize: '11px' }}>Asset ID</div>
          <code
            style={{
              fontSize: '11px',
              wordBreak: 'break-all',
              backgroundColor: 'var(--color-canvas)',
              padding: '2px 4px',
              borderRadius: 'var(--radius-sm)',
              display: 'block',
            }}
          >
            {detail.id}
          </code>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: '8px' }}>
          <div>
            <div style={{ color: 'var(--color-muted)', fontSize: '11px' }}>Created</div>
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: '11px' }}>
              {detail.created_at.slice(0, 19).replace('T', ' ')}
            </div>
          </div>
          <div>
            <div style={{ color: 'var(--color-muted)', fontSize: '11px' }}>Updated</div>
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: '11px' }}>
              {detail.updated_at.slice(0, 19).replace('T', ' ')}
            </div>
          </div>
        </div>
      </div>

      {/* Tags */}
      {detail.tags.length > 0 && (
        <div
          style={{
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-md)',
            padding: '14px',
          }}
        >
          <div
            style={{
              fontSize: '11px',
              textTransform: 'uppercase',
              color: 'var(--color-muted)',
              fontWeight: 600,
              marginBottom: '8px',
            }}
          >
            Tags ({detail.tags.length})
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
            {detail.tags.map((t) => (
              <button
                key={t}
                onClick={() => onSelectTag?.(t)}
                title={`Filter by tag #${t}`}
                style={{
                  all: 'unset',
                  cursor: onSelectTag ? 'pointer' : 'default',
                }}
              >
                <Badge variant="muted">#{t}</Badge>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* External References */}
      <ExternalRefsPanel refs={detail.external_refs} />
    </div>
  );
};
