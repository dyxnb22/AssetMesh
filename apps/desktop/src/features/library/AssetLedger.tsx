import React, { useEffect, useRef, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { formatRelativeTime, t } from '../../i18n';
import type {
  ActiveModule,
  AppCapabilities,
  AssetSummary,
  DesktopError,
  LifecycleOption,
  MediaStatusCountDto,
  Page,
  SortOption,
} from './types';
import { SavedFilters, type SavedFilterState } from './SavedFilters';

/** Canonical media statuses, in segmented-control order. */
const MEDIA_STATUS_KEYS = ['planned', 'in_progress', 'completed', 'paused', 'dropped'] as const;

const statusTint: Record<string, { bg: string; ink: string }> = {
  planned: { bg: 'var(--color-attention-bg)', ink: 'var(--color-attention)' },
  in_progress: { bg: 'var(--color-mesh-bg)', ink: 'var(--color-mesh)' },
  completed: { bg: 'var(--color-canvas)', ink: 'var(--color-muted)' },
  paused: { bg: 'var(--color-attention-bg)', ink: 'var(--color-attention)' },
  dropped: { bg: 'var(--color-danger-bg)', ink: 'var(--color-danger)' },
};

const podStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 2,
  padding: 2,
  backgroundColor: 'var(--color-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 999,
  boxShadow: '0 1px 3px rgba(23, 33, 38, 0.06)',
};

const segBtn = (active: boolean): React.CSSProperties => ({
  display: 'inline-flex',
  alignItems: 'center',
  gap: 5,
  height: 26,
  padding: '0 10px',
  border: 'none',
  borderRadius: 999,
  cursor: 'pointer',
  fontSize: 12,
  whiteSpace: 'nowrap',
  backgroundColor: active ? 'var(--color-mesh)' : 'transparent',
  color: active ? '#ffffff' : 'var(--color-ink)',
  fontWeight: active ? 600 : 400,
});

const segCount: React.CSSProperties = {
  fontSize: 10,
  fontWeight: 600,
  color: 'var(--color-muted)',
};

const selectStyle: React.CSSProperties = {
  padding: '5px 10px',
  fontSize: 12,
  borderRadius: 999,
  border: '1px solid var(--color-border)',
  backgroundColor: 'var(--color-surface)',
  color: 'var(--color-ink)',
  cursor: 'pointer',
  boxShadow: '0 1px 3px rgba(23, 33, 38, 0.06)',
};

interface AssetLedgerProps {
  module: ActiveModule;
  lifecycle: LifecycleOption;
  sort: SortOption;
  selectedKind: string | null;
  selectedTag: string | null;
  selectedMediaStatus: string | null;
  statusCounts: MediaStatusCountDto[] | null;
  searchQuery: string;
  page: number;
  pageSize: number;
  data: Page<AssetSummary> | null;
  error: DesktopError | null;
  loading: boolean;
  selectedAssetId: string | null;
  capabilities: AppCapabilities | null;
  onNewAsset?: () => void;
  onImportInfo?: () => void;
  onDiscoverSoftware?: () => void;
  onSelectAsset: (id: string) => void;
  onOpenDetail?: (id: string) => void;
  onSelectLifecycle: (lifecycle: LifecycleOption) => void;
  onSelectMediaStatus: (status: string | null) => void;
  onSelectSort: (sort: SortOption) => void;
  onSelectKind: (kind: string | null) => void;
  onSelectTag: (tag: string | null) => void;
  onSearchChange: (search: string) => void;
  onSelectPage: (page: number) => void;
  onResetFilters: () => void;
  onApplySavedFilter: (state: SavedFilterState) => void;
  onRetry: () => void;
}

