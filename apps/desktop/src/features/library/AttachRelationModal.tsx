import React, { useState, useEffect } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type { AppCapabilities, AssetSummary, DesktopError, MutationReceiptDto } from './types';

interface AttachRelationModalProps {
  isOpen: boolean;
  onClose: () => void;
  sourceAsset: { id: string; name: string };
  capabilities: AppCapabilities | null;
  onAttached: (receipt: MutationReceiptDto) => void;
}

export const AttachRelationModal: React.FC<AttachRelationModalProps> = ({
  isOpen,
  onClose,
  sourceAsset,
  capabilities,
  onAttached,
}) => {
  const [relationType, setRelationType] = useState('depends_on');
  const [searchTargetText, setSearchTargetText] = useState('');
  const [targetSearchResults, setTargetSearchResults] = useState<AssetSummary[]>([]);
  const [selectedTarget, setSelectedTarget] = useState<AssetSummary | null>(null);
  const [note, setNote] = useState('');
  const [searching, setSearching] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  const transport = getTransport();

  const availableTypes =
    capabilities?.relation_types && capabilities.relation_types.length > 0
      ? capabilities.relation_types
      : [
          'depends_on',
          'dependency_of',
          'uses',
          'used_by',
          'installed_via',
          'installs',
          'hosted_on',
          'hosts',
          'points_to',
          'pointed_to_by',
          'related_to',
        ];

  useEffect(() => {
    if (!isOpen) {
      setSearchTargetText('');
      setSelectedTarget(null);
      setNote('');
      setError(null);
      setTargetSearchResults([]);
      return;
    }

    let active = true;
    const searchTimer = setTimeout(async () => {
      setSearching(true);
      try {
        if (searchTargetText.trim()) {
          const page = await transport.searchAssets({
            text: searchTargetText.trim(),
            limit: 10,
          });
          if (active) {
            setTargetSearchResults(page.items.filter((a) => a.id !== sourceAsset.id));
          }
        } else {
          const page = await transport.listAssets({
            lifecycle: 'active',
            limit: 10,
          });
          if (active) {
            setTargetSearchResults(page.items.filter((a) => a.id !== sourceAsset.id));
          }
        }
      } catch {
        // ignore search typing errors
      } finally {
        if (active) setSearching(false);
      }
    }, 200);

    return () => {
      active = false;
      clearTimeout(searchTimer);
    };
  }, [isOpen, searchTargetText, sourceAsset.id, transport]);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedTarget) {
      setError({
        category: 'invalid_input',
        message: 'Please select a target asset',
      });
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const receipt = await transport.relationAttach({
        source_asset_id: sourceAsset.id,
        relation_type: relationType,
        target_asset_id: selectedTarget.id,
        note: note.trim() || undefined,
      });

      onAttached(receipt);
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
      aria-label="Add Relation"
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.45)',
        backdropFilter: 'blur(2px)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 120,
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
          maxWidth: '520px',
          maxHeight: '85vh',
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
            <Badge variant="mesh">Relation</Badge>
            <h3 style={{ margin: 0, fontSize: '15px', fontWeight: 600, color: 'var(--color-ink)' }}>
              Add Relation from {sourceAsset.name}
            </h3>
          </div>
          <button
            onClick={onClose}
            disabled={submitting}
            aria-label="Close modal"
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
          data-testid="attach-relation-form"
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
              data-testid="attach-relation-error"
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
              htmlFor="attach-relation-type"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Relation Type *
            </label>
            <select
              id="attach-relation-type"
              data-testid="attach-relation-type-select"
              value={relationType}
              onChange={(e) => setRelationType(e.target.value)}
              disabled={submitting}
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                backgroundColor: 'var(--color-canvas)',
                boxSizing: 'border-box',
              }}
            >
              {availableTypes.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label
              htmlFor="attach-target-search"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Select Target Asset *
            </label>
            <input
              id="attach-target-search"
              data-testid="attach-target-search-input"
              type="text"
              value={searchTargetText}
              onChange={(e) => setSearchTargetText(e.target.value)}
              disabled={submitting}
              placeholder="Search target asset by name or tag..."
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
                backgroundColor: 'var(--color-canvas)',
              }}
            />

            {selectedTarget && (
              <div
                data-testid="selected-target-card"
                style={{
                  marginTop: '6px',
                  padding: '6px 10px',
                  backgroundColor: 'var(--color-mesh-bg)',
                  border: '1px solid var(--color-mesh)',
                  borderRadius: 'var(--radius-sm)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  fontSize: '12px',
                }}
              >
                <span>
                  <strong>Selected:</strong> {selectedTarget.name} ({selectedTarget.kind})
                </span>
                <button
                  type="button"
                  onClick={() => setSelectedTarget(null)}
                  style={{
                    background: 'none',
                    border: 'none',
                    color: 'var(--color-mesh)',
                    cursor: 'pointer',
                    fontWeight: 600,
                  }}
                >
                  Change
                </button>
              </div>
            )}

            {!selectedTarget && (
              <div
                data-testid="target-search-results"
                style={{
                  maxHeight: '140px',
                  overflowY: 'auto',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  marginTop: '4px',
                  backgroundColor: 'var(--color-surface)',
                }}
              >
                {searching && (
                  <div style={{ padding: '8px', fontSize: '12px', color: 'var(--color-muted)' }}>
                    Searching...
                  </div>
                )}
                {!searching && targetSearchResults.length === 0 && (
                  <div style={{ padding: '8px', fontSize: '12px', color: 'var(--color-muted)' }}>
                    No matching assets found.
                  </div>
                )}
                {!searching &&
                  targetSearchResults.map((asset) => (
                    <button
                      key={asset.id}
                      type="button"
                      data-testid={`target-asset-option-${asset.id}`}
                      onClick={() => {
                        setSelectedTarget(asset);
                      }}
                      style={{
                        width: '100%',
                        textAlign: 'left',
                        padding: '6px 10px',
                        border: 'none',
                        borderBottom: '1px solid var(--color-border-subtle)',
                        background: 'none',
                        cursor: 'pointer',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        fontSize: '12px',
                      }}
                    >
                      <span style={{ fontWeight: 500, color: 'var(--color-ink)' }}>{asset.name}</span>
                      <Badge variant="muted">{asset.kind}</Badge>
                    </button>
                  ))}
              </div>
            )}
          </div>

          <div>
            <label
              htmlFor="attach-relation-note"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Note (Optional)
            </label>
            <input
              id="attach-relation-note"
              data-testid="attach-relation-note-input"
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              disabled={submitting}
              placeholder="e.g. Depends on runtime for execution"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
                backgroundColor: 'var(--color-canvas)',
              }}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '10px' }}>
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
              data-testid="submit-attach-relation-button"
              disabled={submitting || !selectedTarget}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-mesh)',
                color: '#fff',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                fontWeight: 500,
                cursor: submitting || !selectedTarget ? 'not-allowed' : 'pointer',
                opacity: submitting || !selectedTarget ? 0.7 : 1,
              }}
            >
              {submitting ? 'Connecting...' : 'Attach Relation'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
