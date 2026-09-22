import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { ExternalRefsPanel } from './panels/ExternalRefsPanel';
import { MediaPanel } from './panels/MediaPanel';
import { MergedRedirectPanel } from './panels/MergedRedirectPanel';
import { ServicePanel } from './panels/ServicePanel';
import { SoftwarePanel } from './panels/SoftwarePanel';
import { UnknownPanel } from './panels/UnknownPanel';
import { RelationExplorer } from './RelationExplorer';
import { ActivityFeed } from './ActivityFeed';
import { getTransport, normalizeDesktopError } from './transport';
import type {
  AppCapabilities,
  AssetDetailDto,
  DesktopError,
  MediaRecordDto,
  MutationReceiptDto,
  ServiceRecordDto,
  SoftwareRecordDto,
  UnknownDetailsDto,
} from './types';

interface AssetDetailViewProps {
  assetId: string;
  onClose?: () => void;
  onFollowRedirect?: (survivingAssetId: string) => void;
  onSelectTag?: (tag: string) => void;
  onAssetUpdated?: (receipt: MutationReceiptDto) => void;
  capabilities?: AppCapabilities | null;
  onOpenAssetDetail?: (assetId: string) => void;
}

export const AssetDetailView: React.FC<AssetDetailViewProps> = ({
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

  // Software Mutation State
  const [isEditingSoftware, setIsEditingSoftware] = useState(false);
  const [draftPurpose, setDraftPurpose] = useState('');
  const [draftNotes, setDraftNotes] = useState('');

  // Media Mutation State
  const [isEditingMedia, setIsEditingMedia] = useState(false);
  const [draftMediaTitle, setDraftMediaTitle] = useState('');
  const [draftMediaSummary, setDraftMediaSummary] = useState('');
  const [draftMediaYear, setDraftMediaYear] = useState<number | ''>('');
  const [draftMediaPlatform, setDraftMediaPlatform] = useState('');
  const [draftMediaNotes, setDraftMediaNotes] = useState('');

  // Service Mutation State
  const [isEditingService, setIsEditingService] = useState(false);
  const [isRecordingRenewal, setIsRecordingRenewal] = useState(false);
  const [draftServiceName, setDraftServiceName] = useState('');
  const [draftServiceProvider, setDraftServiceProvider] = useState('');
  const [draftServicePlan, setDraftServicePlan] = useState('');
  const [draftServiceCost, setDraftServiceCost] = useState('');
  const [draftServiceCurrency, setDraftServiceCurrency] = useState('');
  const [draftServiceCadence, setDraftServiceCadence] = useState('');
  const [draftServiceAutoRenew, setDraftServiceAutoRenew] = useState(true);
  const [draftServiceRenewsAt, setDraftServiceRenewsAt] = useState('');
  const [draftServiceDashboardUrl, setDraftServiceDashboardUrl] = useState('');
  const [draftServiceNotes, setDraftServiceNotes] = useState('');

  // Renewal State
  const [renewalDate, setRenewalDate] = useState('');
  const [renewalNextDate, setRenewalNextDate] = useState('');
  const [renewalCost, setRenewalCost] = useState('');
  const [renewalCurrency, setRenewalCurrency] = useState('USD');

  // Archive State
  const [confirmingArchive, setConfirmingArchive] = useState(false);

  const [submitting, setSubmitting] = useState(false);
  const [mutationError, setMutationError] = useState<DesktopError | null>(null);
  const [isConflict, setIsConflict] = useState(false);
  const [receiptNotice, setReceiptNotice] = useState<string | null>(null);
  const [showActivity, setShowActivity] = useState(false);

  const transport = getTransport();

  useEffect(() => {
    let cancelled = false;

    const fetchDetail = async () => {
      setLoading(true);
      setError(null);
      setIsEditingSoftware(false);
      setIsEditingMedia(false);
      setIsEditingService(false);
      setIsRecordingRenewal(false);
      setConfirmingArchive(false);
      setMutationError(null);
      setIsConflict(false);
      setReceiptNotice(null);

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

  const isSoftware = detail?.details.module === 'software';
  const canEditSoftware = isSoftware && detail?.lifecycle === 'active';

  const handleStartEditSoftware = () => {
    if (!detail || !isSoftware) return;
    const sw = detail.details as SoftwareRecordDto;
    setDraftPurpose(sw.purpose || '');
    setDraftNotes(sw.notes || '');
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);
    setIsEditingSoftware(true);
  };

  const handleCancelEditSoftware = () => {
    setIsEditingSoftware(false);
    setMutationError(null);
    setIsConflict(false);
  };

  const handleSaveSoftware = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!detail || !isSoftware || submitting) return;

    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.softwareCommand({
        action: 'update_metadata',
        asset_id: detail.id,
        expected_revision: detail.revision,
        purpose: draftPurpose.trim() || undefined,
        notes: draftNotes.trim() || undefined,
      });

      // Mandatory Read-Back: UI never assumes success without reading back canonical state
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsEditingSoftware(false);
      setReceiptNotice(
        receipt.changed
          ? `Saved successfully (rev ${receipt.revision})`
          : 'No changes detected'
      );
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      const norm = normalizeDesktopError(err);
      if (norm.category === 'stale_revision') {
        setIsConflict(true);
      }
      setMutationError(norm);
    } finally {
      setSubmitting(false);
    }
  };

  const handleReloadLatest = async () => {
    if (!detail) return;
    try {
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsConflict(false);
      setMutationError(null);
      setIsEditingSoftware(false);
      setIsEditingMedia(false);
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    }
  };

  const isMedia = detail?.details.module === 'media';
  const canEditMedia = isMedia && detail?.lifecycle === 'active';

  const handleStartEditMedia = () => {
    if (!detail || !isMedia) return;
    const med = detail.details as MediaRecordDto;
    setDraftMediaTitle(detail.name || '');
    setDraftMediaSummary(detail.summary || '');
    setDraftMediaYear(med.year ?? '');
    setDraftMediaPlatform(med.platform || '');
    setDraftMediaNotes(med.notes || '');
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);
    setIsEditingMedia(true);
  };

  const handleCancelEditMedia = () => {
    setIsEditingMedia(false);
    setMutationError(null);
    setIsConflict(false);
  };

  const handleSaveMedia = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!detail || !isMedia || submitting) return;

    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.mediaCommand({
        action: 'update_metadata',
        asset_id: detail.id,
        expected_revision: detail.revision,
        title: draftMediaTitle.trim() || undefined,
        summary: draftMediaSummary.trim() || undefined,
        year: draftMediaYear !== '' ? Number(draftMediaYear) : undefined,
        platform: draftMediaPlatform.trim() || undefined,
        notes: draftMediaNotes.trim() || undefined,
      });

      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsEditingMedia(false);
      setReceiptNotice(
        receipt.changed
          ? `Saved successfully (rev ${receipt.revision})`
          : 'No changes detected'
      );
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      const norm = normalizeDesktopError(err);
      if (norm.category === 'stale_revision') {
        setIsConflict(true);
      }
      setMutationError(norm);
    } finally {
      setSubmitting(false);
    }
  };

  const handleMediaTransitionStatus = async (status: string) => {
    if (!detail || submitting) return;
    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.mediaCommand({
        action: 'transition_status',
        asset_id: detail.id,
        status,
        expected_revision: detail.revision,
      });
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setReceiptNotice(`Status updated to ${status}`);
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      setMutationError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  const handleMediaUpdateProgress = async (prog: {
    unit?: string;
    current?: number;
    total?: number;
  }) => {
    if (!detail || submitting) return;
    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.mediaCommand({
        action: 'update_progress',
        asset_id: detail.id,
        ...prog,
        expected_revision: detail.revision,
      });
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setReceiptNotice('Progress updated');
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      setMutationError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  const handleMediaRate = async (rating: number) => {
    if (!detail || submitting) return;
    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.mediaCommand({
        action: 'rate',
        asset_id: detail.id,
        rating,
        expected_revision: detail.revision,
      });
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setReceiptNotice(`Rating updated to ${rating}`);
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      setMutationError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  const handleArchiveAsset = async () => {
    if (!detail || submitting) return;
    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      let receipt;
      if (detail.kind.startsWith('software')) {
        receipt = await transport.softwareCommand({
          action: 'archive',
          asset_id: detail.id,
          expected_revision: detail.revision,
        });
      } else if (detail.kind.startsWith('service')) {
        receipt = await transport.serviceCommand({
          action: 'archive',
          asset_id: detail.id,
          expected_revision: detail.revision,
        });
      } else {
        receipt = await transport.mediaCommand({
          action: 'archive',
          asset_id: detail.id,
          expected_revision: detail.revision,
        });
      }
      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setConfirmingArchive(false);
      setReceiptNotice('Asset archived');
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      setMutationError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  const isService = detail?.details.module === 'services';
  const canEditService = isService && detail?.lifecycle === 'active';

  const handleStartEditService = () => {
    if (!detail || !isService) return;
    const s = detail.details as ServiceRecordDto;
    setDraftServiceName(detail.name || '');
    setDraftServiceProvider(s.provider || '');
    setDraftServicePlan(s.plan || '');
    setDraftServiceCost(s.cost_minor != null ? (s.cost_minor / 100).toFixed(2) : '');
    setDraftServiceCurrency(s.currency || 'USD');
    setDraftServiceCadence(s.billing_cadence || 'monthly');
    setDraftServiceAutoRenew(s.auto_renew ?? true);
    setDraftServiceRenewsAt(s.renews_at ? s.renews_at.slice(0, 10) : '');
    setDraftServiceDashboardUrl(s.dashboard_url || '');
    setDraftServiceNotes(s.notes || '');
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);
    setIsEditingService(true);
    setIsRecordingRenewal(false);
  };

  const handleCancelEditService = () => {
    setIsEditingService(false);
    setMutationError(null);
    setIsConflict(false);
  };

  const handleSaveService = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!detail || !isService || submitting) return;

    if (!draftServiceName.trim()) {
      setMutationError({
        category: 'invalid_input',
        message: 'service name must not be empty',
      });
      return;
    }

    setSubmitting(true);
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);

    try {
      const receipt = await transport.serviceCommand({
        action: 'update',
        asset_id: detail.id,
        expected_revision: detail.revision,
        name: draftServiceName.trim() || undefined,
        provider: draftServiceProvider.trim() || undefined,
        plan: draftServicePlan.trim() || undefined,
        cost: draftServiceCost.trim() || undefined,
        currency: draftServiceCost.trim() ? draftServiceCurrency.trim().toUpperCase() : undefined,
        billing_cadence: draftServiceCadence || undefined,
        renews_at: draftServiceRenewsAt.trim() || undefined,
        auto_renew: draftServiceAutoRenew,
        dashboard_url: draftServiceDashboardUrl.trim() || undefined,
        notes: draftServiceNotes.trim() || undefined,
      });

      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsEditingService(false);
      setReceiptNotice(receipt.changed ? 'Service updated' : 'No changes were made');
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      const norm = normalizeDesktopError(err);
      setMutationError(norm);
      if (norm.category === 'stale_revision') {
        setIsConflict(true);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleStartRecordRenewal = () => {
    if (!detail || !isService) return;
    const s = detail.details as ServiceRecordDto;
    setRenewalDate(s.renews_at ? s.renews_at.slice(0, 10) : new Date().toISOString().slice(0, 10));
    setRenewalNextDate('');
    setRenewalCost(s.cost_minor != null ? (s.cost_minor / 100).toFixed(2) : '');
    setRenewalCurrency(s.currency || 'USD');
    setMutationError(null);
    setIsConflict(false);
    setReceiptNotice(null);
    setIsRecordingRenewal(true);
    setIsEditingService(false);
  };

  const handleCancelRecordRenewal = () => {
    setIsRecordingRenewal(false);
    setMutationError(null);
  };

  const handleSubmitRenewal = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!detail || !isService || submitting) return;

    if (!renewalDate.trim()) {
      setMutationError({
        category: 'invalid_input',
        message: 'renewal date must not be empty',
      });
      return;
    }

    setSubmitting(true);
    setMutationError(null);
    setReceiptNotice(null);

    try {
      const receipt = await transport.serviceCommand({
        action: 'record_renewal',
        asset_id: detail.id,
        renews_at: renewalDate.trim(),
        next_renews_at: renewalNextDate.trim() || undefined,
        cost: renewalCost.trim() || undefined,
        currency: renewalCost.trim() ? renewalCurrency.trim().toUpperCase() : undefined,
        expected_revision: detail.revision,
      });

      const freshDetail = await transport.getAsset(detail.id);
      setDetail(freshDetail);
      setIsRecordingRenewal(false);
      setReceiptNotice('Renewal recorded');
      onAssetUpdated?.(receipt);
    } catch (err: unknown) {
      setMutationError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  // Exhaustive typed module detail renderer
  const renderModuleDetails = () => {
    switch (detail.details.module) {
      case 'media':
        if (isEditingMedia) {
          return (
            <form
              onSubmit={handleSaveMedia}
              data-testid="media-edit-form"
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '14px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span
                  style={{
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    color: 'var(--color-muted)',
                    fontWeight: 600,
                  }}
                >
                  Edit Media Metadata
                </span>
                <Badge variant="mesh">rev {detail.revision}</Badge>
              </div>

              <div>
                <label
                  htmlFor="media-title"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Title
                </label>
                <input
                  id="media-title"
                  data-testid="media-title-input"
                  type="text"
                  value={draftMediaTitle}
                  onChange={(e) => setDraftMediaTitle(e.target.value)}
                  disabled={submitting}
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="media-summary"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Summary
                </label>
                <input
                  id="media-summary"
                  data-testid="media-summary-input"
                  type="text"
                  value={draftMediaSummary}
                  onChange={(e) => setDraftMediaSummary(e.target.value)}
                  disabled={submitting}
                  placeholder="Short tagline or subtitle"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
                <div>
                  <label
                    htmlFor="media-year"
                    style={{
                      display: 'block',
                      fontSize: '12px',
                      fontWeight: 500,
                      marginBottom: '4px',
                      color: 'var(--color-ink)',
                    }}
                  >
                    Release Year
                  </label>
                  <input
                    id="media-year"
                    data-testid="media-year-input"
                    type="number"
                    value={draftMediaYear}
                    onChange={(e) =>
                      setDraftMediaYear(e.target.value === '' ? '' : Number(e.target.value))
                    }
                    disabled={submitting}
                    placeholder="e.g. 2023"
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '13px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                      color: 'var(--color-ink)',
                    }}
                  />
                </div>

                <div>
                  <label
                    htmlFor="media-platform"
                    style={{
                      display: 'block',
                      fontSize: '12px',
                      fontWeight: 500,
                      marginBottom: '4px',
                      color: 'var(--color-ink)',
                    }}
                  >
                    Platform
                  </label>
                  <input
                    id="media-platform"
                    data-testid="media-platform-input"
                    type="text"
                    value={draftMediaPlatform}
                    onChange={(e) => setDraftMediaPlatform(e.target.value)}
                    disabled={submitting}
                    placeholder="e.g. Steam, Crunchyroll"
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '13px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                      color: 'var(--color-ink)',
                    }}
                  />
                </div>
              </div>

              <div>
                <label
                  htmlFor="media-notes"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Notes
                </label>
                <textarea
                  id="media-notes"
                  data-testid="media-notes-input"
                  rows={3}
                  value={draftMediaNotes}
                  onChange={(e) => setDraftMediaNotes(e.target.value)}
                  disabled={submitting}
                  placeholder="Personal reflections, notes, etc."
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontFamily: 'inherit',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '4px' }}>
                <button
                  type="button"
                  data-testid="cancel-media-button"
                  onClick={handleCancelEditMedia}
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    cursor: submitting ? 'not-allowed' : 'pointer',
                    color: 'var(--color-ink)',
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  data-testid="save-media-button"
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-mesh)',
                    color: '#FFFFFF',
                    border: 'none',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    fontWeight: 500,
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  {submitting ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </form>
          );
        }
        return (
          <MediaPanel
            record={detail.details}
            isEditable={canEditMedia}
            onEditMetadata={handleStartEditMedia}
            onTransitionStatus={handleMediaTransitionStatus}
            onUpdateProgress={handleMediaUpdateProgress}
            onRate={handleMediaRate}
          />
        );
      case 'software':
        if (isEditingSoftware) {
          return (
            <form
              onSubmit={handleSaveSoftware}
              data-testid="software-edit-form"
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '14px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span
                  style={{
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    color: 'var(--color-muted)',
                    fontWeight: 600,
                  }}
                >
                  Edit Software Metadata
                </span>
                <Badge variant="mesh">rev {detail.revision}</Badge>
              </div>

              <div>
                <label
                  htmlFor="software-purpose"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Purpose
                </label>
                <input
                  id="software-purpose"
                  data-testid="software-purpose-input"
                  type="text"
                  value={draftPurpose}
                  onChange={(e) => setDraftPurpose(e.target.value)}
                  disabled={submitting}
                  placeholder="e.g. CLI tool for git repository management"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="software-notes"
                  style={{
                    display: 'block',
                    fontSize: '12px',
                    fontWeight: 500,
                    marginBottom: '4px',
                    color: 'var(--color-ink)',
                  }}
                >
                  Notes
                </label>
                <textarea
                  id="software-notes"
                  data-testid="software-notes-input"
                  rows={3}
                  value={draftNotes}
                  onChange={(e) => setDraftNotes(e.target.value)}
                  disabled={submitting}
                  placeholder="Personal usage notes, configuration tips, etc."
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontFamily: 'inherit',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    color: 'var(--color-ink)',
                  }}
                />
              </div>

              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '4px' }}>
                <button
                  type="button"
                  data-testid="cancel-edit-button"
                  onClick={handleCancelEditSoftware}
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    cursor: submitting ? 'not-allowed' : 'pointer',
                    color: 'var(--color-ink)',
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  data-testid="save-software-button"
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-mesh)',
                    color: '#FFFFFF',
                    border: 'none',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    fontWeight: 500,
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  {submitting ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </form>
          );
        }
        return (
          <SoftwarePanel
            record={detail.details}
            onEdit={canEditSoftware ? handleStartEditSoftware : undefined}
          />
        );
      case 'services':
        if (isEditingService) {
          return (
            <form
              onSubmit={handleSaveService}
              data-testid="service-edit-form"
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '14px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span
                  style={{
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    color: 'var(--color-muted)',
                    fontWeight: 600,
                  }}
                >
                  Edit Service Subscription
                </span>
                <Badge variant="mesh">rev {detail.revision}</Badge>
              </div>

              <div>
                <label
                  htmlFor="service-edit-name"
                  style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                >
                  Name *
                </label>
                <input
                  id="service-edit-name"
                  data-testid="service-name-input"
                  type="text"
                  value={draftServiceName}
                  onChange={(e) => setDraftServiceName(e.target.value)}
                  disabled={submitting}
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                  }}
                />
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label
                    htmlFor="service-edit-provider"
                    style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                  >
                    Provider
                  </label>
                  <input
                    id="service-edit-provider"
                    data-testid="service-provider-input"
                    type="text"
                    value={draftServiceProvider}
                    onChange={(e) => setDraftServiceProvider(e.target.value)}
                    disabled={submitting}
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '13px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>

                <div>
                  <label
                    htmlFor="service-edit-plan"
                    style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                  >
                    Plan / Tier
                  </label>
                  <input
                    id="service-edit-plan"
                    data-testid="service-plan-input"
                    type="text"
                    value={draftServicePlan}
                    onChange={(e) => setDraftServicePlan(e.target.value)}
                    disabled={submitting}
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '13px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '10px' }}>
                <div>
                  <label
                    htmlFor="service-edit-cost"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Cost (Decimal)
                  </label>
                  <input
                    id="service-edit-cost"
                    data-testid="service-cost-input"
                    type="text"
                    value={draftServiceCost}
                    onChange={(e) => setDraftServiceCost(e.target.value)}
                    disabled={submitting}
                    placeholder="19.99"
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>

                <div>
                  <label
                    htmlFor="service-edit-currency"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Currency
                  </label>
                  <input
                    id="service-edit-currency"
                    data-testid="service-currency-input"
                    type="text"
                    value={draftServiceCurrency}
                    onChange={(e) => setDraftServiceCurrency(e.target.value)}
                    disabled={submitting}
                    placeholder="USD"
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>

                <div>
                  <label
                    htmlFor="service-edit-cadence"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Cadence
                  </label>
                  <select
                    id="service-edit-cadence"
                    data-testid="service-cadence-select"
                    value={draftServiceCadence}
                    onChange={(e) => setDraftServiceCadence(e.target.value)}
                    disabled={submitting}
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
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

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label
                    htmlFor="service-edit-renews"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Next Renewal Date
                  </label>
                  <input
                    id="service-edit-renews"
                    data-testid="service-renews-input"
                    type="date"
                    value={draftServiceRenewsAt}
                    onChange={(e) => setDraftServiceRenewsAt(e.target.value)}
                    disabled={submitting}
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', paddingTop: '16px' }}>
                  <input
                    id="service-edit-autorenew"
                    data-testid="service-autorenew-checkbox"
                    type="checkbox"
                    checked={draftServiceAutoRenew}
                    onChange={(e) => setDraftServiceAutoRenew(e.target.checked)}
                    disabled={submitting}
                  />
                  <label
                    htmlFor="service-edit-autorenew"
                    style={{ fontSize: '12px', color: 'var(--color-ink)', cursor: 'pointer' }}
                  >
                    Auto-renewing
                  </label>
                </div>
              </div>

              <div>
                <label
                  htmlFor="service-edit-notes"
                  style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                >
                  Notes
                </label>
                <textarea
                  id="service-edit-notes"
                  data-testid="service-notes-input"
                  rows={2}
                  value={draftServiceNotes}
                  onChange={(e) => setDraftServiceNotes(e.target.value)}
                  disabled={submitting}
                  placeholder="Notes about billing, plans, or license"
                  style={{
                    width: '100%',
                    padding: '6px 10px',
                    fontSize: '13px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    boxSizing: 'border-box',
                    backgroundColor: 'var(--color-canvas)',
                    fontFamily: 'inherit',
                  }}
                />
              </div>

              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '4px' }}>
                <button
                  type="button"
                  data-testid="cancel-service-button"
                  onClick={handleCancelEditService}
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  data-testid="save-service-button"
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-mesh)',
                    color: '#FFFFFF',
                    border: 'none',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    fontWeight: 500,
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  {submitting ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </form>
          );
        }

        if (isRecordingRenewal) {
          return (
            <form
              onSubmit={handleSubmitRenewal}
              data-testid="record-renewal-form"
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-mesh)',
                borderRadius: 'var(--radius-md)',
                padding: '14px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span
                  style={{
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    color: 'var(--color-muted)',
                    fontWeight: 600,
                  }}
                >
                  Record Explicit Renewal
                </span>
                <Badge variant="mesh">rev {detail.revision}</Badge>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label
                    htmlFor="renewal-date"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Renewal Occurred At *
                  </label>
                  <input
                    id="renewal-date"
                    data-testid="renewal-date-input"
                    type="date"
                    required
                    value={renewalDate}
                    onChange={(e) => setRenewalDate(e.target.value)}
                    disabled={submitting}
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>

                <div>
                  <label
                    htmlFor="renewal-next-date"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Next Renewal Date (Optional)
                  </label>
                  <input
                    id="renewal-next-date"
                    data-testid="renewal-next-date-input"
                    type="date"
                    value={renewalNextDate}
                    onChange={(e) => setRenewalNextDate(e.target.value)}
                    disabled={submitting}
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label
                    htmlFor="renewal-cost"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Charged Cost (Optional)
                  </label>
                  <input
                    id="renewal-cost"
                    data-testid="renewal-cost-input"
                    type="text"
                    value={renewalCost}
                    onChange={(e) => setRenewalCost(e.target.value)}
                    disabled={submitting}
                    placeholder="19.99"
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>

                <div>
                  <label
                    htmlFor="renewal-currency"
                    style={{ display: 'block', fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}
                  >
                    Currency
                  </label>
                  <input
                    id="renewal-currency"
                    data-testid="renewal-currency-input"
                    type="text"
                    value={renewalCurrency}
                    onChange={(e) => setRenewalCurrency(e.target.value)}
                    disabled={submitting}
                    placeholder="USD"
                    style={{
                      width: '100%',
                      padding: '6px 10px',
                      fontSize: '12px',
                      border: '1px solid var(--color-border)',
                      borderRadius: 'var(--radius-sm)',
                      boxSizing: 'border-box',
                      backgroundColor: 'var(--color-canvas)',
                    }}
                  />
                </div>
              </div>

              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '4px' }}>
                <button
                  type="button"
                  data-testid="cancel-renewal-button"
                  onClick={handleCancelRecordRenewal}
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  data-testid="submit-renewal-button"
                  disabled={submitting}
                  style={{
                    padding: '6px 14px',
                    backgroundColor: 'var(--color-mesh)',
                    color: '#FFFFFF',
                    border: 'none',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    fontWeight: 500,
                    cursor: submitting ? 'not-allowed' : 'pointer',
                  }}
                >
                  {submitting ? 'Recording...' : 'Record Renewal'}
                </button>
              </div>
            </form>
          );
        }

        return (
          <ServicePanel
            record={detail.details}
            onEdit={canEditService ? handleStartEditService : undefined}
            onRecordRenewal={canEditService ? handleStartRecordRenewal : undefined}
          />
        );
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
              >
                Archive
              </button>
            )}
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
            <strong>Confirm Archive:</strong> Are you sure you want to archive <em>{detail.name}</em>? Archived assets become read-only.
          </div>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              type="button"
              data-testid="confirm-archive-button"
              disabled={submitting}
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
              {submitting ? 'Archiving...' : 'Yes, Archive'}
            </button>
            <button
              type="button"
              data-testid="cancel-archive-button"
              disabled={submitting}
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
            >
              Cancel
            </button>
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
          <strong>Archived Asset:</strong> This asset is preserved in read-only state.
          {detail.archived_at && ` Archived on ${detail.archived_at.slice(0, 10)}.`}
        </div>
      )}

      {/* Receipt Notice */}
      {receiptNotice && (
        <div
          data-testid="mutation-receipt-badge"
          style={{
            padding: '8px 12px',
            backgroundColor: 'var(--color-canvas)',
            border: '1px solid var(--color-mesh)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-mesh)',
            fontSize: '12px',
            fontWeight: 500,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <span>✓ {receiptNotice}</span>
          <button
            type="button"
            onClick={() => setReceiptNotice(null)}
            aria-label="Dismiss notice"
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              color: 'var(--color-muted)',
              fontSize: '14px',
            }}
          >
            ✕
          </button>
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
            <Badge variant="danger">stale_revision</Badge>
            <strong>Edit Conflict Detected</strong>
          </div>
          <div>
            {mutationError?.message ||
              `This asset has been modified elsewhere (expected rev ${detail.revision}). Your edits have been preserved, but were not saved.`}
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
            >
              Discard Draft & Reload Latest
            </button>
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
            <Badge variant="danger">{mutationError.category}</Badge>
            <strong>
              {mutationError.category === 'invalid_input'
                ? 'Validation Error'
                : 'Mutation Failed'}
            </strong>
          </div>
          <div>{mutationError.message}</div>
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

      {/* Relations & Impact Explorer */}
      {detail.lifecycle !== 'merged' && (
        <RelationExplorer
          rootAsset={detail}
          capabilities={capabilities}
          onOpenAssetDetail={onOpenAssetDetail}
          onAssetUpdated={() => {
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
          >
            Activity History
          </span>
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
            {showActivity ? 'Hide History' : 'Show History'}
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
