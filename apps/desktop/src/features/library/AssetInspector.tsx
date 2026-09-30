import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { formatRelativeTime, t } from '../../i18n';
import { getTransport } from './transport';
import type { AssetSummary, MediaRecordDto, RelationViewDto } from './types';

/** Canonical media statuses, in selector order. */
const MEDIA_STATUS_KEYS = ['planned', 'in_progress', 'completed', 'paused', 'dropped'] as const;

const statusTint: Record<string, { bg: string; ink: string }> = {
  planned: { bg: 'var(--color-attention-bg)', ink: 'var(--color-attention)' },
  in_progress: { bg: 'var(--color-mesh-bg)', ink: 'var(--color-mesh)' },
  completed: { bg: 'var(--color-canvas)', ink: 'var(--color-muted)' },
  paused: { bg: 'var(--color-attention-bg)', ink: 'var(--color-attention)' },
  dropped: { bg: 'var(--color-danger-bg)', ink: 'var(--color-danger)' },
};

interface AssetInspectorProps {
  asset: AssetSummary | null;
  onClose?: () => void;
  onSelectTag?: (tag: string) => void;
  onOpenDetail?: (assetId: string) => void;
  onOpenRelations?: () => void;
  /** Applies a media watch-status transition through the application layer. */
  onMediaStatusChange?: (asset: AssetSummary, status: string) => Promise<void> | void;
}

