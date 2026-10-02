import React, { useEffect, useState } from 'react';
import { t } from '../../../i18n';
import { getTransport } from '../transport';
import type { AssetDetailDto, ServiceRecordDto } from '../types';
import { DetailEditorForm } from './DetailEditorForm';
import type { DetailMutation } from './useDetailMutation';
import { ServicePanel } from '../panels/ServicePanel';
import { ServiceRenewalEditor } from './ServiceRenewalEditor';

function serviceDraft(detail: AssetDetailDto, record: ServiceRecordDto) {
  return {
    name: detail.name, provider: record.provider || '', plan: record.plan || '',
    cost: record.cost_minor != null ? (record.cost_minor / 100).toFixed(2) : '',
    currency: record.currency || 'USD', cadence: record.billing_cadence || 'monthly',
    autoRenew: record.auto_renew ?? true, renewsAt: record.renews_at?.slice(0, 10) || '',
    notes: record.notes || '', projectDir: record.project_dir || '',
    startCommand: record.start_command || '', stopCommand: record.stop_command || '',
    endpointUrl: record.endpoint_url || '',
  };
}

export function ServiceDetailEditor({ detail, record, mutation }: {
  detail: AssetDetailDto; record: ServiceRecordDto; mutation: DetailMutation;
}) {
  const [mode, setMode] = useState<'view' | 'edit' | 'renewal'>('view');
  const [draft, setDraft] = useState(() => serviceDraft(detail, record));
  const [serviceRuntimeBusy, setServiceRuntimeBusy] = useState(false);
  const isLocalService = record.service_type === 'local';
  const submitting = mutation.pending;
  const transport = getTransport();
  const updateDraft = <K extends keyof typeof draft>(key: K, value: typeof draft[K]) =>
    setDraft((previous) => ({ ...previous, [key]: value }));

  useEffect(() => {
    if (!isLocalService) return;
    let cancelled = false;
    transport.serviceRuntimeStatus(detail.id).then((status) => {
      if (!cancelled) setServiceRuntimeBusy(['starting', 'running', 'stopping'].includes(status.state));
    }).catch(() => { if (!cancelled) setServiceRuntimeBusy(false); });
    return () => { cancelled = true; };
  }, [detail, isLocalService, transport]);

  const startEditing = () => {
    setDraft(serviceDraft(detail, record));
    mutation.clear();
    setMode('edit');
  };
  const handleCancelEditService = () => { setMode('view'); mutation.clear(); };
  const handleSaveService = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!draft.name.trim()) {
      mutation.fail({ category: 'invalid_input', message: 'service name must not be empty' });
      return;
    }
    const launchPatch = (value: string, current?: string | null) =>
      value.trim() === (current ?? '') ? undefined : (value.trim() || null);
    try {
      await mutation.run(() => transport.serviceCommand({
        action: 'update', asset_id: detail.id, expected_revision: detail.revision,
        name: draft.name.trim(), notes: draft.notes.trim() || null,
        provider: isLocalService ? undefined : (draft.provider.trim() || null),
        plan: isLocalService ? undefined : (draft.plan.trim() || null),
        cost: isLocalService ? undefined : (draft.cost.trim() || null),
        currency: isLocalService ? undefined : (draft.cost.trim() ? draft.currency.trim().toUpperCase() : null),
        billing_cadence: isLocalService ? undefined : (draft.cadence || undefined),
        renews_at: isLocalService ? undefined : (draft.renewsAt.trim() || null),
        auto_renew: isLocalService ? undefined : draft.autoRenew,
        // Dashboard URL is not an editable field in this form.
        project_dir: isLocalService ? launchPatch(draft.projectDir, record.project_dir) : undefined,
        start_command: isLocalService ? launchPatch(draft.startCommand, record.start_command) : undefined,
        stop_command: isLocalService ? launchPatch(draft.stopCommand, record.stop_command) : undefined,
        endpoint_url: isLocalService ? launchPatch(draft.endpointUrl, record.endpoint_url) : undefined,
      }), (receipt) => receipt.changed ? 'Service updated' : 'No changes were made');
      setMode('view');
    } catch { /* Keep the failed draft. */ }
  };
  if (mode === 'renewal') return <ServiceRenewalEditor detail={detail} record={record}
    mutation={mutation} onClose={() => setMode('view')} />;
  if (mode === 'edit') return (
    <DetailEditorForm title={t(isLocalService ? 'Edit Configuration' : 'Edit Service Subscription')} revision={detail.revision} testId="service-edit-form"
      pending={submitting} onSubmit={handleSaveService} onCancel={handleCancelEditService}
      cancelTestId="cancel-service-button" saveTestId="save-service-button" saveLabel="Save Changes" pendingLabel="Saving...">
      <div>
        <label htmlFor="service-edit-name">{t('Name *')}</label>
        <input
          id="service-edit-name"
          data-testid="service-name-input"
          type="text"
          value={draft.name}
          onChange={(e) => updateDraft('name', e.target.value)}
          disabled={submitting}
        />
      </div>

      {!isLocalService && <>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
          <div>
            <label htmlFor="service-edit-provider">{t('Provider')}</label>
            <input
              id="service-edit-provider"
              data-testid="service-provider-input"
              type="text"
              value={draft.provider}
              onChange={(e) => updateDraft('provider', e.target.value)}
              disabled={submitting}
            />
          </div>

          <div>
            <label htmlFor="service-edit-plan">{t('Plan / Tier')}</label>
            <input
              id="service-edit-plan"
              data-testid="service-plan-input"
              type="text"
              value={draft.plan}
              onChange={(e) => updateDraft('plan', e.target.value)}
              disabled={submitting}
            />
          </div>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '10px' }}>
          <div>
            <label
              htmlFor="service-edit-cost"
              className="detail-editor-hint"
            >{t('Cost (Decimal)')}</label>
            <input
              id="service-edit-cost"
              data-testid="service-cost-input"
              type="text"
              value={draft.cost}
              onChange={(e) => updateDraft('cost', e.target.value)}
              disabled={submitting}
              placeholder="19.99"
              className="detail-editor-field-small"
            />
          </div>

          <div>
            <label
              htmlFor="service-edit-currency"
              className="detail-editor-hint"
            >{t('Currency')}</label>
            <input
              id="service-edit-currency"
              data-testid="service-currency-input"
              type="text"
              value={draft.currency}
              onChange={(e) => updateDraft('currency', e.target.value)}
              disabled={submitting}
              placeholder={t('USD')}
              className="detail-editor-field-small"
            />
          </div>

          <div>
            <label
              htmlFor="service-edit-cadence"
              className="detail-editor-hint"
            >{t('Cadence')}</label>
            <select
              id="service-edit-cadence"
              data-testid="service-cadence-select"
              value={draft.cadence}
              onChange={(e) => updateDraft('cadence', e.target.value)}
              disabled={submitting}
              className="detail-editor-field-small"
            >
              <option value="monthly">{t('Monthly')}</option>
              <option value="yearly">{t('Yearly')}</option>
              <option value="quarterly">{t('Quarterly')}</option>
              <option value="usage_based">{t('Usage-based')}</option>
              <option value="one_time">{t('One-time')}</option>
              <option value="other">{t('Other')}</option>
            </select>
          </div>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
          <div>
            <label
              htmlFor="service-edit-renews"
              className="detail-editor-hint"
            >{t('Next Renewal Date')}</label>
            <input
              id="service-edit-renews"
              data-testid="service-renews-input"
              type="date"
              value={draft.renewsAt}
              onChange={(e) => updateDraft('renewsAt', e.target.value)}
              disabled={submitting}
              className="detail-editor-field-small"
            />
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', paddingTop: '16px' }}>
            <input
              id="service-edit-autorenew"
              data-testid="service-autorenew-checkbox"
              type="checkbox"
              checked={draft.autoRenew}
              onChange={(e) => updateDraft('autoRenew', e.target.checked)}
              disabled={submitting}
            />
            <label
              htmlFor="service-edit-autorenew"
              style={{ fontSize: '12px', color: 'var(--color-ink)', cursor: 'pointer' }}
            >{t('Auto-renewing')}</label>
          </div>
        </div>

      </>}

      {isLocalService && (
        <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: '10px' }}>
          <span
            style={{
              fontSize: '11px',
              textTransform: 'uppercase',
              color: 'var(--color-muted)',
              fontWeight: 600,
              display: 'block',
              marginBottom: '8px',
            }}
          >{t('Launch Configuration')}</span>
          {serviceRuntimeBusy && (
            <div
              data-testid="service-runtime-edit-warning"
              style={{
                marginBottom: 8,
                padding: '8px 10px',
                fontSize: 11.5,
                color: 'var(--color-attention)',
                backgroundColor: 'var(--color-attention-bg)',
                border: '1px solid var(--color-attention)',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              {t('The service is running; stop it before editing its launch configuration.')}
            </div>
          )}
          <div style={{ marginBottom: '10px' }}>
            <label htmlFor="service-edit-dir">{t('Project Directory')}</label>
            <div style={{ display: 'flex', gap: 8 }}>
              <input
                id="service-edit-dir"
                data-testid="service-project-dir-input"
                type="text"
                value={draft.projectDir}
                onChange={(e) => updateDraft('projectDir', e.target.value)}
                disabled={submitting || serviceRuntimeBusy}
              />
              <button
                type="button"
                onClick={async () => {
                  try {
                    const dir = await transport.pickDirectory(t('Choose the project directory'));
                    if (dir) updateDraft('projectDir', dir);
                  } catch {
                    // The typed value stays editable.
                  }
                }}
                disabled={submitting || serviceRuntimeBusy}
                style={{
                  padding: '6px 12px',
                  backgroundColor: 'var(--color-canvas)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '12px',
                  cursor: submitting || serviceRuntimeBusy ? 'wait' : 'pointer',
                  whiteSpace: 'nowrap',
                }}
              >{t('Choose…')}</button>
            </div>
          </div>
          <div>
            <label htmlFor="service-edit-command">{t('Start Command')}</label>
            <input
              id="service-edit-command"
              data-testid="service-start-command-input"
              type="text"
              value={draft.startCommand}
              onChange={(e) => updateDraft('startCommand', e.target.value)}
              disabled={submitting || serviceRuntimeBusy}
              className="detail-editor-command"
            />
          </div>
        </div>
      )}

      {isLocalService && <div>
        <label htmlFor="service-edit-stop-command">{t('Stop Command (optional)')}</label>
        <input id="service-edit-stop-command" data-testid="service-stop-command-input" type="text"
          value={draft.stopCommand} onChange={(event) => updateDraft('stopCommand', event.target.value)}
          disabled={submitting || serviceRuntimeBusy} placeholder={t('e.g. bash stop-local.sh')}
          className="detail-editor-command" />
      </div>}

      {isLocalService && <div>
        <label htmlFor="service-edit-endpoint">{t('Access Address (optional)')}</label>
        <input id="service-edit-endpoint" data-testid="service-endpoint-input" type="text"
          value={draft.endpointUrl} onChange={e => updateDraft('endpointUrl', e.target.value)}
          disabled={submitting} placeholder="http://127.0.0.1:7861"
        />
      </div>}

      <div>
        <label htmlFor="service-edit-notes">{t('Notes')}</label>
        <textarea
          id="service-edit-notes"
          data-testid="service-notes-input"
          rows={2}
          value={draft.notes}
          onChange={(e) => updateDraft('notes', e.target.value)}
          disabled={submitting}
          placeholder={t('Notes about billing, plans, or license')}
        />
      </div>
    </DetailEditorForm>
  );
  const editable = detail.lifecycle === 'active' && !submitting;
  return <ServicePanel record={record} onEdit={editable ? startEditing : undefined}
    onRecordRenewal={editable && !isLocalService ? () => { mutation.clear(); setMode('renewal'); } : undefined} />;
}
