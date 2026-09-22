import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { ExternalRefsPanel } from './panels/ExternalRefsPanel';
import { MediaPanel } from './panels/MediaPanel';
import { MergedRedirectPanel } from './panels/MergedRedirectPanel';
import { ServicePanel } from './panels/ServicePanel';
import { SoftwarePanel } from './panels/SoftwarePanel';
import { UnknownPanel } from './panels/UnknownPanel';
import { getTransport } from './transport';
import type { AssetDetailDto, DesktopError, UnknownDetailsDto } from './types';

interface AssetDetailViewProps {
  assetId: string;
  onClose?: () => void;
  onFollowRedirect?: (survivingAssetId: string) => void;
  onSelectTag?: (tag: string) => void;
}

export const AssetDetailView: React.FC<AssetDetailViewProps> = ({
  assetId,
  onClose,
  onFollowRedirect,
  onSelectTag,
}) => {
  const [detail, setDetail] = useState<AssetDetailDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<DesktopError | null>(null);

  const transport = getTransport();

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
          setError({
            category: 'DetailQueryFailed',
            message: err instanceof Error ? err.message : String(err),
          });
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

  // Exhaustive typed module detail renderer
  const renderModuleDetails = () => {
    switch (detail.details.module) {
      case 'media':
        return <MediaPanel record={detail.details} />;
      case 'software':
        return <SoftwarePanel record={detail.details} />;
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
