import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { ExternalRefsPanel } from './panels/ExternalRefsPanel';
import { MutationFeedback } from './MutationFeedback';
import { useMediaStatusActions } from './useMediaStatusActions';
import { MergedRedirectPanel } from './panels/MergedRedirectPanel';
import { InfoPanel } from './InfoEditor';
import { UnknownPanel } from './panels/UnknownPanel';
import { RelationExplorer } from './RelationExplorer';
import { ActivityFeed } from './ActivityFeed';
import { getTransport, normalizeDesktopError } from './transport';
import { MediaDetailEditor } from './detail/MediaDetailEditor';
import { SoftwareDetailEditor } from './detail/SoftwareDetailEditor';
import { ServiceDetailEditor } from './detail/ServiceDetailEditor';
import { useDetailMutation } from './detail/useDetailMutation';
import type { AppCapabilities, AssetDetailDto, DesktopError, MutationReceiptDto, UnknownDetailsDto } from './types';
import { t } from '../../i18n';

interface AssetDetailViewProps {
  assetId: string;
  onClose?: () => void;
  onFollowRedirect?: (survivingAssetId: string) => void;
  onSelectTag?: (tag: string) => void;
  onAssetUpdated?: (receipt: MutationReceiptDto, fresh?: AssetDetailDto) => void;
  capabilities?: AppCapabilities | null;
  onOpenAssetDetail?: (assetId: string) => void;
}

export const AssetDetailView: React.FC<AssetDetailViewProps> = (props) => (
  <AssetDetailSession key={props.assetId} {...props} />
);

