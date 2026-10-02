import React from 'react';
import { t } from '../../../i18n';
import './DetailEditor.css';

interface DetailEditorFormProps {
  title: string;
  revision: number;
  testId: string;
  pending: boolean;
  onSubmit: (event: React.FormEvent) => void;
  onCancel: () => void;
  cancelTestId: string;
  saveTestId: string;
  saveLabel: string;
  pendingLabel: string;
  children: React.ReactNode;
}

/** Shared form chrome; fields and drafts remain owned by each module. */
export function DetailEditorForm({ title, revision, testId, pending, onSubmit, onCancel,
  cancelTestId, saveTestId, saveLabel, pendingLabel, children }: DetailEditorFormProps) {
  return <form className="detail-editor" data-testid={testId} onSubmit={onSubmit}>
    <div className="detail-editor-heading">
      <span>{title}</span>
      <details className="quiet-details"><summary>{t('Technical details')}</summary>
        <span>{t('rev ')}{revision}</span>
      </details>
    </div>
    {children}
    <div className="detail-editor-actions">
      <button type="button" data-testid={cancelTestId} disabled={pending} onClick={onCancel}>{t('Cancel')}</button>
      <button type="submit" data-testid={saveTestId} disabled={pending}>{pending ? t(pendingLabel) : t(saveLabel)}</button>
    </div>
  </form>;
}
