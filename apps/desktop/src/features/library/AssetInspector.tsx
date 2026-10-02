import React, { useEffect, useRef, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { Icon } from '../../ui/Icon';
import { formatRelativeTime, t } from '../../i18n';
import { getTransport } from './transport';
import { completionLabel, defaultProgressUnit, MEDIA_STATUSES } from './media-presentation';
import { AssetArtwork } from './AssetArtwork';
import type { AssetSummary, RelationViewDto } from './types';

interface AssetInspectorProps {
  asset: AssetSummary | null;
  mobileOpen?: boolean;
  mutationPending?: boolean;
  onClose?: () => void;
  onSelectTag?: (tag: string) => void;
  onOpenDetail?: (assetId: string) => void;
  onOpenRelations?: () => void;
  onMediaStatusChange?: (asset: AssetSummary, status: string) => Promise<void> | void;
  onUpdateProgress?: (asset: AssetSummary, progress: { current?: number; total?: number; unit?: string }) => Promise<void>;
}

export const AssetInspector: React.FC<AssetInspectorProps> = ({ asset, mobileOpen = true, mutationPending = false, onClose, onSelectTag, onOpenDetail, onOpenRelations, onMediaStatusChange, onUpdateProgress }) => {
  const [relations, setRelations] = useState<RelationViewDto[] | null>(null);
  const [relationsFailed, setRelationsFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const saving = useRef(false);
  const [editingProgress, setEditingProgress] = useState(false);
  const media = asset?.details?.module === 'media' ? asset.details : null;
  const [current, setCurrent] = useState<string>(() => String(media?.progress?.current ?? 0));
  const [total, setTotal] = useState<string>(() => media?.progress?.total == null ? '' : String(media.progress.total));
  const [unit, setUnit] = useState<string>(() => media?.progress?.unit ?? defaultProgressUnit(media?.media_type ?? 'anime'));
  const progress = media?.progress;
  const percentage = progress?.current != null && progress.total != null && progress.total > 0
    ? Math.max(0, Math.min(100, progress.current / progress.total * 100)) : null;
  const editable = asset?.lifecycle === 'active';
  const disabled = busy || mutationPending || !editable;
  const [width, setWidth] = useState(310);
  const drag = useRef<{ x: number; width: number } | null>(null);

  useEffect(() => {
    let alive = true;
    setRelations(null); setRelationsFailed(false);
    if (asset) getTransport().relationList(asset.id).then((rows) => { if (alive) setRelations(rows); }).catch(() => { if (alive) setRelationsFailed(true); });
    return () => { alive = false; };
  }, [asset]);

  useEffect(() => { setEditingProgress(false); setCopied(false); }, [asset?.id]);

  const changeStatus = async (status: string) => {
    if (!asset || !onMediaStatusChange || disabled || saving.current || media?.status === status) return;
    saving.current = true; setBusy(true);
    try { await onMediaStatusChange(asset, status); }
    catch { /* Shared feedback displays the failure. */ }
    finally { saving.current = false; setBusy(false); }
  };
  const openProgress = () => {
    setCurrent(String(media?.progress?.current ?? 0));
    setTotal(media?.progress?.total == null ? '' : String(media.progress.total));
    setUnit(media?.progress?.unit ?? defaultProgressUnit(media?.media_type ?? 'anime'));
    setEditingProgress(true);
  };
  const saveProgress = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!asset || !onUpdateProgress || disabled || saving.current) return;
    saving.current = true; setBusy(true);
    try {
      await onUpdateProgress(asset, { current: current === '' ? undefined : Number(current), total: total === '' ? undefined : Number(total), unit: unit.trim() || undefined });
      setEditingProgress(false);
    } catch { /* Preserve the draft for explicit retry. */ }
    finally { saving.current = false; setBusy(false); }
  };
  const copyId = async () => {
    if (!asset) return;
    try { await navigator.clipboard.writeText(asset.id); setCopied(true); }
    catch { /* The identifier remains selectable in technical details. */ }
  };
  const clampWidth = (value: number) => Math.min(440, Math.max(260, value));

  return (
    <aside aria-label={t('Asset Inspector')} className={`inspector-panel${mobileOpen ? ' inspector-open' : ''}`} style={{ '--inspector-width': `${width}px` } as React.CSSProperties}>
      <div className="inspector-resizer" role="separator" aria-label={t('Resize inspector')} aria-orientation="vertical" aria-valuemin={260} aria-valuemax={440} aria-valuenow={width} tabIndex={0}
        onPointerDown={(event) => { drag.current = { x: event.clientX, width }; event.currentTarget.setPointerCapture(event.pointerId); }}
        onPointerMove={(event) => { if (drag.current) setWidth(clampWidth(drag.current.width + drag.current.x - event.clientX)); }}
        onPointerUp={() => { drag.current = null; }} onPointerCancel={() => { drag.current = null; }}
        onKeyDown={(event) => { if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') { event.preventDefault(); setWidth((value) => clampWidth(value + (event.key === 'ArrowLeft' ? 10 : -10))); } }} />
      <div className="inspector-scroll">
        <header className="inspector-header"><h2>{t('Inspector')}</h2>{onClose && <button className="icon-button" onClick={onClose} aria-label={t('Close Inspector')}><Icon name="close" size={16} /></button>}</header>
        {asset ? <>
          <div className="inspector-identity">
            <AssetArtwork asset={asset} large />
            <div className="inspector-identity-text"><h3>{asset.name}</h3>
              <p>{t(asset.kind)}{media?.year ? ` · ${media.year}` : ''}</p>
              {asset.subtitle && asset.subtitle !== media?.media_type && <p>{asset.subtitle}</p>}
              {media?.platform && <p>{media.platform}</p>}
              {media && <span className={`inspector-status inspector-status-${media.status}`}><span aria-hidden="true" />{t(media.status)}</span>}
              {asset.lifecycle !== 'active' && <Badge variant={asset.lifecycle === 'archived' ? 'attention' : 'danger'}>{t(asset.lifecycle)}</Badge>}
            </div>
          </div>
          {media && <section className="inspector-section inspector-media">
            <h4>{media.media_type === 'game' ? t('Game progress') : t('Watch progress')}</h4>
            <div className="inspector-progress-value">{progress && (progress.current != null || progress.total != null)
              ? <>{progress.current ?? '—'}{progress.total != null ? ` / ${progress.total}` : ''} <span>{progress.unit ? t(progress.unit) : ''}</span></>
              : <span className="muted">{t('No progress recorded')}</span>}</div>
            {percentage !== null ? <div className="inspector-progress-track" role="progressbar" aria-label={t('Viewing progress')} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percentage)}><span style={{ width: `${percentage}%` }} /></div> : <div className="inspector-progress-track" aria-hidden="true" />}
            {editingProgress ? <form className="inspector-progress-form" onSubmit={saveProgress}>
              <div className="inspector-progress-fields">
                <label>{t('Current')}<input autoFocus type="number" min="0" step="any" required max={total === '' ? undefined : Number(total)} aria-label={t('Current progress')} value={current} onChange={(event) => setCurrent(event.target.value)} disabled={disabled} /></label>
                <label>{t('Total')}<input type="number" min="0.000001" step="any" aria-label={t('Total progress')} value={total} onChange={(event) => setTotal(event.target.value)} disabled={disabled} /></label>
                <label>{t('Unit')}<input aria-label={t('Progress unit')} value={unit} onChange={(event) => setUnit(event.target.value)} disabled={disabled} /></label>
              </div>
              <div className="inspector-actions"><button className="native-button native-button-primary" type="submit" disabled={disabled}>{busy ? t('Saving...') : t('Save Progress')}</button><button className="native-button" type="button" disabled={busy} onClick={() => setEditingProgress(false)}>{t('Cancel')}</button></div>
            </form> : <div className="inspector-actions">
              {onUpdateProgress && editable && <button className="native-button native-button-primary" onClick={openProgress} disabled={disabled}>{t('Update progress')}</button>}
              {onMediaStatusChange && editable && media.status !== 'completed' && <button className="native-button" onClick={() => void changeStatus('completed')} disabled={disabled}>{completionLabel(media.media_type)}</button>}
            </div>}
            <label className="inspector-field"><span>{t('Status')}</span><select aria-label={t('Media Status')} value={media.status} disabled={disabled || !onMediaStatusChange} onChange={(event) => void changeStatus(event.target.value)}>{MEDIA_STATUSES.map((status) => <option key={status} value={status}>{t(status)}</option>)}</select></label>
            <div className="inspector-field"><span>{t('My Rating')}</span><div className="inspector-rating" aria-label={media.rating == null ? t('Unrated') : `${media.rating} / 10`}>
              <span className="rating-stars" aria-hidden="true">{Array.from({ length: 5 }, (_, index) => <span key={index} className={media.rating != null && media.rating >= (index + 1) * 2 ? 'rating-star-filled' : ''}><Icon name="star" size={19} /></span>)}</span>
              <span className="muted">{media.rating == null ? t('Unrated') : `${media.rating.toFixed(1)} / 10`}</span>
            </div></div>
          </section>}
          {media && <section className="inspector-section"><h4>{t('Notes')}</h4>{media.notes ? <p className="inspector-notes">{media.notes}</p> : onOpenDetail && editable ? <button className="inspector-add-note" onClick={() => onOpenDetail(asset.id)}>{media.media_type === 'game' ? t('Add notes…') : t('Add viewing notes…')}</button> : <p className="muted">{t('No notes')}</p>}</section>}
          <details className="quiet-details inspector-disclosure"><summary>{t('Related assets')}{relations && relations.length > 0 ? ` (${relations.length})` : ''}</summary>
            {relationsFailed ? <p className="muted">{t('Could not load relations')}</p> : relations === null ? <p className="muted">{t('Loading...')}</p> : relations.length === 0 ? <p className="muted">{t('No relations')}</p> : <div className="inspector-relations">{relations.slice(0, 3).map((relation) => <div key={relation.relation_id}><span>{relation.other_asset_name}</span><span className="muted">{t(relation.relation_type)}</span></div>)}{onOpenRelations && <button className="text-button" onClick={onOpenRelations}>{t('Open in relations')}</button>}</div>}
          </details>
          {asset.tags.length > 0 && <section className="inspector-tags"><h4>{t('Tags')}</h4><div>{asset.tags.map((tag) => <button key={tag} className="text-button" onClick={() => onSelectTag?.(tag)} disabled={!onSelectTag} title={t('Filter by #{tag}', { tag })}><Badge variant="muted">#{tag}</Badge></button>)}</div></section>}
          <details className="quiet-details inspector-disclosure"><summary>{t('Technical details')}</summary><div className="inspector-technical"><span title={asset.id}>{`${asset.id.slice(0, 8)}…${asset.id.slice(-4)}`}</span><button className="text-button" onClick={copyId} aria-label={t('Copy ID')}>{copied ? t('Copied') : t('Copy ID')}</button></div></details>
          <footer className="inspector-footer"><span>{t('Last Updated')} {formatRelativeTime(asset.updated_at)}</span>{onOpenDetail && <button className="text-button" aria-label={t('View Full Details')} onClick={() => onOpenDetail(asset.id)}>{t('View Full Details')} <Icon name="right" size={12} /></button>}</footer>
        </> : <div className="inspector-empty"><Icon name="inspector" size={30} /><p>{t('Select an asset to view details.')}</p></div>}
      </div>
    </aside>
  );
};