const AssetDetailSession: React.FC<AssetDetailViewProps> = ({
  assetId,
  onClose,
  onFollowRedirect,
  onSelectTag,
  onAssetUpdated,
  capabilities = null,
  onOpenAssetDetail,
}) => {
  const [detail, setDetail] = useState<AssetDetailDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<DesktopError | null>(null);

  const [confirmingArchive, setConfirmingArchive] = useState(false);
  const [showActivity, setShowActivity] = useState(false);
  const [editorVersion, setEditorVersion] = useState(0);
  const transport = getTransport();
  const mutation = useDetailMutation(assetId, transport, (receipt, fresh) => {
    setDetail(fresh);
    onAssetUpdated?.(receipt);
  });
  const { pending: submitting, error: mutationError, notice: receiptNotice } = mutation;
  const isConflict = mutationError?.category === 'stale_revision';

  useEffect(() => {
    let cancelled = false;

    const fetchDetail = async () => {
      setLoading(true);
      setError(null);

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

  const mediaActions = useMediaStatusActions(async (receipt, fresh) => {
    if (fresh) setDetail(fresh);
    else setDetail(await transport.getAsset(assetId));
    onAssetUpdated?.(receipt, fresh);
  }, assetId);

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
      >{t('Loading asset details...')}</div>
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
          <Badge variant="danger">{error?.category || t('Error')}</Badge>
          <span style={{ fontWeight: 600, color: 'var(--color-danger)' }}>{t('Failed to load asset details')}</span>
        </div>
        <p style={{ fontSize: '12px', color: 'var(--color-ink)' }}>
          {error?.message || t('Asset could not be found.')}
        </p>
      </div>
    );
  }

  const handleReloadLatest = async () => {
    try {
      setDetail(await transport.getAsset(assetId));
      mutation.clear();
      setConfirmingArchive(false);
      setEditorVersion((version) => version + 1);
    } catch (reason: unknown) {
      mutation.fail(normalizeDesktopError(reason));
    }
  };
  const handleArchiveAsset = async () => {
    if (submitting || mediaActions.feedback.pending) return;
    try {
      const command = { action: 'archive' as const, asset_id: detail.id, expected_revision: detail.revision };
      await mutation.run(() => {
        switch (detail.details.module) {
          case 'software': return transport.softwareCommand(command);
          case 'services': return transport.serviceCommand(command);
          case 'info': return transport.infoCommand(command);
          case 'media': return transport.mediaCommand(command);
          default: throw { category: 'invalid_input', message: 'This asset cannot be archived.' };
        }
      }, 'Asset archived');
      setConfirmingArchive(false);
    } catch { /* The mutation controller presents the error. */ }
  };
  const renderModuleDetails = () => {
    switch (detail.details.module) {
      case 'media': return <MediaDetailEditor detail={detail} record={detail.details} mutation={mutation} mediaActions={mediaActions} />;
      case 'software': return <SoftwareDetailEditor detail={detail} record={detail.details} mutation={mutation} />;
      case 'services': return <ServiceDetailEditor detail={detail} record={detail.details} mutation={mutation} />;
      case 'merged_redirect': return <MergedRedirectPanel record={detail.details} onFollowRedirect={(id) => onFollowRedirect?.(id)} />;
      case 'info': return <InfoPanel detail={detail} record={detail.details} onSaved={(receipt, fresh) => {
        setDetail(fresh); onAssetUpdated?.(receipt);
      }} />;
      default: return <UnknownPanel record={detail.details as UnknownDetailsDto} />;
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
            <Badge variant="mesh">{t(detail.kind)}</Badge>
            {detail.lifecycle !== 'active' && (
              <Badge variant={detail.lifecycle === 'archived' ? 'attention' : 'danger'}>
                {t(detail.lifecycle)}
              </Badge>
            )}
            <details className="quiet-details"><summary>{t('Technical details')}</summary>
            <span
              style={{
                fontSize: '11px',
                fontFamily: 'var(--font-mono)',
                color: 'var(--color-muted)',
                backgroundColor: 'var(--color-canvas)',
                padding: '2px 6px',
                borderRadius: 'var(--radius-sm)',
              }}
              title={t('Optimistic concurrency revision')}
            >{t('rev ')}{detail.revision}
            </span>
            </details>
            {detail.lifecycle === 'active' && !confirmingArchive && (
              <button
                type="button"
                data-testid="archive-asset-button"
                onClick={() => setConfirmingArchive(true)}
                style={{
                  background: 'none',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  padding: '2px 8px',
                  fontSize: '11px',
                  color: 'var(--color-muted)',
                  cursor: 'pointer',
                }}
              >{t('Archive')}</button>
            )}
          </div>

          {onClose && (
            <button
              onClick={onClose}
              aria-label={t('Close detail view')}
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

      {/* Archive Confirmation Banner */}
      {confirmingArchive && (
        <div
          role="alert"
          data-testid="archive-confirm-banner"
          style={{
            padding: '12px 14px',
            backgroundColor: 'var(--color-attention-bg, #fffbeb)',
            border: '1px solid var(--color-attention)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-attention)',
            fontSize: '12px',
            display: 'flex',
            flexDirection: 'column',
            gap: '8px',
          }}
        >
          <div>
            <strong>{t('Confirm Archive:')}</strong>{t(' Are you sure you want to archive ')}<em>{detail.name}</em>{t('? Archived assets become read-only.')}</div>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              type="button"
              data-testid="confirm-archive-button"
              disabled={submitting || mediaActions.feedback.pending}
              onClick={handleArchiveAsset}
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-danger)',
                color: '#fff',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                fontSize: '11px',
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >
              {submitting ? t('Archiving...') : t('Yes, Archive')}
            </button>
            <button
              type="button"
              data-testid="cancel-archive-button"
              disabled={submitting || mediaActions.feedback.pending}
              onClick={() => setConfirmingArchive(false)}
              style={{
                padding: '4px 10px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '11px',
                cursor: submitting ? 'not-allowed' : 'pointer',
                color: 'var(--color-ink)',
              }}
            >{t('Cancel')}</button>
          </div>
        </div>
      )}

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
          <strong>{t('Archived Asset:')}</strong>{t(' This asset is preserved in read-only state.')}{detail.archived_at && ` ${t('Archived on {date}.', { date: detail.archived_at.slice(0, 10) })}`}
        </div>
      )}

      <MutationFeedback {...mediaActions.feedback} />
      {submitting && <MutationFeedback pending notice={null} error={null} onDismiss={() => {}} />}

      {receiptNotice && !submitting && (
        <div data-testid="mutation-receipt-badge">
          <MutationFeedback pending={false} notice={t(receiptNotice)} error={null} onDismiss={mutation.clear} />
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
            <Badge variant="danger">{t('stale_revision')}</Badge>
            <strong>{t('Edit Conflict Detected')}</strong>
          </div>
          <div>
            {mutationError?.message
              ? t(mutationError.message)
              : t(
                  'This asset has been modified elsewhere (expected rev {rev}). Your edits have been preserved, but were not saved.',
                  { rev: detail.revision }
                )}
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
            >{t('Discard Draft & Reload Latest')}</button>
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
            <Badge variant="danger">{t(mutationError.category)}</Badge>
            <strong>
              {mutationError.category === 'invalid_input'
                ? t('Validation Error')
                : t('Mutation Failed')}
            </strong>
          </div>
          <div>{t(mutationError.message)}</div>
        </div>
      )}

      {/* Typed Module Details */}
      <React.Fragment key={`${editorVersion}:${detail.lifecycle}`}>{renderModuleDetails()}</React.Fragment>

      <details className="quiet-details"><summary>{t('Record information')}</summary>
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
        >{t('Canonical Identity')}</div>

        <div>
          <div style={{ color: 'var(--color-muted)', fontSize: '11px' }}>{t('Asset ID')}</div>
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
            <div style={{ color: 'var(--color-muted)', fontSize: '11px' }}>{t('Created')}</div>
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: '11px' }}>
              {detail.created_at.slice(0, 19).replace('T', ' ')}
            </div>
          </div>
          <div>
            <div style={{ color: 'var(--color-muted)', fontSize: '11px' }}>{t('Updated')}</div>
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: '11px' }}>
              {detail.updated_at.slice(0, 19).replace('T', ' ')}
            </div>
          </div>
        </div>
      </div>

      </details>

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
          >{t('Tags (')}{detail.tags.length})
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
            {detail.tags.map((tag) => (
              <button
                key={tag}
                onClick={() => onSelectTag?.(tag)}
                title={t('Filter by #{tag}', { tag })}
                style={{
                  all: 'unset',
                  cursor: onSelectTag ? 'pointer' : 'default',
                }}
              >
                <Badge variant="muted">#{tag}</Badge>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Relations & Impact Explorer */}
      {detail.lifecycle !== 'merged' && (
        <RelationExplorer
          rootAsset={detail}
          capabilities={capabilities}
          onOpenAssetDetail={onOpenAssetDetail}
          onAssetUpdated={async () => {
            try {
              setDetail(await transport.getAsset(detail.id));
            } catch (err: unknown) {
              mutation.fail(normalizeDesktopError(err));
            }
            onAssetUpdated?.({
              operation: 'relation.update',
              asset_ids: [detail.id],
              revision: null,
              changed: true,
              warnings: [],
            });
          }}
        />
      )}

      {/* Asset Activity History */}
      <div
        data-testid="asset-activity-section"
        style={{
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
          borderRadius: 'var(--radius-md)',
          padding: '14px',
          display: 'flex',
          flexDirection: 'column',
          gap: '10px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span
            style={{
              fontSize: '11px',
              textTransform: 'uppercase',
              color: 'var(--color-muted)',
              fontWeight: 600,
            }}
          >{t('Activity History')}</span>
          <button
            type="button"
            data-testid="toggle-asset-activity-button"
            onClick={() => setShowActivity(!showActivity)}
            style={{
              background: 'none',
              border: 'none',
              fontSize: '11px',
              color: 'var(--color-mesh)',
              cursor: 'pointer',
              textDecoration: 'underline',
            }}
          >
            {showActivity ? t('Hide History') : t('Show History')}
          </button>
        </div>
        {showActivity && (
          <div style={{ maxHeight: '350px', overflowY: 'auto' }}>
            <ActivityFeed assetId={detail.id} onOpenAssetDetail={onOpenAssetDetail} />
          </div>
        )}
      </div>

      {/* External References */}
      <ExternalRefsPanel refs={detail.external_refs} />
    </div>
  );
};
