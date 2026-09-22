import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type { DesktopError } from './types';

interface CreateServiceModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated: (newAssetId: string) => void;
}

export const CreateServiceModal: React.FC<CreateServiceModalProps> = ({
  isOpen,
  onClose,
  onCreated,
}) => {
  const [name, setName] = useState('');
  const [serviceType, setServiceType] = useState('saas');
  const [provider, setProvider] = useState('');
  const [accountLabel, setAccountLabel] = useState('');
  const [plan, setPlan] = useState('');
  const [cost, setCost] = useState('');
  const [currency, setCurrency] = useState('USD');
  const [billingCadence, setBillingCadence] = useState('monthly');
  const [renewsAt, setRenewsAt] = useState('');
  const [autoRenew, setAutoRenew] = useState(true);
  const [dashboardUrl, setDashboardUrl] = useState('');
  const [domainName, setDomainName] = useState('');
  const [notes, setNotes] = useState('');
  const [tagsInput, setTagsInput] = useState('');

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !submitting) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose, submitting]);

  if (!isOpen) return null;

  const transport = getTransport();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (submitting) return;

    if (!name.trim()) {
      setError({
        category: 'invalid_input',
        message: 'Service name must not be empty',
      });
      return;
    }

    if ((cost.trim() && !currency.trim()) || (!cost.trim() && currency.trim() && cost.trim() !== '')) {
      setError({
        category: 'invalid_input',
        message: 'Cost and currency must be set together',
      });
      return;
    }

    setSubmitting(true);
    setError(null);

    const tags = tagsInput
      .split(',')
      .map((t) => t.trim().toLowerCase())
      .filter((t) => t.length > 0);

    try {
      const receipt = await transport.serviceCommand({
        action: 'create',
        name: name.trim(),
        service_type: serviceType,
        provider: provider.trim() || undefined,
        account_label: accountLabel.trim() || undefined,
        plan: plan.trim() || undefined,
        cost: cost.trim() || undefined,
        currency: cost.trim() ? currency.trim().toUpperCase() : undefined,
        billing_cadence: billingCadence || undefined,
        renews_at: renewsAt.trim() || undefined,
        auto_renew: autoRenew,
        dashboard_url: dashboardUrl.trim() || undefined,
        domain_name: domainName.trim() || undefined,
        notes: notes.trim() || undefined,
        tags,
      });

      const newId = receipt.asset_ids[0];
      onCreated(newId);
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
      aria-label="Add Service Asset"
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.45)',
        backdropFilter: 'blur(2px)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 110,
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
          maxWidth: '560px',
          maxHeight: '88vh',
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
            <Badge variant="mesh">Service</Badge>
            <h3 style={{ margin: 0, fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)' }}>
              Add Service Asset
            </h3>
          </div>
          <button
            onClick={onClose}
            disabled={submitting}
            aria-label="Close add service modal"
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
          data-testid="create-service-form"
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
              data-testid="create-service-error"
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
              htmlFor="create-service-name"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Service Name *
            </label>
            <input
              id="create-service-name"
              data-testid="service-create-name-input"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={submitting}
              placeholder="e.g. GitHub Copilot, AWS, Cloudflare"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-service-type"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Service Type
              </label>
              <select
                id="create-service-type"
                data-testid="service-create-type-select"
                value={serviceType}
                onChange={(e) => setServiceType(e.target.value)}
                disabled={submitting}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface)',
                  boxSizing: 'border-box',
                }}
              >
                <option value="saas">SaaS</option>
                <option value="api">API</option>
                <option value="vps">VPS</option>
                <option value="domain">Domain</option>
                <option value="local">Local</option>
              </select>
            </div>

            <div>
              <label
                htmlFor="create-service-provider"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Provider
              </label>
              <input
                id="create-service-provider"
                data-testid="service-create-provider-input"
                type="text"
                value={provider}
                onChange={(e) => setProvider(e.target.value)}
                disabled={submitting}
                placeholder="e.g. OpenAI, GitHub, AWS"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-service-plan"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Plan / Tier
              </label>
              <input
                id="create-service-plan"
                data-testid="service-create-plan-input"
                type="text"
                value={plan}
                onChange={(e) => setPlan(e.target.value)}
                disabled={submitting}
                placeholder="e.g. Pro, Business, Pay-as-you-go"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="create-service-account"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Account Label
              </label>
              <input
                id="create-service-account"
                data-testid="service-create-account-input"
                type="text"
                value={accountLabel}
                onChange={(e) => setAccountLabel(e.target.value)}
                disabled={submitting}
                placeholder="e.g. personal, work"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '10px' }}>
            <span style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-muted)', display: 'block', marginBottom: '8px' }}>
              Subscription & Renewal
            </span>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '12px' }}>
              <div>
                <label
                  htmlFor="create-service-cost"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Cost (Decimal string)
                </label>
                <input
                  id="create-service-cost"
                  data-testid="service-create-cost-input"
                  type="text"
                  value={cost}
                  onChange={(e) => setCost(e.target.value)}
                  disabled={submitting}
                  placeholder="e.g. 19.99"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="create-service-currency"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Currency
                </label>
                <input
                  id="create-service-currency"
                  data-testid="service-create-currency-input"
                  type="text"
                  value={currency}
                  onChange={(e) => setCurrency(e.target.value)}
                  disabled={submitting}
                  placeholder="USD, EUR"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="create-service-cadence"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Cadence
                </label>
                <select
                  id="create-service-cadence"
                  data-testid="service-create-cadence-select"
                  value={billingCadence}
                  onChange={(e) => setBillingCadence(e.target.value)}
                  disabled={submitting}
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    backgroundColor: 'var(--color-surface)',
                    boxSizing: 'border-box',
                  }}
                >
                  <option value="monthly">Monthly</option>
                  <option value="yearly">Yearly</option>
                  <option value="quarterly">Quarterly</option>
                  <option value="usage_based">Usage-based</option>
                  <option value="one_time">One-time</option>
                  <option value="other">Other</option>
                </select>
              </div>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px', marginTop: '10px' }}>
              <div>
                <label
                  htmlFor="create-service-renews"
                  style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                >
                  Next Renewal Date
                </label>
                <input
                  id="create-service-renews"
                  data-testid="service-create-renews-input"
                  type="date"
                  value={renewsAt}
                  onChange={(e) => setRenewsAt(e.target.value)}
                  disabled={submitting}
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '12px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                  }}
                />
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: '8px', paddingTop: '16px' }}>
                <input
                  id="create-service-autorenew"
                  data-testid="service-create-autorenew-checkbox"
                  type="checkbox"
                  checked={autoRenew}
                  onChange={(e) => setAutoRenew(e.target.checked)}
                  disabled={submitting}
                />
                <label
                  htmlFor="create-service-autorenew"
                  style={{ fontSize: '12px', color: 'var(--color-ink)', cursor: 'pointer' }}
                >
                  Auto-renews automatically
                </label>
              </div>
            </div>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-service-url"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Dashboard URL
              </label>
              <input
                id="create-service-url"
                data-testid="service-create-url-input"
                type="text"
                value={dashboardUrl}
                onChange={(e) => setDashboardUrl(e.target.value)}
                disabled={submitting}
                placeholder="https://dashboard.example.com"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="create-service-domain"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Domain Name
              </label>
              <input
                id="create-service-domain"
                data-testid="service-create-domain-input"
                type="text"
                value={domainName}
                onChange={(e) => setDomainName(e.target.value)}
                disabled={submitting}
                placeholder="example.com"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div>
            <label
              htmlFor="create-service-tags"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Tags (comma separated)
            </label>
            <input
              id="create-service-tags"
              data-testid="service-create-tags-input"
              type="text"
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
              disabled={submitting}
              placeholder="saas, cloud, infra"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="create-service-notes"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Notes
            </label>
            <textarea
              id="create-service-notes"
              data-testid="service-create-notes-input"
              rows={3}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              disabled={submitting}
              placeholder="Usage notes, renewal notes"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
                fontFamily: 'inherit',
              }}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '12px' }}>
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
              data-testid="submit-create-service-button"
              disabled={submitting}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-mesh)',
                color: '#fff',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                fontWeight: 500,
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >
              {submitting ? 'Creating...' : 'Create Service Asset'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
