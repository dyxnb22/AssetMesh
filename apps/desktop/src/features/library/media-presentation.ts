import { t } from '../../i18n';

export const MEDIA_STATUSES = ['planned', 'in_progress', 'completed', 'paused', 'dropped'] as const;

export function defaultProgressUnit(mediaType: string): string {
  return mediaType === 'game' ? 'hours' : mediaType === 'movie' ? 'minutes' : 'episodes';
}

export function completionLabel(mediaType: string): string {
  return mediaType === 'game' ? t('Mark game completed') : t('Mark Completed');
}
