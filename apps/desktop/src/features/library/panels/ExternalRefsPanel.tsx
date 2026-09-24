import React, { useState } from 'react';
import type { ExternalRefDto } from '../types';
import { t } from '../../../i18n';

interface ExternalRefsPanelProps {
  refs: ExternalRefDto[];
}

export const ExternalRefsPanel: React.FC<ExternalRefsPanelProps> = ({ refs }) => {
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const handleCopy = (r: ExternalRefDto) => {
    const text = `${r.namespace}:${r.external_id}`;
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      navigator.clipboard.writeText(text);
    }
    setCopiedId(text);
    setTimeout(() => {
      setCopiedId(null);
    }, 2000);
  };

  if (refs.length === 0) {
    return null;
  }

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '8px',
        backgroundColor: 'var(--color-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        padding: '14px',
      }}
      data-testid="external-refs-panel"
    >
      <div style={{ fontSize: '11px', textTransform: 'uppercase', color: 'var(--color-muted)', fontWeight: 600 }}>{t('External References (')}{refs.length})
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
        {refs.map((r, index) => {
          const key = `${r.namespace}:${r.external_id}:${index}`;
          const isCopied = copiedId === `${r.namespace}:${r.external_id}`;

          return (
            <div
              key={key}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '6px 10px',
                backgroundColor: 'var(--color-canvas)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '12px',
                gap: '8px',
              }}
            >
              <div style={{ minWidth: 0, flex: 1 }}>
                <span style={{ color: 'var(--color-muted)', fontWeight: 500 }}>
                  {r.namespace}:
                </span>{' '}
                <code style={{ fontSize: '11px', wordBreak: 'break-all' }}>{r.external_id}</code>
                {r.source_url && (
                  <div style={{ marginTop: '2px' }}>
                    <a
                      href={r.source_url}
                      target="_blank"
                      rel="noreferrer"
                      style={{
                        fontSize: '11px',
                        color: 'var(--color-mesh)',
                        textDecoration: 'none',
                      }}
                    >
                      {r.source_url} ↗
                    </a>
                  </div>
                )}
              </div>

              <button
                onClick={() => handleCopy(r)}
                title={t('Copy namespace:external_id to clipboard')}
                aria-label={`Copy reference ${r.namespace}:${r.external_id}`}
                style={{
                  padding: '3px 8px',
                  fontSize: '11px',
                  backgroundColor: isCopied ? 'var(--color-mesh)' : 'var(--color-surface)',
                  color: isCopied ? '#FFFFFF' : 'var(--color-ink)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  cursor: 'pointer',
                  flexShrink: 0,
                  transition: 'background-color 0.15s ease',
                }}
              >
                {isCopied ? t('Copied!') : t('Copy')}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
};
