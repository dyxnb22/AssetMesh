import React from 'react';
import { Badge } from '../../ui/Badge';
import type { AssetSummary } from './types';
import { t } from '../../i18n';

interface AssetInspectorProps {
  asset: AssetSummary | null;
  onClose?: () => void;
  onSelectTag?: (tag: string) => void;
  onOpenDetail?: (assetId: string) => void;
}

export const AssetInspector: React.FC<AssetInspectorProps> = ({
  asset,
  onClose,
  onSelectTag,
  onOpenDetail,
}) => {
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
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <Badge variant="mesh">{t(asset.kind)}</Badge>
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
          </div>

          <div
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '14px',
              fontSize: '12px',
            }}
          >
            <div style={{ color: 'var(--color-muted)', marginBottom: '4px' }}>{t('Asset ID')}</div>
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
              {asset.id}
            </code>

            <div style={{ color: 'var(--color-muted)', marginTop: '10px', marginBottom: '4px' }}>{t('Lifecycle Status')}</div>
            <div style={{ fontWeight: 500 }}>{t(asset.lifecycle)}</div>

            <div style={{ color: 'var(--color-muted)', marginTop: '10px', marginBottom: '4px' }}>{t('Last Updated')}</div>
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: '11px' }}>
              {asset.updated_at}
            </div>
          </div>

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
