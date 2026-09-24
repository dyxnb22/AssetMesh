import React, { useState, useEffect, useCallback } from 'react';
import { Badge } from '../../ui/Badge';
import { MergeModal } from './MergeModal';
import { getTransport, normalizeDesktopError } from './transport';
import type {
  AppCapabilities,
  DesktopError,
  DuplicateCandidateDto,
  DuplicateQuery,
  MutationReceiptDto,
} from './types';
import { t } from '../../i18n';

interface DuplicateReviewProps {
  capabilities: AppCapabilities | null;
  onOpenAssetDetail?: (assetId: string) => void;
  onAssetMerged?: (receipt: MutationReceiptDto, winnerId: string) => void;
}

export const DuplicateReview: React.FC<DuplicateReviewProps> = ({
  capabilities,
  onOpenAssetDetail,
  onAssetMerged,
}) => {
  const [candidates, setCandidates] = useState<DuplicateCandidateDto[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<DesktopError | null>(null);

  // Filters
  const [kindFilter, setKindFilter] = useState<string>('all');
  const [includeArchived, setIncludeArchived] = useState<boolean>(true);

  // Ephemeral dismissal set (per-session only; does not claim persistent suppression)
  const [dismissedPairs, setDismissedPairs] = useState<Set<string>>(new Set());

  // Merge modal state
  const [selectedCandidate, setSelectedCandidate] = useState<DuplicateCandidateDto | null>(null);
  const [receiptNotice, setReceiptNotice] = useState<string | null>(null);

  const transport = getTransport();

  const loadCandidates = useCallback(async () => {
    setLoading(true);
    setError(null);

    const query: DuplicateQuery = {
      kinds: kindFilter !== 'all' ? [kindFilter] : undefined,
      include_archived: includeArchived,
      limit: 50,
      offset: 0,
    };

    try {
      const page = await transport.duplicateCandidates(query);
      setCandidates(page.items);
      setTotal(page.total);
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setLoading(false);
    }
  }, [kindFilter, includeArchived, transport]);

  useEffect(() => {
    loadCandidates();
  }, [loadCandidates]);

  const pairKey = (cand: DuplicateCandidateDto) => `${cand.left.id}:${cand.right.id}`;

  const handleDismiss = (cand: DuplicateCandidateDto) => {
    setDismissedPairs((prev) => {
      const next = new Set(prev);
      next.add(pairKey(cand));
      return next;
    });
  };

  const handleMerged = (receipt: MutationReceiptDto, winnerId: string) => {
    setReceiptNotice(t('Successfully merged assets into {id}', { id: winnerId }));
    onAssetMerged?.(receipt, winnerId);
    loadCandidates();
  };

  const visibleCandidates = candidates.filter((c) => !dismissedPairs.has(pairKey(c)));

  return (
    <div
      data-testid="duplicate-review-container"
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        backgroundColor: 'var(--color-surface)',
        overflow: 'hidden',
      }}
    >
      {/* Header Toolbar */}
      <div
        style={{
          padding: '16px',
          borderBottom: '1px solid var(--color-border)',
          display: 'flex',
          flexDirection: 'column',
          gap: '12px',
          backgroundColor: 'var(--color-canvas)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <h2 style={{ fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)', margin: 0 }}>{t('Duplicate Review')}</h2>
              {total != null && (
                <Badge variant="muted">
                  {t(total === 1 ? '{n} candidate' : '{n} candidates', { n: total })}
                </Badge>
              )}
            </div>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginTop: '2px' }}>{t('Deterministic evidence-based candidates. Review and merge explicitly without automated winner selection.')}</div>
          </div>
          <button
            type="button"
            data-testid="refresh-duplicates-button"
            onClick={() => loadCandidates()}
            disabled={loading}
            style={{
              padding: '4px 10px',
              fontSize: '12px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              cursor: loading ? 'not-allowed' : 'pointer',
              color: 'var(--color-ink)',
            }}
          >
            {loading ? t('Refreshing...') : t('↻ Refresh')}
          </button>
        </div>

        {/* Filter Toolbar */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '16px',
            flexWrap: 'wrap',
            fontSize: '12px',
          }}
        >
          {/* Kind Filter */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <label htmlFor="dup-kind-select" style={{ color: 'var(--color-muted)' }}>{t('Kind:')}</label>
            <select
              id="dup-kind-select"
              data-testid="duplicate-kind-filter"
              value={kindFilter}
              onChange={(e) => setKindFilter(e.target.value)}
              style={{
                padding: '3px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
              }}
            >
              <option value="all">{t('All Kinds')}</option>
              {capabilities?.asset_kinds.map((k) => (
                <option key={k} value={k}>
                  {t(k)}
                </option>
              ))}
            </select>
          </div>

          {/* Include Archived Checkbox */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <input
              id="include-archived-dup"
              data-testid="include-archived-checkbox"
              type="checkbox"
              checked={includeArchived}
              onChange={(e) => setIncludeArchived(e.target.checked)}
              style={{ cursor: 'pointer' }}
            />
            <label htmlFor="include-archived-dup" style={{ cursor: 'pointer', color: 'var(--color-ink)' }}>{t('Include Archived Assets')}</label>
          </div>

          {dismissedPairs.size > 0 && (
            <button
              type="button"
              data-testid="reset-dismissed-button"
              onClick={() => setDismissedPairs(new Set())}
              style={{
                background: 'none',
                border: 'none',
                padding: 0,
                fontSize: '11px',
                color: 'var(--color-mesh)',
                cursor: 'pointer',
                textDecoration: 'underline',
              }}
            >{t('Reset ')}{dismissedPairs.size}{t(' dismissed')}</button>
          )}
        </div>
      </div>

      {/* Notice Banner */}
      {receiptNotice && (
        <div
          data-testid="duplicate-receipt-notice"
          style={{
            margin: '12px 16px 0',
            padding: '8px 12px',
            backgroundColor: 'var(--color-canvas)',
            border: '1px solid var(--color-mesh)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--color-mesh)',
            fontSize: '12px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <span>✓ {t(receiptNotice)}</span>
          <button
            type="button"
            onClick={() => setReceiptNotice(null)}
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--color-muted)' }}
          >
            ✕
          </button>
        </div>
      )}

      {/* Error State */}
      {error && (
        <div
          data-testid="duplicate-error-banner"
          style={{
            margin: '12px 16px',
            padding: '10px 14px',
            backgroundColor: 'var(--color-danger-subtle, #ffebee)',
            border: '1px solid var(--color-danger, #d32f2f)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--color-danger, #d32f2f)',
            fontSize: '12px',
          }}
        >
          <strong>{t('Error [')}{t(error.category)}]:</strong> {t(error.message)}
        </div>
      )}

      {/* Candidates List */}
      <div
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '16px',
        }}
      >
        {loading && candidates.length === 0 ? (
          <div
            data-testid="duplicate-loading-indicator"
            style={{ textAlign: 'center', padding: '32px', color: 'var(--color-muted)', fontSize: '13px' }}
          >{t('Scanning library for duplicate candidate pairs...')}</div>
        ) : visibleCandidates.length === 0 ? (
          <div
            data-testid="duplicate-empty-state"
            style={{
              textAlign: 'center',
              padding: '48px 16px',
              color: 'var(--color-muted)',
              fontSize: '13px',
              backgroundColor: 'var(--color-canvas)',
              borderRadius: 'var(--radius-md)',
              border: '1px dashed var(--color-border)',
            }}
          >{t('No duplicate candidates detected.')}</div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            {visibleCandidates.map((cand) => {
              const k = pairKey(cand);
              return (
                <div
                  key={k}
                  data-testid={`candidate-card-${k}`}
                  style={{
                    backgroundColor: 'var(--color-surface)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-md)',
                    padding: '14px',
                    display: 'flex',
                    flexDirection: 'column',
                    gap: '12px',
                  }}
                >
                  {/* Evidence Header */}
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      borderBottom: '1px solid var(--color-border)',
                      paddingBottom: '8px',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
                      <span
                        style={{
                          fontSize: '11px',
                          textTransform: 'uppercase',
                          fontWeight: 600,
                          color: 'var(--color-muted)',
                        }}
                      >{t('Evidence')}</span>
                      {cand.evidence_labels.map((lbl, idx) => (
                        <span
                          key={idx}
                          data-testid="candidate-evidence-badge"
                          style={{
                            fontSize: '11px',
                            padding: '2px 8px',
                            backgroundColor: 'var(--color-canvas)',
                            border: '1px solid var(--color-border)',
                            borderRadius: 'var(--radius-sm)',
                            color: 'var(--color-mesh)',
                            fontWeight: 500,
                          }}
                        >
                          {lbl}
                        </span>
                      ))}
                    </div>

                    <div style={{ display: 'flex', gap: '6px' }}>
                      <button
                        type="button"
                        data-testid={`dismiss-candidate-${k}`}
                        onClick={() => handleDismiss(cand)}
                        style={{
                          padding: '3px 8px',
                          fontSize: '11px',
                          backgroundColor: 'var(--color-canvas)',
                          border: '1px solid var(--color-border)',
                          borderRadius: 'var(--radius-sm)',
                          cursor: 'pointer',
                          color: 'var(--color-muted)',
                        }}
                      >{t('Dismiss')}</button>
                      <button
                        type="button"
                        data-testid={`review-merge-${k}`}
                        onClick={() => setSelectedCandidate(cand)}
                        style={{
                          padding: '3px 10px',
                          fontSize: '11px',
                          fontWeight: 500,
                          backgroundColor: 'var(--color-mesh)',
                          color: '#FFFFFF',
                          border: 'none',
                          borderRadius: 'var(--radius-sm)',
                          cursor: 'pointer',
                        }}
                      >{t('Review & Merge')}</button>
                    </div>
                  </div>

                  {/* Side-by-Side Asset Comparison */}
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
                    {/* Left Asset */}
                    <div
                      data-testid={`candidate-asset-left-${cand.left.id}`}
                      style={{
                        padding: '10px 12px',
                        backgroundColor: 'var(--color-canvas)',
                        borderRadius: 'var(--radius-sm)',
                        border: '1px solid var(--color-border)',
                        display: 'flex',
                        flexDirection: 'column',
                        gap: '4px',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                        {onOpenAssetDetail ? (
                          <button
                            type="button"
                            onClick={() => onOpenAssetDetail(cand.left.id)}
                            style={{
                              background: 'none',
                              border: 'none',
                              padding: 0,
                              fontWeight: 600,
                              fontSize: '13px',
                              color: 'var(--color-ink)',
                              cursor: 'pointer',
                              textAlign: 'left',
                              textDecoration: 'underline',
                            }}
                          >
                            {cand.left.name}
                          </button>
                        ) : (
                          <span style={{ fontWeight: 600, fontSize: '13px', color: 'var(--color-ink)' }}>
                            {cand.left.name}
                          </span>
                        )}
                        <Badge variant="muted">{t(cand.left.lifecycle)}</Badge>
                      </div>
                      <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                        <Badge variant="mesh">{t(cand.left.kind)}</Badge>
                        <span
                          style={{
                            fontSize: '10px',
                            fontFamily: 'var(--font-mono)',
                            color: 'var(--color-muted)',
                          }}
                        >
                          {cand.left.id}
                        </span>
                      </div>
                      {cand.left.subtitle && (
                        <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginTop: '2px' }}>
                          {cand.left.subtitle}
                        </div>
                      )}
                      {cand.left.tags && cand.left.tags.length > 0 && (
                        <div style={{ display: 'flex', gap: '4px', flexWrap: 'wrap', marginTop: '4px' }}>
                          {cand.left.tags.map((t) => (
                            <span key={t} style={{ fontSize: '10px', color: 'var(--color-muted)' }}>
                              #{t}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>

                    {/* Right Asset */}
                    <div
                      data-testid={`candidate-asset-right-${cand.right.id}`}
                      style={{
                        padding: '10px 12px',
                        backgroundColor: 'var(--color-canvas)',
                        borderRadius: 'var(--radius-sm)',
                        border: '1px solid var(--color-border)',
                        display: 'flex',
                        flexDirection: 'column',
                        gap: '4px',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                        {onOpenAssetDetail ? (
                          <button
                            type="button"
                            onClick={() => onOpenAssetDetail(cand.right.id)}
                            style={{
                              background: 'none',
                              border: 'none',
                              padding: 0,
                              fontWeight: 600,
                              fontSize: '13px',
                              color: 'var(--color-ink)',
                              cursor: 'pointer',
                              textAlign: 'left',
                              textDecoration: 'underline',
                            }}
                          >
                            {cand.right.name}
                          </button>
                        ) : (
                          <span style={{ fontWeight: 600, fontSize: '13px', color: 'var(--color-ink)' }}>
                            {cand.right.name}
                          </span>
                        )}
                        <Badge variant="muted">{t(cand.right.lifecycle)}</Badge>
                      </div>
                      <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                        <Badge variant="mesh">{t(cand.right.kind)}</Badge>
                        <span
                          style={{
                            fontSize: '10px',
                            fontFamily: 'var(--font-mono)',
                            color: 'var(--color-muted)',
                          }}
                        >
                          {cand.right.id}
                        </span>
                      </div>
                      {cand.right.subtitle && (
                        <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginTop: '2px' }}>
                          {cand.right.subtitle}
                        </div>
                      )}
                      {cand.right.tags && cand.right.tags.length > 0 && (
                        <div style={{ display: 'flex', gap: '4px', flexWrap: 'wrap', marginTop: '4px' }}>
                          {cand.right.tags.map((t) => (
                            <span key={t} style={{ fontSize: '10px', color: 'var(--color-muted)' }}>
                              #{t}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Merge Modal */}
      {selectedCandidate && (
        <MergeModal
          candidate={selectedCandidate}
          isOpen={true}
          onClose={() => setSelectedCandidate(null)}
          onMerged={handleMerged}
        />
      )}
    </div>
  );
};
