import { Badge } from '../../ui/Badge';
import { formatRelativeTime, t } from '../../i18n';
import type { AssetSummary } from './types';
import './LedgerRow.css';

const statusTint: Record<string, { bg: string; ink: string }> = {
  planned: { bg: 'var(--color-attention-bg)', ink: 'var(--color-attention)' },
  in_progress: { bg: 'var(--color-mesh-bg)', ink: 'var(--color-mesh)' },
  completed: { bg: 'var(--color-canvas)', ink: 'var(--color-muted)' },
  paused: { bg: 'var(--color-attention-bg)', ink: 'var(--color-attention)' },
  dropped: { bg: 'var(--color-danger-bg)', ink: 'var(--color-danger)' },
};

interface LedgerRowProps {
  asset: AssetSummary;
  selected: boolean;
  onSelect: (id: string) => void;
  onOpen?: (id: string) => void;
  onSelectTag: (tag: string) => void;
}

/** One row owns its keyboard interaction and module-specific display columns. */
export function LedgerRow({ asset, selected, onSelect, onOpen, onSelectTag }: LedgerRowProps) {
  const media = asset.details?.module === 'media' ? asset.details : null;
  const tint = media ? statusTint[media.status] : undefined;
  const progress = media?.progress && (media.progress.current != null || media.progress.total != null)
    ? `${media.progress.unit ? `${t(media.progress.unit)} ` : ''}${media.progress.current ?? '·'}/${media.progress.total ?? '·'}`
    : null;
  return <div role="row" tabIndex={0} className={`asset-row ledger-row${selected ? ' ledger-row-selected' : ''}`}
    onClick={() => onSelect(asset.id)} onDoubleClick={() => onOpen?.(asset.id)}
    onKeyDown={(event) => {
      if (event.key === 'Enter') { event.preventDefault(); (onOpen ?? onSelect)(asset.id); }
      else if (event.key === ' ') { event.preventDefault(); onSelect(asset.id); }
    }}>
    <div className="ledger-row-identity">
      <span className="ledger-row-name">{asset.name}</span>
      {media && tint && <span className="ledger-row-status" style={{ color: tint.ink, backgroundColor: tint.bg }}>{t(media.status)}</span>}
      <Badge variant="muted">{t(asset.kind)}</Badge>
      {asset.lifecycle !== 'active' && <Badge variant={asset.lifecycle === 'archived' ? 'attention' : 'danger'}>{t(asset.lifecycle)}</Badge>}
    </div>
    <div className="ledger-row-columns">
      {asset.tags.map((tag) => <button key={tag} className="ledger-row-tag" title={t('Filter by #{tag}', { tag })}
        onClick={(event) => { event.stopPropagation(); onSelectTag(tag); }}>#{tag}</button>)}
      {!media && asset.subtitle && <span className="ledger-row-subtitle">{asset.subtitle}</span>}
      <span className={`ledger-row-year${media?.year == null ? ' ledger-row-year-empty' : ''}`}>{media?.year ?? '—'}</span>
      <span className="ledger-row-progress">{progress ?? (media ? t('No progress recorded') : '—')}</span>
      <span className={`ledger-row-rating${media?.rating != null ? ' ledger-row-rated' : ''}`}>
        {media?.rating != null ? `★ ${media.rating.toFixed(1)}` : media ? t('Unrated') : '—'}
      </span>
      <span className="ledger-row-updated">{formatRelativeTime(asset.updated_at)}</span>
    </div>
  </div>;
}