export const AssetInspector: React.FC<AssetInspectorProps> = ({
  asset,
  onClose,
  onSelectTag,
  onOpenDetail,
  onOpenRelations,
  onMediaStatusChange,
}) => {
  const [relations, setRelations] = useState<RelationViewDto[] | null>(null);
  const [copied, setCopied] = useState(false);
  const [pendingStatus, setPendingStatus] = useState<string | null>(null);

  const media: MediaRecordDto | null =
    asset?.details && asset.details.module === 'media' ? asset.details : null;

  useEffect(() => {
    let alive = true;
    setRelations(null);
    if (!asset) return undefined;
    getTransport()
      .relationList(asset.id)
      .then((rows) => {
        if (alive) setRelations(rows);
      })
      .catch(() => {
        if (alive) setRelations([]);
      });
    return () => {
      alive = false;
    };
  }, [asset]);

  const copyId = async () => {
    if (!asset) return;
    try {
      await navigator.clipboard.writeText(asset.id);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard unavailable (permissions); the raw id stays visible.
    }
  };

  const changeStatus = async (status: string) => {
    if (!asset || !onMediaStatusChange || pendingStatus || asset.lifecycle !== 'active') return;
    setPendingStatus(status);
    try {
      await onMediaStatusChange(asset, status);
    } finally {
      setPendingStatus(null);
    }
  };

  const shortId = asset ? `${asset.id.slice(0, 8)}…${asset.id.slice(-4)}` : '';
  const progress =
    media?.progress && (media.progress.current != null || media.progress.total != null)
      ? media.progress
      : null;
  const progressPct =
    progress && progress.current != null && progress.total
      ? Math.min(100, Math.round((progress.current / progress.total) * 100))
      : null;

  return (
    <aside
      aria-label={t('Asset Inspector')}
      className="inspector-panel"
      style={{
        width: '360px',
        borderLeft: '1px solid var(--color-border)',
        backgroundColor: 'var(--color-canvas)',
        display: 'flex',
        flexDirection: 'column',
        overflowY: 'auto',
        padding: '16px',
        flexShrink: 0,
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: '12px',
        }}
      >
        <span
          style={{
            fontSize: '11px',
            textTransform: 'uppercase',
            letterSpacing: '0.05em',
            color: 'var(--color-muted)',
            fontWeight: 600,
          }}
        >{t('Inspector')}</span>
        {onClose && (
          <button
            onClick={onClose}
            aria-label={t('Close Inspector')}
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              color: 'var(--color-muted)',
              fontSize: '16px',
              padding: '2px 6px',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            ✕
          </button>
        )}
      </div>

      {asset ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexWrap: 'wrap' }}>
              <Badge variant="mesh">{t(asset.kind)}</Badge>
              {media && statusTint[media.status] && (
                <span
                  style={{
                    fontSize: '10px',
                    fontWeight: 600,
                    color: statusTint[media.status].ink,
                    backgroundColor: statusTint[media.status].bg,
                    padding: '1px 8px',
                    borderRadius: 999,
                  }}
                >
                  {t(media.status)}
                </span>
              )}
              {asset.lifecycle !== 'active' && (
                <Badge variant={asset.lifecycle === 'archived' ? 'attention' : 'danger'}>
                  {t(asset.lifecycle)}
                </Badge>
              )}
            </div>
            <h3
              style={{
                fontSize: '17px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                marginTop: '8px',
                lineHeight: 1.3,
                wordBreak: 'break-word',
              }}
            >
              {asset.name}
            </h3>
            {asset.subtitle && (
              <p style={{ color: 'var(--color-muted)', fontSize: '13px', marginTop: '4px' }}>
                {asset.subtitle}
              </p>
            )}
            {media?.platform && (
              <p style={{ color: 'var(--color-muted)', fontSize: '12px', marginTop: '2px' }}>
                {media.platform}
              </p>
            )}
          </div>

          {/* Media watch panel: status transition, progress, rating */}
          {media && (
            <div
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-md)',
                padding: '12px 14px',
                display: 'flex',
                flexDirection: 'column',
                gap: '10px',
              }}
            >
              <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-ink)' }}>
                {t('Media Status')}
              </div>
              <div
                role="group"
                aria-label={t('Media Status')}
                style={{
                  display: 'flex',
                  backgroundColor: 'var(--color-canvas)',
                  borderRadius: 999,
                  padding: 3,
                  gap: 2,
                }}
              >
                {MEDIA_STATUS_KEYS.map((key) => {
                  const active = media.status === key;
                  return (
                    <button
                      key={key}
                      type="button"
                      onClick={() => changeStatus(key)}
                      disabled={pendingStatus !== null || asset.lifecycle !== 'active'}
                      aria-pressed={active}
                      title={t(key)}
                      style={{
                        flex: 1,
                        height: 26,
                        border: 'none',
                        borderRadius: 999,
                        cursor: pendingStatus !== null ? 'wait' : 'pointer',
                        fontSize: 11.5,
                        whiteSpace: 'nowrap',
                        padding: 0,
                        backgroundColor: active ? 'var(--color-mesh)' : 'transparent',
                        color: active ? '#ffffff' : 'var(--color-muted)',
                        fontWeight: active ? 600 : 400,
                      }}
                    >
                      {t(key)}
                    </button>
                  );
                })}
              </div>

              {progress && (
                <div>
                  <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between' }}>
                    <span style={{ fontSize: '11.5px', color: 'var(--color-muted)' }}>{t('Progress')}</span>
                    <span style={{ fontSize: '12.5px', fontWeight: 600, color: 'var(--color-ink)' }}>
                      {progress.unit ? `${progress.unit} ` : ''}
                      {progress.current ?? '·'}/{progress.total ?? '·'}
                    </span>
                  </div>
                  {progressPct !== null && (
                    <div
                      style={{
                        marginTop: 5,
                        height: 4,
                        borderRadius: 999,
                        backgroundColor: 'var(--color-border-subtle)',
                        overflow: 'hidden',
                      }}
                      role="progressbar"
                      aria-valuenow={progressPct}
                      aria-valuemin={0}
                      aria-valuemax={100}
                    >
                      <div
                        style={{
                          width: `${progressPct}%`,
                          height: '100%',
                          backgroundColor: 'var(--color-mesh)',
                          borderRadius: 999,
                        }}
                      />
                    </div>
                  )}
                </div>
              )}

              {media.rating != null && (
                <div>
                  <div style={{ fontSize: '11.5px', color: 'var(--color-muted)', marginBottom: 2 }}>
                    {t('My Rating')}
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span
                      aria-label={t('Rating')}
                      style={{
                        position: 'relative',
                        fontSize: 15,
                        letterSpacing: 2,
                        color: 'var(--color-border)',
                        lineHeight: 1,
                      }}
                    >
                      ★★★★★
                      <span
                        aria-hidden="true"
                        style={{
                          position: 'absolute',
                          inset: 0,
                          width: `${Math.max(0, Math.min(100, media.rating * 10))}%`,
                          overflow: 'hidden',
                          color: '#E8A33D',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        ★★★★★
                      </span>
                    </span>
                    <span style={{ fontSize: '12.5px', fontWeight: 600, color: 'var(--color-ink)' }}>
                      {media.rating.toFixed(1)}
                    </span>
                    <span style={{ fontSize: '11px', color: 'var(--color-muted)' }}>/ 10</span>
                  </div>
                </div>
              )}

              {media.status !== 'completed' && asset.lifecycle === 'active' && (
                <button
                  type="button"
                  onClick={() => changeStatus('completed')}
                  disabled={pendingStatus !== null}
                  style={{
                    height: 30,
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    backgroundColor: 'var(--color-surface)',
                    color: 'var(--color-ink)',
                    fontSize: 12,
                    fontWeight: 500,
                    cursor: pendingStatus !== null ? 'wait' : 'pointer',
                  }}
                >
                  {t('Mark Completed')}
                </button>
              )}
            </div>
          )}

          {/* Relations preview */}
          {relations !== null && (
            <div>
              <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-ink)', marginBottom: '6px' }}>
                {t('Relations')} · {relations.length}
              </div>
              {relations.length === 0 ? (
                <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>{t('No relations')}</div>
              ) : (
                <div
                  style={{
                    backgroundColor: 'var(--color-surface)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-md)',
                    padding: '4px 12px',
                  }}
                >
                  {relations.slice(0, 3).map((rel) => (
                    <div
                      key={rel.relation_id}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 8,
                        padding: '5px 0',
                        borderBottom: '1px solid var(--color-border-subtle)',
                      }}
                    >
                      <span
                        style={{
                          fontFamily: 'var(--font-mono)',
                          fontSize: 10,
                          color: 'var(--color-muted)',
                          backgroundColor: 'var(--color-canvas)',
                          borderRadius: 4,
                          padding: '0 5px',
                          lineHeight: '16px',
                        }}
                      >
                        {rel.relation_type}
                      </span>
                      <span
                        style={{
                          fontSize: 12,
                          fontWeight: 500,
                          color: 'var(--color-ink)',
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {rel.other_asset_name}
                      </span>
                    </div>
                  ))}
                </div>
              )}
              {relations.length > 3 && onOpenRelations && (
                <button
                  type="button"
                  onClick={onOpenRelations}
                  style={{
                    marginTop: 6,
                    border: 'none',
                    background: 'none',
                    padding: 0,
                    cursor: 'pointer',
                    fontSize: '11.5px',
                    fontWeight: 600,
                    color: 'var(--color-mesh)',
                  }}
                >
                  {t('Open in relations')}
                </button>
              )}
            </div>
          )}

          {asset.tags.length > 0 && (
            <div>
              <div
                style={{
                  fontSize: '12px',
                  fontWeight: 600,
                  color: 'var(--color-ink)',
                  marginBottom: '6px',
                }}
              >{t('Tags')}</div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px' }}>
                {asset.tags.map((tag) => (
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

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              fontSize: 11,
              color: 'var(--color-muted)',
              flexWrap: 'wrap',
            }}
          >
            <span
              title={asset.id}
              style={{ fontFamily: 'var(--font-mono)', fontSize: 10.5, wordBreak: 'break-all' }}
            >
              {shortId}
            </span>
            <button
              type="button"
              onClick={copyId}
              aria-label={t('Copy ID')}
              style={{
                border: 'none',
                background: 'none',
                padding: 0,
                cursor: 'pointer',
                fontSize: 10.5,
                fontWeight: 600,
                color: copied ? 'var(--color-mesh)' : 'var(--color-muted)',
              }}
            >
              {copied ? t('Copied') : t('Copy ID')}
            </button>
            <span>·</span>
            <span>
              {t('Last Updated')} {formatRelativeTime(asset.updated_at)}
            </span>
          </div>

          {onOpenDetail && (
            <button
              onClick={() => onOpenDetail(asset.id)}
              aria-label={t('View Full Details')}
              style={{
                marginTop: '4px',
                padding: '8px 12px',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                cursor: 'pointer',
                fontWeight: 500,
                fontSize: '12px',
                textAlign: 'center',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '6px',
              }}
            >{t('View Full Details →')}</button>
          )}
        </div>
      ) : (
        <div style={{ color: 'var(--color-muted)', textAlign: 'center', marginTop: '48px' }}>{t('Select an asset to view details.')}</div>
      )}
    </aside>
  );
};
