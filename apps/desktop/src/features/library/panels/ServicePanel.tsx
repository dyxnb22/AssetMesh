import React from 'react';
import { Badge } from '../../../ui/Badge';
import type { ServiceRecordDto } from '../types';

interface ServicePanelProps {
  record: ServiceRecordDto;
  onEdit?: () => void;
  onRecordRenewal?: () => void;
}

export const ServicePanel: React.FC<ServicePanelProps> = ({
  record,
  onEdit,
  onRecordRenewal,
}) => {
  const formattedCost =
    record.cost_minor != null && record.currency
      ? `${(record.cost_minor / 100).toFixed(2)} ${record.currency}${
          record.billing_cadence ? ` / ${record.billing_cadence}` : ''
        }`
      : null;

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '12px',
        backgroundColor: 'var(--color-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        padding: '14px',
      }}
      data-testid="service-panel"
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span style={{ fontSize: '11px', textTransform: 'uppercase', color: 'var(--color-muted)', fontWeight: 600 }}>
            Service Details
          </span>
          <Badge variant="muted">{record.service_type}</Badge>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {onRecordRenewal && (
            <button
              type="button"
              data-testid="record-renewal-button"
              onClick={onRecordRenewal}
              style={{
                background: 'none',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                padding: '2px 8px',
                fontSize: '11px',
                cursor: 'pointer',
                color: 'var(--color-mesh)',
                fontWeight: 500,
              }}
            >
              ↻ Record Renewal
            </button>
          )}

          {onEdit && (
            <button
              type="button"
              data-testid="edit-service-button"
              onClick={onEdit}
              style={{
                background: 'none',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                padding: '2px 8px',
                fontSize: '11px',
                cursor: 'pointer',
                color: 'var(--color-ink)',
              }}
            >
              ✎ Edit
            </button>
          )}
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: '10px', fontSize: '12px' }}>
        {record.provider && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Provider</div>
            <div style={{ fontWeight: 500 }}>{record.provider}</div>
          </div>
        )}

        {record.plan && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Plan</div>
            <div>{record.plan}</div>
          </div>
        )}

        {record.account_label && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Account</div>
            <div>{record.account_label}</div>
          </div>
        )}

        {formattedCost && (
          <div>
            <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Cost</div>
            <div style={{ fontWeight: 600, color: 'var(--color-mesh)' }}>{formattedCost}</div>
          </div>
        )}
      </div>

      {record.domain_name && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Domain</div>
          <code style={{ fontSize: '12px' }}>{record.domain_name}</code>
        </div>
      )}

      {(record.dashboard_url || record.endpoint_url) && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          {record.dashboard_url && (
            <div style={{ marginBottom: '6px' }}>
              <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Dashboard URL</div>
              <a
                href={record.dashboard_url}
                target="_blank"
                rel="noreferrer"
                style={{ color: 'var(--color-mesh)', wordBreak: 'break-all', textDecoration: 'none' }}
              >
                {record.dashboard_url} ↗
              </a>
            </div>
          )}
          {record.endpoint_url && (
            <div>
              <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Endpoint URL</div>
              <code style={{ fontSize: '11px', wordBreak: 'break-all' }}>{record.endpoint_url}</code>
            </div>
          )}
        </div>
      )}

      {(record.renews_at || record.expires_at || record.auto_renew != null) && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '16px', flexWrap: 'wrap' }}>
            {record.renews_at && (
              <div>
                <span style={{ color: 'var(--color-muted)' }}>Renews: </span>
                <span>{record.renews_at.slice(0, 10)}</span>
              </div>
            )}
            {record.expires_at && (
              <div>
                <span style={{ color: 'var(--color-muted)' }}>Expires: </span>
                <span>{record.expires_at.slice(0, 10)}</span>
              </div>
            )}
            {record.auto_renew != null && (
              <Badge variant={record.auto_renew ? 'mesh' : 'muted'}>
                {record.auto_renew ? 'Auto-Renew Active' : 'Manual Renewal'}
              </Badge>
            )}
          </div>
        </div>
      )}

      {record.notes && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '8px', fontSize: '12px' }}>
          <div style={{ color: 'var(--color-muted)', marginBottom: '2px' }}>Notes</div>
          <p style={{ whiteSpace: 'pre-wrap', color: 'var(--color-ink)' }}>{record.notes}</p>
        </div>
      )}
    </div>
  );
};
