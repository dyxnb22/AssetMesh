import React, { useState } from 'react';
import { t } from '../../../i18n';
import { getTransport } from '../transport';
import type { AssetDetailDto, MediaRecordDto } from '../types';
import { DetailEditorForm } from './DetailEditorForm';
import type { DetailMutation } from './useDetailMutation';
import { MediaPanel } from '../panels/MediaPanel';
import type { useMediaStatusActions } from '../useMediaStatusActions';

export function MediaDetailEditor({ detail, record, mutation, mediaActions }: {
  detail: AssetDetailDto; record: MediaRecordDto; mutation: DetailMutation;
  mediaActions: ReturnType<typeof useMediaStatusActions>;
}) {
  const [editing, setEditing] = useState(false);
  const [draftMediaTitle, setDraftMediaTitle] = useState('');
  const [draftMediaSummary, setDraftMediaSummary] = useState('');
  const [draftMediaYear, setDraftMediaYear] = useState<number | ''>('');
  const [draftMediaPlatform, setDraftMediaPlatform] = useState('');
  const [draftMediaNotes, setDraftMediaNotes] = useState('');
  const submitting = mutation.pending || mediaActions.feedback.pending;
  const transport = getTransport();
  const startEditing = () => {
    setDraftMediaTitle(detail.name);
    setDraftMediaSummary(detail.summary || '');
    setDraftMediaYear(record.year ?? '');
    setDraftMediaPlatform(record.platform || '');
    setDraftMediaNotes(record.notes || '');
    mutation.clear();
    setEditing(true);
  };
  const handleCancelEditMedia = () => { setEditing(false); mutation.clear(); };
  const handleSaveMedia = async (event: React.FormEvent) => {
    event.preventDefault();
    try {
      await mutation.run(() => transport.mediaCommand({
        action: 'update_metadata', asset_id: detail.id, expected_revision: detail.revision,
        title: draftMediaTitle.trim(), summary: draftMediaSummary.trim() || null,
        year: draftMediaYear !== '' ? Number(draftMediaYear) : null,
        platform: draftMediaPlatform.trim() || null, notes: draftMediaNotes.trim() || null,
      }), (receipt) => receipt.changed ? t('Saved successfully') : 'No changes detected');
      setEditing(false);
    } catch { /* Keep the failed draft. */ }
  };
  if (editing) return (
    <DetailEditorForm title={t('Edit Media Metadata')} revision={detail.revision} testId="media-edit-form"
      pending={submitting} onSubmit={handleSaveMedia} onCancel={handleCancelEditMedia}
      cancelTestId="cancel-media-button" saveTestId="save-media-button" saveLabel="Save Changes" pendingLabel="Saving...">
      <div>
        <label htmlFor="media-title">{t('Title')}</label>
        <input
          id="media-title"
          data-testid="media-title-input"
          type="text"
          value={draftMediaTitle}
          onChange={(e) => setDraftMediaTitle(e.target.value)}
          disabled={submitting}
        />
      </div>

      <div>
        <label htmlFor="media-summary">{t('Summary')}</label>
        <input
          id="media-summary"
          data-testid="media-summary-input"
          type="text"
          value={draftMediaSummary}
          onChange={(e) => setDraftMediaSummary(e.target.value)}
          disabled={submitting}
          placeholder={t('Short tagline or subtitle')}
        />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
        <div>
          <label htmlFor="media-year">{t('Release Year')}</label>
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
          />
        </div>

        <div>
          <label htmlFor="media-platform">{t('Platform')}</label>
          <input
            id="media-platform"
            data-testid="media-platform-input"
            type="text"
            value={draftMediaPlatform}
            onChange={(e) => setDraftMediaPlatform(e.target.value)}
            disabled={submitting}
            placeholder={t('e.g. Steam, Crunchyroll')}
          />
        </div>
      </div>

      <div>
        <label htmlFor="media-notes">{t('Notes')}</label>
        <textarea
          id="media-notes"
          data-testid="media-notes-input"
          rows={3}
          value={draftMediaNotes}
          onChange={(e) => setDraftMediaNotes(e.target.value)}
          disabled={submitting}
          placeholder={t('Personal reflections, notes, etc.')}
        />
      </div>
    </DetailEditorForm>
  );
  return <MediaPanel record={record} isEditable={detail.lifecycle === 'active' && !submitting}
    onEditMetadata={startEditing}
    onTransitionStatus={async (status) => { mutation.clear(); await mediaActions.transition(detail, status); }}
    onUpdateProgress={async (progress) => {
      mediaActions.feedback.onDismiss();
      await mutation.run(() => transport.mediaCommand({
        action: 'update_progress', asset_id: detail.id, expected_revision: detail.revision, ...progress,
      }), 'Progress updated');
    }}
    onRate={async (rating) => {
      mediaActions.feedback.onDismiss();
      await mutation.run(() => transport.mediaCommand({
        action: 'rate', asset_id: detail.id, expected_revision: detail.revision, rating,
      }), t('Rating updated to {rating}', { rating }));
    }} />;
}