export const AssetLedger: React.FC<AssetLedgerProps> = ({
  module,
  lifecycle,
  sort,
  selectedKind,
  selectedTag,
  selectedMediaStatus,
  statusCounts,
  searchQuery,
  page,
  pageSize,
  data,
  error,
  loading,
  selectedAssetId,
  capabilities,
  onNewAsset,
  onImportInfo,
  onDiscoverSoftware,
  onSelectAsset,
  onOpenDetail,
  onSelectLifecycle,
  onSelectMediaStatus,
  onSelectSort,
  onSelectKind,
  onSelectTag,
  onSearchChange,
  onSelectPage,
  onResetFilters,
  onApplySavedFilter,
  onRetry,
}) => {
  const tableRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [savedOpen, setSavedOpen] = useState(false);

  useEffect(() => {
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      } else if (
        e.key === '/' &&
        document.activeElement?.tagName !== 'INPUT' &&
        document.activeElement?.tagName !== 'TEXTAREA' &&
        document.activeElement?.tagName !== 'SELECT'
      ) {
        e.preventDefault();
        searchInputRef.current?.focus();
      }
    };
    window.addEventListener('keydown', handleGlobalKeyDown);
    return () => window.removeEventListener('keydown', handleGlobalKeyDown);
  }, []);

  const title =
    module === 'all'
      ? t('All Assets')
      : module === 'media'
      ? t('Media Library')
      : module === 'software'
      ? t('Software Inventory')
      : module === 'services'
      ? t('Services & Subscriptions')
      : t('Information Library');

  // Filter available kinds by active module
  const availableKinds =
    capabilities?.asset_kinds.filter((k) => {
      if (module === 'all') return true;
      return k.startsWith(module);
    }) || [];

  const totalItems = data?.total ?? data?.items.length ?? 0;
  const totalPages = Math.max(1, Math.ceil(totalItems / pageSize));
  const hasNextPage = data ? page < totalPages : false;
  const hasPrevPage = page > 1;

  // Keyboard navigation through ledger rows
  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (!data || data.items.length === 0) return;

    const currentIndex = data.items.findIndex((item) => item.id === selectedAssetId);

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const nextIndex = currentIndex < data.items.length - 1 ? currentIndex + 1 : 0;
      onSelectAsset(data.items[nextIndex].id);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prevIndex = currentIndex > 0 ? currentIndex - 1 : data.items.length - 1;
      onSelectAsset(data.items[prevIndex].id);
    }
  };

  const isFiltered =
    lifecycle !== 'active' ||
    sort !== 'updated_desc' ||
    selectedKind !== null ||
    selectedTag !== null ||
    (module === 'media' && selectedMediaStatus !== null) ||
    Boolean(searchQuery.trim()) ||
    page > 1;

  const allCount = statusCounts ? statusCounts.reduce((n, c) => n + c.count, 0) : null;

  return (
    <main
      className="collection-workspace"
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        minWidth: 0,
        backgroundColor: 'var(--color-canvas)',
      }}
    >
      {/* Header & Controls Toolbar */}
      <header
        style={{
          padding: '14px 16px 10px',
          display: 'flex',
          flexDirection: 'column',
          gap: '10px',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            flexWrap: 'wrap',
            gap: '10px',
          }}
        >
          <div>
            <h2 style={{ fontSize: '17px', fontWeight: 650, color: 'var(--color-ink)' }}>
              {title}
            </h2>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
              {loading
                ? t('Loading assets...')
                : searchQuery.trim()
                ? t(
                    totalItems === 1
                      ? '{n} result for "{q}"'
                      : '{n} results for "{q}"',
                    { n: totalItems, q: searchQuery.trim() }
                  )
                : data
                ? t(totalItems === 1 ? '{n} asset recorded' : '{n} assets recorded', {
                    n: totalItems,
                  })
                : t('No assets')}
            </div>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            {/* Saved filters live in a popover so the toolbar stays a browse tool */}
            <div style={{ position: 'relative' }}>
              <button
                type="button"
                aria-label={t('Saved filters')}
                aria-expanded={savedOpen}
                onClick={() => setSavedOpen((open) => !open)}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  width: 32,
                  height: 32,
                  backgroundColor: 'var(--color-surface)',
                  color: savedOpen ? 'var(--color-mesh)' : 'var(--color-muted)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 999,
                  cursor: 'pointer',
                  boxShadow: '0 1px 3px rgba(23, 33, 38, 0.06)',
                }}
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <path d="M4 1.5h8a1 1 0 0 1 1 1v12l-5-3-5 3v-12a1 1 0 0 1 1-1Z" />
                </svg>
              </button>
              {savedOpen && (
                <div
                  style={{
                    position: 'absolute',
                    top: 'calc(100% + 6px)',
                    right: 0,
                    zIndex: 40,
                    width: 360,
                    padding: 10,
                    backgroundColor: 'var(--color-surface)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-lg)',
                    boxShadow: '0 8px 24px rgba(23, 33, 38, 0.14)',
                  }}
                >
                  <SavedFilters
                    current={{
                      module,
                      lifecycle,
                      mediaStatus: module === 'media' ? selectedMediaStatus ?? null : null,
                      sort,
                      kind: selectedKind,
                      tag: selectedTag,
                      search: searchQuery,
                    }}
                    onApply={(state) => {
                      onApplySavedFilter(state);
                      setSavedOpen(false);
                    }}
                  />
                </div>
              )}
            </div>

            {module === 'info' && onImportInfo && <button type="button" onClick={onImportInfo}>{t('Import CSV')}</button>}
            {onDiscoverSoftware && module === 'software' && (
              <button
                type="button"
                data-testid="discover-software-button"
                onClick={onDiscoverSoftware}
                style={{
                  padding: '6px 14px',
                  backgroundColor: 'var(--color-canvas)',
                  color: 'var(--color-ink)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '12px',
                  fontWeight: 500,
                  cursor: 'pointer',
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                }}
              >{t('🔍 Scan System')}</button>
            )}

            {onNewAsset && (
              <button
                type="button"
                data-testid="new-asset-button"
                onClick={onNewAsset}
                style={{
                  padding: '7px 14px',
                  backgroundColor: 'var(--color-mesh)',
                  color: '#ffffff',
                  border: 'none',
                  borderRadius: 999,
                  fontSize: '12px',
                  fontWeight: 600,
                  cursor: 'pointer',
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                  boxShadow: '0 2px 8px rgba(20, 125, 120, 0.28)',
                }}
              >
                +{' '}
                {module === 'media'
                  ? t('Add Media')
                  : module === 'software'
                  ? t('Add Software')
                  : module === 'services'
                  ? t('Add Service')
                  : t('New Asset')}
              </button>
            )}
          </div>
        </div>

        {/* Filter toolbar */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
          {/* Media watch-status segmented control */}
          {module === 'media' && onSelectMediaStatus && (
            <div role="group" aria-label={t('Filter by media status')} style={podStyle}>
              <button
                type="button"
                onClick={() => onSelectMediaStatus(null)}
                aria-pressed={!selectedMediaStatus}
                style={segBtn(!selectedMediaStatus)}
              >
                {t('All')}
                {allCount !== null && <span style={segCount}>{allCount}</span>}
              </button>
              {MEDIA_STATUS_KEYS.map((key) => {
                const count = statusCounts?.find((c) => c.status === key)?.count;
                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => onSelectMediaStatus(key)}
                    aria-pressed={selectedMediaStatus === key}
                    style={segBtn(selectedMediaStatus === key)}
                  >
                    {t(key)}
                    {count != null && <span style={segCount}>{count}</span>}
                  </button>
                );
              })}
            </div>
          )}

          {/* Lifecycle Selector */}
          <select
            aria-label={t('Filter by lifecycle')}
            value={lifecycle}
            onChange={(e) => onSelectLifecycle(e.target.value as LifecycleOption)}
            style={selectStyle}
          >
            <option value="active">{t('Active Only')}</option>
            <option value="active_or_archived">{t('Active + Archived')}</option>
            <option value="all">{t('All (incl. Merged)')}</option>
          </select>

          {/* Kind Selector */}
          {availableKinds.length > 0 && (
            <select
              aria-label={t('Filter by kind')}
              value={selectedKind || ''}
              onChange={(e) => onSelectKind(e.target.value || null)}
              style={selectStyle}
            >
              <option value="">{t('All Kinds')}</option>
              {availableKinds.map((k) => (
                <option key={k} value={k}>
                  {t(k)}
                </option>
              ))}
            </select>
          )}

          {/* Sort Selector */}
          <select
            aria-label={t('Sort assets')}
            value={sort}
            onChange={(e) => onSelectSort(e.target.value as SortOption)}
            style={selectStyle}
          >
            <option value="updated_desc">{t('Recently Updated')}</option>
            <option value="updated_asc">{t('Oldest Updated')}</option>
            <option value="name_asc">{t('Name (A-Z)')}</option>
            <option value="name_desc">{t('Name (Z-A)')}</option>
            <option value="kind_asc">{t('Asset Kind')}</option>
          </select>

          {/* Search Input */}
          <div
            style={{
              position: 'relative',
              display: 'flex',
              alignItems: 'center',
              flex: '1 1 200px',
              maxWidth: '340px',
              minWidth: '160px',
            }}
          >
            <span
              style={{
                position: 'absolute',
                left: '10px',
                color: 'var(--color-muted)',
                pointerEvents: 'none',
                display: 'flex',
                alignItems: 'center',
              }}
              aria-hidden="true"
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
              </svg>
            </span>
            <input
              ref={searchInputRef}
              type="search"
              aria-label={t('Search library assets')}
              placeholder={t('Search assets (Cmd+K)...')}
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  if (searchQuery) {
                    onSearchChange('');
                  } else {
                    searchInputRef.current?.blur();
                  }
                }
              }}
              style={{
                width: '100%',
                padding: '6px 32px 6px 30px',
                fontSize: '12px',
                border: '1px solid var(--color-border)',
                borderRadius: 999,
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
                outline: 'none',
                boxShadow: '0 1px 3px rgba(23, 33, 38, 0.06)',
              }}
            />
            {searchQuery ? (
              <button
                onClick={() => onSearchChange('')}
                aria-label={t('Clear search')}
                style={{
                  position: 'absolute',
                  right: '8px',
                  border: 'none',
                  background: 'none',
                  color: 'var(--color-muted)',
                  cursor: 'pointer',
                  padding: '2px',
                  fontSize: '12px',
                  lineHeight: 1,
                }}
              >
                ✕
              </button>
            ) : (
              <kbd
                style={{
                  position: 'absolute',
                  right: '8px',
                  padding: '1px 5px',
                  fontSize: '10px',
                  fontWeight: 600,
                  color: 'var(--color-muted)',
                  backgroundColor: 'var(--color-canvas)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  pointerEvents: 'none',
                }}
              >
                ⌘K
              </kbd>
            )}
          </div>
        </div>

        {/* Active Filters Summary Chips */}
        {(selectedTag ||
          selectedKind ||
          (module === 'media' && selectedMediaStatus) ||
          Boolean(searchQuery.trim()) ||
          isFiltered) && (
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexWrap: 'wrap' }}>
            {searchQuery.trim() && (
              <span
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                  backgroundColor: 'rgba(20, 125, 120, 0.1)',
                  border: '1px solid var(--color-mesh)',
                  padding: '2px 8px',
                  borderRadius: 999,
                  fontSize: '11px',
                  color: 'var(--color-mesh)',
                  fontWeight: 500,
                }}
              >
                {t('query: "{q}"', { q: searchQuery.trim() })}
                <button
                  onClick={() => onSearchChange('')}
                  aria-label={t('Remove search filter')}
                  style={{
                    border: 'none',
                    background: 'none',
                    cursor: 'pointer',
                    color: 'var(--color-mesh)',
                    marginLeft: '2px',
                    fontSize: '12px',
                  }}
                >
                  ✕
                </button>
              </span>
            )}
            {selectedTag && (
              <span
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                  backgroundColor: 'var(--color-surface)',
                  border: '1px solid var(--color-border)',
                  padding: '2px 8px',
                  borderRadius: 999,
                  fontSize: '11px',
                  color: 'var(--color-ink)',
                }}
              >
                {t('tag: #{tag}', { tag: selectedTag })}
                <button
                  onClick={() => onSelectTag(null)}
                  aria-label={t('Remove tag filter')}
                  style={{
                    border: 'none',
                    background: 'none',
                    cursor: 'pointer',
                    color: 'var(--color-muted)',
                    marginLeft: '2px',
                    fontSize: '12px',
                  }}
                >
                  ✕
                </button>
              </span>
            )}

            {selectedKind && (
              <span
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                  backgroundColor: 'var(--color-surface)',
                  border: '1px solid var(--color-border)',
                  padding: '2px 8px',
                  borderRadius: 999,
                  fontSize: '11px',
                  color: 'var(--color-ink)',
                }}
              >
                {t('kind: {kind}', { kind: t(selectedKind) })}
                <button
                  onClick={() => onSelectKind(null)}
                  aria-label={t('Remove kind filter')}
                  style={{
                    border: 'none',
                    background: 'none',
                    cursor: 'pointer',
                    color: 'var(--color-muted)',
                    marginLeft: '2px',
                    fontSize: '12px',
                  }}
                >
                  ✕
                </button>
              </span>
            )}

            {module === 'media' && selectedMediaStatus && (
              <span
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                  backgroundColor: 'var(--color-surface)',
                  border: '1px solid var(--color-border)',
                  padding: '2px 8px',
                  borderRadius: 999,
                  fontSize: '11px',
                  color: 'var(--color-ink)',
                }}
              >
                {t('status: {status}', { status: t(selectedMediaStatus) })}
                <button
                  onClick={() => onSelectMediaStatus(null)}
                  aria-label={t('Remove status filter')}
                  style={{
                    border: 'none',
                    background: 'none',
                    cursor: 'pointer',
                    color: 'var(--color-muted)',
                    marginLeft: '2px',
                    fontSize: '12px',
                  }}
                >
                  ✕
                </button>
              </span>
            )}

            {isFiltered && (
              <button
                onClick={onResetFilters}
                style={{
                  border: 'none',
                  background: 'none',
                  cursor: 'pointer',
                  color: 'var(--color-mesh)',
                  fontSize: '11px',
                  fontWeight: 500,
                  textDecoration: 'underline',
                  padding: '2px 4px',
                }}
              >{t('Reset filters')}</button>
            )}
          </div>
        )}
      </header>

      {/* Error state */}
      {error && (
        <div
          role="alert"
          style={{
            margin: '0 16px 12px',
            padding: '16px',
            backgroundColor: 'var(--color-danger-bg)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-md)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
            <Badge variant="danger">{t(error.category)}</Badge>
            <span style={{ fontWeight: 600, color: 'var(--color-danger)' }}>{t('Query Execution Failed')}</span>
          </div>
          <p style={{ color: 'var(--color-ink)', fontSize: '12px', marginBottom: '12px' }}>
            {t(error.message)}
          </p>
          <button
            onClick={onRetry}
            style={{
              padding: '4px 12px',
              backgroundColor: 'var(--color-danger)',
              color: '#FFFFFF',
              border: 'none',
              borderRadius: 'var(--radius-sm)',
              cursor: 'pointer',
              fontSize: '12px',
              fontWeight: 500,
            }}
          >{t('Retry')}</button>
        </div>
      )}

      {/* Ledger card */}
      <div
        ref={tableRef}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-label={t('Asset Ledger')}
        style={{
          flex: 1,
          overflowY: 'auto',
          outline: 'none',
          margin: '0 16px',
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
          borderRadius: 12,
          minHeight: 0,
        }}
      >
        {!error && data && data.items.length === 0 ? (
          <div
            style={{
              padding: '48px 16px',
              textAlign: 'center',
              color: 'var(--color-muted)',
            }}
          >
            <div style={{ fontSize: '14px', fontWeight: 500, color: 'var(--color-ink)', marginBottom: '4px' }}>
              {searchQuery.trim() ? t('No matching assets found') : t('No assets found')}
            </div>
            <p style={{ fontSize: '12px', marginBottom: '16px' }}>
              {searchQuery.trim()
                ? `No assets found matching "${searchQuery.trim()}". Try different keywords or reset filters.`
                : t('No assets match the current view and filter criteria.')}
            </p>
            {isFiltered && (
              <button
                onClick={onResetFilters}
                style={{
                  padding: '6px 14px',
                  backgroundColor: 'var(--color-surface)',
                  color: 'var(--color-mesh)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  cursor: 'pointer',
                  fontWeight: 500,
                  fontSize: '12px',
                }}
              >{t('Reset All Filters')}</button>
            )}
          </div>
        ) : (
          <div role="table" aria-label={t('Assets')}>
            {data && data.items.length > 0 && (
              // Presentational column header: deliberately not role="row", so
              // assistive tech and tests counting data rows are unaffected.
              <div
                aria-hidden="true"
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  padding: '8px 16px',
                  borderBottom: '1px solid var(--color-border)',
                  fontSize: '11px',
                  fontWeight: 600,
                  color: 'var(--color-muted)',
                  letterSpacing: '0.03em',
                  position: 'sticky',
                  top: 0,
                  backgroundColor: 'var(--color-surface)',
                  zIndex: 1,
                }}
              >
                <span style={{ flex: 1, minWidth: 0 }}>{t('Name')}</span>
                <span style={{ width: 90, textAlign: 'right' }}>{t('Progress')}</span>
                <span style={{ width: 52, textAlign: 'right' }}>{t('Rating')}</span>
                <span style={{ width: 72, textAlign: 'right' }}>{t('Last Updated')}</span>
              </div>
            )}
            {data?.items.map((asset) => {
              const isSelected = asset.id === selectedAssetId;
              const media = asset.details && asset.details.module === 'media' ? asset.details : null;
              const tint = media ? statusTint[media.status] : undefined;
              const progressText =
                media?.progress && (media.progress.current != null || media.progress.total != null)
                  ? `${media.progress.unit ? `${media.progress.unit} ` : ''}${
                      media.progress.current ?? '·'
                    }/${media.progress.total ?? '·'}`
                  : null;
              const typeText = media
                ? `${t(media.media_type)}${media.year ? ` · ${media.year}` : ''}`
                : null;
              return (
                <div
                  key={asset.id}
                  role="row"
                  tabIndex={0}
                  onClick={() => onSelectAsset(asset.id)}
                  onDoubleClick={() => onOpenDetail?.(asset.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      if (onOpenDetail) {
                        onOpenDetail(asset.id);
                      } else {
                        onSelectAsset(asset.id);
                      }
                    } else if (e.key === ' ') {
                      e.preventDefault();
                      onSelectAsset(asset.id);
                    }
                  }}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '8px',
                    padding: '11px 16px',
                    borderBottom: '1px solid var(--color-border-subtle)',
                    backgroundColor: isSelected ? 'var(--color-mesh-bg)' : 'transparent',
                    borderLeft: isSelected ? '3px solid var(--color-mesh)' : '3px solid transparent',
                    cursor: 'pointer',
                    outline: 'none',
                    transition: 'background-color 0.1s ease',
                  }}
                  className="asset-row"
                >
                  <div style={{ flex: 1, minWidth: 0, display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <span
                      style={{
                        fontWeight: 600,
                        color: 'var(--color-ink)',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                        fontSize: '13px',
                      }}
                    >
                      {asset.name}
                    </span>
                    {media && tint && (
                      <span
                        style={{
                          flexShrink: 0,
                          fontSize: '10px',
                          fontWeight: 600,
                          color: tint.ink,
                          backgroundColor: tint.bg,
                          padding: '1px 8px',
                          borderRadius: 999,
                        }}
                      >
                        {t(media.status)}
                      </span>
                    )}
                    <Badge variant="muted">{t(asset.kind)}</Badge>
                    {asset.lifecycle !== 'active' && (
                      <Badge variant={asset.lifecycle === 'archived' ? 'attention' : 'danger'}>
                        {t(asset.lifecycle)}
                      </Badge>
                    )}
                  </div>

                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '8px',
                      flexShrink: 0,
                    }}
                  >
                    {asset.tags.map((tag) => (
                      <button
                        key={tag}
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectTag(tag);
                        }}
                        title={t('Filter by #{tag}', { tag })}
                        style={{
                          all: 'unset',
                          cursor: 'pointer',
                          fontSize: '11px',
                          color: 'var(--color-muted)',
                          backgroundColor: 'var(--color-canvas)',
                          padding: '2px 6px',
                          borderRadius: 'var(--radius-sm)',
                        }}
                      >
                        #{tag}
                      </button>
                    ))}
                    {!media && asset.subtitle && (
                      <span
                        style={{
                          fontSize: '11.5px',
                          color: 'var(--color-muted)',
                          maxWidth: 200,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {asset.subtitle}
                      </span>
                    )}
                    {media && (
                      <span
                        style={{
                          fontSize: '11.5px',
                          color: 'var(--color-muted)',
                          maxWidth: 140,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                          textAlign: 'right',
                        }}
                      >
                        {typeText}
                      </span>
                    )}
                    <span
                      style={{
                        width: 90,
                        fontSize: '11.5px',
                        color: progressText ? 'var(--color-muted)' : 'transparent',
                        textAlign: 'right',
                        whiteSpace: 'nowrap',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                      }}
                    >
                      {progressText ?? '—'}
                    </span>
                    <span
                      style={{
                        width: 52,
                        fontSize: '11.5px',
                        fontWeight: 600,
                        color:
                          media?.rating != null ? 'var(--color-attention)' : 'var(--color-border)',
                        textAlign: 'right',
                      }}
                    >
                      {media?.rating != null ? `★ ${media.rating.toFixed(1)}` : '—'}
                    </span>
                    <span
                      style={{
                        width: 72,
                        fontSize: '11px',
                        color: 'var(--color-muted)',
                        textAlign: 'right',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {formatRelativeTime(asset.updated_at)}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Pagination Footer */}
      {data && (
        <footer
          style={{
            padding: '8px 16px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            fontSize: '12px',
            color: 'var(--color-muted)',
          }}
        >
          <div>
            {t('Page {page} of {total}', { page, total: totalPages })}
            {totalItems > 0 && ` ${t('({n} total)', { n: totalItems })}`}
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button
              onClick={() => onSelectPage(page - 1)}
              disabled={!hasPrevPage}
              aria-label={t('Previous Page')}
              style={{
                padding: '4px 10px',
                borderRadius: 999,
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: hasPrevPage ? 'var(--color-ink)' : 'var(--color-muted)',
                cursor: hasPrevPage ? 'pointer' : 'not-allowed',
                opacity: hasPrevPage ? 1 : 0.5,
              }}
            >{t('Previous')}</button>
            <button
              onClick={() => onSelectPage(page + 1)}
              disabled={!hasNextPage}
              aria-label={t('Next Page')}
              style={{
                padding: '4px 10px',
                borderRadius: 999,
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: hasNextPage ? 'var(--color-ink)' : 'var(--color-muted)',
                cursor: hasNextPage ? 'pointer' : 'not-allowed',
                opacity: hasNextPage ? 1 : 0.5,
              }}
            >{t('Next')}</button>
          </div>
        </footer>
      )}
    </main>
  );
};
