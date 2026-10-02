import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { formatRelativeTime, t } from '../../i18n';
import { Icon } from '../../ui/Icon';
import { AssetArtwork } from './AssetArtwork';
import { MutationFeedback, type MutationFeedbackState } from './MutationFeedback';
import { useViewScroll } from './useViewScroll';
import type { AssetSummary, DesktopError, MutationReceiptDto, Page, SortOption } from './types';
import './TodoList.css';

interface TodoListProps {
  viewKey: string;
  feedback: MutationFeedbackState;
  data: Page<AssetSummary> | null;
  loading: boolean;
  error: DesktopError | null;
  page: number;
  pageSize: number;
  selectedAssetId: string | null;
  kinds: string[];
  selectedKind: string | null;
  sort: SortOption;
  filtered: boolean;
  onSelectKind: (kind: string | null) => void;
  onSelectSort: (sort: SortOption) => void;
  onResetFilters: () => void;
  onSelectAsset: (id: string) => void;
  onSelectPage: (page: number) => void;
  onOpenMedia: () => void;
  onRetry: () => void;
  onTransition: (asset: AssetSummary, status: string) => Promise<MutationReceiptDto>;
}

export const TodoList: React.FC<TodoListProps> = ({ viewKey, feedback, data, loading, error, page, pageSize, selectedAssetId, kinds, selectedKind, sort, filtered, onSelectKind, onSelectSort, onResetFilters, onSelectAsset, onSelectPage, onOpenMedia, onRetry, onTransition }) => {
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const { ref: contentRef, onScroll } = useViewScroll(viewKey, !!data && !loading);
  const openMediaRef = useRef<HTMLButtonElement>(null);
  const nextFocus = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    const dismissMenus = (event: PointerEvent) => {
      for (const menu of contentRef.current?.querySelectorAll<HTMLDetailsElement>('.todo-menu[open]') ?? []) {
        if (event.target instanceof Node && !menu.contains(event.target)) menu.open = false;
      }
    };
    document.addEventListener('pointerdown', dismissMenus);
    return () => document.removeEventListener('pointerdown', dismissMenus);
  }, [contentRef]);
  useLayoutEffect(() => {
    if (pending || loading || nextFocus.current === undefined) return;
    const nextId = nextFocus.current;
    nextFocus.current = undefined;
    if (document.activeElement !== document.body && !contentRef.current?.contains(document.activeElement)) return;
    const action = Array.from(contentRef.current?.querySelectorAll<HTMLButtonElement>('button[data-complete-id]') ?? []).find((button) => button.dataset.completeId === nextId);
    (action ?? openMediaRef.current)?.focus({ preventScroll: true });
  }, [data, pending, loading, contentRef]);

  const transition = async (asset: AssetSummary, status: 'completed' | 'planned') => {
    if (pendingRef.current) return;
    pendingRef.current = true; setPending(true);
    const ownsFocus = contentRef.current?.contains(document.activeElement);
    const index = data?.items.findIndex((item) => item.id === asset.id) ?? -1;
    const following = data?.items[index + 1] ?? data?.items[index - 1];
    try { await onTransition(asset, status); if (ownsFocus) nextFocus.current = following?.id ?? null; }
    catch { /* The shared feedback retains the failure; the row stays available. */ }
    finally { pendingRef.current = false; setPending(false); }
  };
  const navigateRow = (event: React.KeyboardEvent, index: number) => {
    if (!data || !['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? data.items.length - 1 : Math.max(0, Math.min(data.items.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)));
    onSelectAsset(data.items[next].id);
    contentRef.current?.querySelectorAll<HTMLButtonElement>('.todo-title')[next]?.focus();
  };
  const total = data?.total;
  const totalPages = total == null ? null : Math.max(1, Math.ceil(total / pageSize));
  const disabled = loading || pending || feedback.pending || !!error || !data;
  const hasNextPage = totalPages === null ? data?.items.length === pageSize : page < totalPages;

  return (
    <main className="collection-workspace todo-workspace" data-testid="todos-workspace" aria-busy={loading || pending}>
      <header className="todo-header">
        <div><div className="todo-heading"><h2>{t('In Progress')}</h2><span className="todo-count">{total == null ? t('Loading...') : t('{count} media items in progress', { count: total })}</span></div><p>{t('Finish what you are watching or playing, one item at a time.')}</p></div>
        <button className="text-button" ref={openMediaRef} onClick={onOpenMedia}>{t('Open Media Library')} <Icon name="right" size={14} /></button>
      </header>
      <div className="todo-filters">
        <select aria-label={t('Filter by kind')} value={selectedKind ?? ''} onChange={(event) => onSelectKind(event.target.value || null)}><option value="">{t('All Types')}</option>{kinds.map((kind) => <option key={kind} value={kind}>{t(kind)}</option>)}</select>
        <select aria-label={t('Sort assets')} value={sort} onChange={(event) => onSelectSort(event.target.value as SortOption)}><option value="updated_desc">{t('Recently Updated')}</option><option value="updated_asc">{t('Oldest Updated')}</option><option value="name_asc">{t('Name (A-Z)')}</option><option value="name_desc">{t('Name (Z-A)')}</option><option value="kind_asc">{t('Asset Kind')}</option></select>
        {filtered && <button className="text-button" onClick={onResetFilters}>{t('Reset Filters')}</button>}
        {loading && data && <span className="muted" role="status">{t('Refreshing...')}</span>}
      </div>
      <MutationFeedback {...feedback} />
      <div className="todo-columns" aria-hidden="true"><span>{t('Work')}</span><span>{t('Progress')}</span><span /></div>
      <div className="todo-content" ref={contentRef} onScroll={onScroll}>
        {error ? <div className="todo-empty" role="alert"><Icon name="media" size={32} /><h3>{t('Query Execution Failed')}</h3><p>{t(error.message)}</p><button className="native-button" onClick={onRetry} disabled={loading}>{t('Retry')}</button></div>
          : !data ? <div className="todo-empty" role="status">{t('Loading...')}</div>
          : data.items.length === 0 ? <div className="todo-empty"><Icon name={filtered ? 'search' : 'check'} size={34} /><h3>{filtered ? t('No matching media') : t('No media in progress')}</h3><p>{filtered ? t('Try another keyword or reset filters.') : t('Set a media item to In Progress to add it here automatically.')}</p><button className="native-button" onClick={filtered ? onResetFilters : onOpenMedia}>{filtered ? t('Reset Filters') : t('Open Media Library')}</button></div>
          : <ul className="todo-list" aria-label={t('Media to-dos')}>{data.items.map((asset, index) => {
            const media = asset.details?.module === 'media' ? asset.details : null;
            const progress = media?.progress;
            const percentage = progress?.current != null && progress.total != null && progress.total > 0 ? Math.max(0, Math.min(100, Math.round(progress.current / progress.total * 100))) : null;
            const readOnly = disabled || asset.lifecycle !== 'active' || media?.status !== 'in_progress';
            return <li key={asset.id} className={`todo-row${selectedAssetId === asset.id ? ' todo-row-selected' : ''}`}>
              <button className="todo-title" onClick={() => onSelectAsset(asset.id)} onKeyDown={(event) => navigateRow(event, index)} aria-pressed={selectedAssetId === asset.id}><AssetArtwork asset={asset} /><span className="todo-title-text"><strong>{asset.name}</strong><span className="todo-meta"><span>{t(asset.kind)}</span>{media?.year && <span>{media.year}</span>}<span>{formatRelativeTime(asset.updated_at)}</span></span></span></button>
              <div className="todo-progress">{progress && (progress.current != null || progress.total != null) ? <><span>{progress.current ?? '—'}{progress.total != null ? ` / ${progress.total}` : ''} {progress.unit ? t(progress.unit) : ''}</span>{percentage !== null && <div className="todo-progress-track" role="progressbar" aria-label={t('Progress for {name}', { name: asset.name })} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percentage}><span style={{ width: `${percentage}%` }} /></div>}</> : <span className="todo-progress-missing">{t('Not recorded')}</span>}</div>
              <div className="todo-row-actions">
                <button className="icon-button todo-complete" data-complete-id={asset.id} aria-label={t('Complete {name}', { name: asset.name })} title={t('Complete {name}', { name: asset.name })} disabled={readOnly} onClick={() => void transition(asset, 'completed')}><Icon name="check" size={17} /></button>
                <details className="todo-menu" onBlur={(event) => { if (event.relatedTarget instanceof Node && !event.currentTarget.contains(event.relatedTarget)) event.currentTarget.open = false; }} onKeyDown={(event) => { if (event.key === 'Escape') { event.currentTarget.open = false; event.currentTarget.querySelector('summary')?.focus(); } }}>
                  <summary className="icon-button" aria-label={t('More actions for {name}', { name: asset.name })}><Icon name="more" size={18} /></summary>
                  <div className="todo-menu-popover"><button aria-label={t('Move {name} to planned', { name: asset.name })} disabled={readOnly} onClick={(event) => { event.currentTarget.closest('details')?.removeAttribute('open'); void transition(asset, 'planned'); }}>{t('Move to planned')}</button></div>
                </details>
              </div>
            </li>;
          })}</ul>}
      </div>
      {data && <footer className="todo-pagination"><span>{totalPages === null ? t('Page {page}', { page }) : t('Page {page} of {totalPages} ({totalItems} total)', { page, totalPages, totalItems: total })}</span><div><button className="icon-button" title={t('Previous Page')} aria-label={t('Previous Page')} disabled={disabled || page <= 1} onClick={() => onSelectPage(page - 1)}><Icon name="left" size={14} /></button><button className="icon-button" title={t('Next Page')} aria-label={t('Next Page')} disabled={disabled || !hasNextPage} onClick={() => onSelectPage(page + 1)}><Icon name="right" size={14} /></button></div></footer>}
    </main>
  );
};
