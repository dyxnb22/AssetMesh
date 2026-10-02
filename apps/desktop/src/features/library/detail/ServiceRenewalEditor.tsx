import React, { useState } from 'react';
import { t } from '../../../i18n';
import { getTransport } from '../transport';
import type { AssetDetailDto, ServiceRecordDto } from '../types';
import { DetailEditorForm } from './DetailEditorForm';
import type { DetailMutation } from './useDetailMutation';

export function ServiceRenewalEditor({ detail, record, mutation, onClose }: {
  detail: AssetDetailDto; record: ServiceRecordDto; mutation: DetailMutation; onClose: () => void;
}) {
  const [renewalDate, setRenewalDate] = useState(record.renews_at ? record.renews_at.slice(0, 10) : new Date().toISOString().slice(0, 10));
  const [renewalNextDate, setRenewalNextDate] = useState('');
  const [renewalCost, setRenewalCost] = useState(record.cost_minor != null ? (record.cost_minor / 100).toFixed(2) : '');
  const [renewalCurrency, setRenewalCurrency] = useState(record.currency || 'USD');
  const submitting = mutation.pending;
  const transport = getTransport();
  const handleCancelRecordRenewal = () => { mutation.clear(); onClose(); };
  const handleSubmitRenewal = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!renewalDate.trim()) {
      mutation.fail({ category: 'invalid_input', message: 'renewal date must not be empty' });
      return;
    }
    try {
      await mutation.run(() => transport.serviceCommand({
        action: 'record_renewal', asset_id: detail.id, expected_revision: detail.revision,
        renews_at: renewalDate.trim(), next_renews_at: renewalNextDate.trim() || undefined,
        cost: renewalCost.trim() || undefined,
        currency: renewalCost.trim() ? renewalCurrency.trim().toUpperCase() : undefined,
      }), 'Renewal recorded');
      onClose();
    } catch { /* Keep the failed draft. */ }
  };
  return (
    <DetailEditorForm title={t('Record Explicit Renewal')} revision={detail.revision} testId="record-renewal-form"
      pending={submitting} onSubmit={handleSubmitRenewal} onCancel={handleCancelRecordRenewal}
      cancelTestId="cancel-renewal-button" saveTestId="submit-renewal-button" saveLabel="Record Renewal" pendingLabel="Recording...">
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
        <div>
          <label
            htmlFor="renewal-date"
            className="detail-editor-hint"
          >{t('Renewal Occurred At *')}</label>
          <input
            id="renewal-date"
            data-testid="renewal-date-input"
            type="date"
            required
            value={renewalDate}
            onChange={(e) => setRenewalDate(e.target.value)}
            disabled={submitting}
            className="detail-editor-field-small"
          />
        </div>

        <div>
          <label
            htmlFor="renewal-next-date"
            className="detail-editor-hint"
          >{t('Next Renewal Date (Optional)')}</label>
          <input
            id="renewal-next-date"
            data-testid="renewal-next-date-input"
            type="date"
            value={renewalNextDate}
            onChange={(e) => setRenewalNextDate(e.target.value)}
            disabled={submitting}
            className="detail-editor-field-small"
          />
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
        <div>
          <label
            htmlFor="renewal-cost"
            className="detail-editor-hint"
          >{t('Charged Cost (Optional)')}</label>
          <input
            id="renewal-cost"
            data-testid="renewal-cost-input"
            type="text"
            value={renewalCost}
            onChange={(e) => setRenewalCost(e.target.value)}
            disabled={submitting}
            placeholder="19.99"
            className="detail-editor-field-small"
          />
        </div>

        <div>
          <label
            htmlFor="renewal-currency"
            className="detail-editor-hint"
          >{t('Currency')}</label>
          <input
            id="renewal-currency"
            data-testid="renewal-currency-input"
            type="text"
            value={renewalCurrency}
            onChange={(e) => setRenewalCurrency(e.target.value)}
            disabled={submitting}
            placeholder={t('USD')}
            className="detail-editor-field-small"
          />
        </div>
      </div>
    </DetailEditorForm>
  );
}
