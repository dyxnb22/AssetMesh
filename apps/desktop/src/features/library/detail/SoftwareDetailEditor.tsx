import React, { useState } from 'react';
import { t } from '../../../i18n';
import { getTransport } from '../transport';
import type { AssetDetailDto, SoftwareRecordDto } from '../types';
import { DetailEditorForm } from './DetailEditorForm';
import type { DetailMutation } from './useDetailMutation';
import { SoftwarePanel } from '../panels/SoftwarePanel';

export function SoftwareDetailEditor({ detail, record, mutation }: {
  detail: AssetDetailDto; record: SoftwareRecordDto; mutation: DetailMutation;
}) {
  const [editing, setEditing] = useState(false);
  const [draftPurpose, setDraftPurpose] = useState('');
  const [draftNotes, setDraftNotes] = useState('');
  const submitting = mutation.pending;
  const transport = getTransport();
  const startEditing = () => {
    setDraftPurpose(record.purpose || '');
    setDraftNotes(record.notes || '');
    mutation.clear();
    setEditing(true);
  };
  const handleCancelEditSoftware = () => { setEditing(false); mutation.clear(); };
  const handleSaveSoftware = async (event: React.FormEvent) => {
    event.preventDefault();
    try {
      await mutation.run(() => transport.softwareCommand({
        action: 'update_metadata', asset_id: detail.id, expected_revision: detail.revision,
        purpose: draftPurpose.trim() || null, notes: draftNotes.trim() || null,
      }), (receipt) => receipt.changed ? t('Saved successfully') : 'No changes detected');
      setEditing(false);
    } catch { /* The mutation controller presents the error and preserves this draft. */ }
  };
  if (editing) return (
    <DetailEditorForm title={t('Edit Software Metadata')} revision={detail.revision} testId="software-edit-form"
      pending={submitting} onSubmit={handleSaveSoftware} onCancel={handleCancelEditSoftware}
      cancelTestId="cancel-edit-button" saveTestId="save-software-button" saveLabel="Save Changes" pendingLabel="Saving...">
      <div>
        <label htmlFor="software-purpose">{t('Purpose')}</label>
        <input
          id="software-purpose"
          data-testid="software-purpose-input"
          type="text"
          value={draftPurpose}
          onChange={(e) => setDraftPurpose(e.target.value)}
          disabled={submitting}
          placeholder={t('e.g. CLI tool for git repository management')}
        />
      </div>

      <div>
        <label htmlFor="software-notes">{t('Notes')}</label>
        <textarea
          id="software-notes"
          data-testid="software-notes-input"
          rows={3}
          value={draftNotes}
          onChange={(e) => setDraftNotes(e.target.value)}
          disabled={submitting}
          placeholder={t('Personal usage notes, configuration tips, etc.')}
        />
      </div>
    </DetailEditorForm>
  );
  return <SoftwarePanel record={record}
    onEdit={detail.lifecycle === 'active' && !submitting ? startEditing : undefined} />;
}
