import { useEffect, useRef, useState } from 'react';
import { t } from '../../i18n';
import { getTransport, normalizeDesktopError } from './transport';
import type { AssetDetailDto, AssetSummary, DesktopError, MediaCommand, MutationReceiptDto } from './types';
import type { MutationFeedbackState } from './MutationFeedback';

type MediaAsset = Pick<AssetSummary, 'id' | 'name' | 'revision' | 'details'>;
interface UndoAction { asset: MediaAsset; previousStatus: string }

/** One revision-safe mutation/read-back/undo flow for every media view. */
export function useMediaStatusActions(onUpdated: (receipt: MutationReceiptDto, detail?: AssetDetailDto) => Promise<void> | void, scope?: string) {
  const currentScope = useRef(scope);
  currentScope.current = scope;
  const refresh = useRef(onUpdated);
  refresh.current = onUpdated;
  const busy = useRef(false);
  const [pending, setPending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<DesktopError | null>(null);
  const [undo, setUndo] = useState<UndoAction | null>(null);
  useEffect(() => { setNotice(null); setError(null); setUndo(null); }, [scope]);
  useEffect(() => {
    if (!notice || pending) return;
    const timer = setTimeout(() => { setUndo(null); setNotice(null); }, 10_000);
    return () => clearTimeout(timer);
  }, [notice, pending]);

  const mutate = async (
    asset: MediaAsset,
    command: MediaCommand,
    success: (receipt: MutationReceiptDto, fresh: AssetDetailDto) => { notice: string; undo?: UndoAction },
  ): Promise<MutationReceiptDto> => {
    if (busy.current) throw { category: 'unavailable', message: 'Saving...' };
    busy.current = true;
    const actionScope = currentScope.current;
    setPending(true);
    setError(null);
    setNotice(null);
    setUndo(null);
    try {
      const receipt = await getTransport().mediaCommand(command);
      const fresh = await getTransport().getAsset(asset.id);
      await refresh.current(receipt, fresh);
      if (actionScope !== currentScope.current) return receipt;
      const feedback = success(receipt, fresh);
      setNotice(feedback.notice);
      setUndo(feedback.undo ?? null);
      return receipt;
    } catch (err: unknown) {
      const failure = normalizeDesktopError(err);
      if (failure.category === 'stale_revision' || failure.category === 'conflict') {
        try {
          await refresh.current({ operation: 'refresh', asset_ids: [asset.id], revision: null, changed: false, warnings: [] });
          failure.message = t('This media item changed. The list has been refreshed; try again.');
        } catch (refreshError) { failure.message = normalizeDesktopError(refreshError).message; }
      }
      if (actionScope === currentScope.current) setError(failure);
      throw err;
    } finally { busy.current = false; setPending(false); }
  };
  const transition = (asset: MediaAsset, nextStatus: string, undoing = false) => {
    const previousStatus = asset.details?.module === 'media' ? asset.details.status : null;
    return mutate(asset, {
      action: 'transition_status', asset_id: asset.id, status: nextStatus, expected_revision: asset.revision ?? 1,
    }, (receipt, fresh) => ({
      notice: undoing ? t('Change undone') : nextStatus === 'completed'
        ? t('Completed: {name}', { name: asset.name }) : nextStatus === 'planned'
        ? t('Moved to planned: {name}', { name: asset.name }) : t('Status updated to {status}', { status: t(nextStatus) }),
      undo: !undoing && receipt.changed && receipt.revision === fresh.revision && previousStatus && previousStatus !== nextStatus
        ? { asset: { ...asset, revision: fresh.revision, details: fresh.details }, previousStatus } : undefined,
    }));
  };
  const updateProgress = async (asset: MediaAsset, progress: Pick<Extract<MediaCommand, { action: 'update_progress' }>, 'current' | 'total' | 'unit'>): Promise<MutationReceiptDto> => {
    return mutate(asset, {
      action: 'update_progress', asset_id: asset.id, expected_revision: asset.revision ?? 1, ...progress,
    }, () => ({ notice: t('Progress updated') }));
  };
  const feedback: MutationFeedbackState = {
    pending, notice, error,
    onUndo: undo ? () => { void transition(undo.asset, undo.previousStatus, true).catch(() => {}); } : undefined,
    onDismiss: () => { setNotice(null); setUndo(null); setError(null); },
  };
  return { transition, updateProgress, feedback };
}
