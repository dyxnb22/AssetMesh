import React from 'react';
import { t } from '../../i18n';
import type { DesktopError } from './types';

export interface MutationFeedbackState {
  pending: boolean;
  notice: string | null;
  error: DesktopError | null;
  onUndo?: () => void;
  onDismiss: () => void;
}

export const MutationFeedback: React.FC<MutationFeedbackState> = ({ pending, notice, error, onUndo, onDismiss }) => {
  if (!pending && !notice && !error) return null;
  return <div className={`mutation-feedback${error ? ' mutation-feedback-error' : ''}`} role={error ? 'alert' : 'status'}>
    <span>{pending ? t('Saving...') : error ? t(error.message) : notice}</span>
    <div>
      {onUndo && !error && <button disabled={pending} onClick={onUndo}>{t('Undo')}</button>}
      {!pending && <button onClick={onDismiss} aria-label={t('Dismiss notice')}>{t('Dismiss')}</button>}
    </div>
  </div>;
};
