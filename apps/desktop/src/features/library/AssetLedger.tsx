import React, { useEffect, useRef } from 'react';
import { Badge } from '../../ui/Badge';
import type {
  ActiveModule,
  AppCapabilities,
  AssetSummary,
  DesktopError,
  LifecycleOption,
  Page,
  SortOption,
} from './types';

interface AssetLedgerProps {
  module: ActiveModule;
  lifecycle: LifecycleOption;
  sort: SortOption;
  selectedKind: string | null;
  selectedTag: string | null;
  searchQuery: string;
  page: number;
  pageSize: number;
  data: Page<AssetSummary> | null;
  error: DesktopError | null;
  loading: boolean;
  selectedAssetId: string | null;
  capabilities: AppCapabilities | null;
  onSelectAsset: (id: string) => void;
  onOpenDetail?: (id: string) => void;
  onSelectLifecycle: (lifecycle: LifecycleOption) => void;
  onSelectSort: (sort: SortOption) => void;
  onSelectKind: (kind: string | null) => void;
  onSelectTag: (tag: string | null) => void;
  onSearchChange: (search: string) => void;
  onSelectPage: (page: number) => void;
  onResetFilters: () => void;
  onRetry: () => void;
}

export const AssetLedger: React.FC<AssetLedgerProps> = ({
  module,
  lifecycle,
  sort,
  selectedKind,
  selectedTag,
  searchQuery,
  page,
  pageSize,
  data,
  error,
  loading,
  selectedAssetId,
  capabilities,
  onSelectAsset,
  onOpenDetail,
  onSelectLifecycle,
  onSelectSort,
  onSelectKind,
  onSelectTag,
  onSearchChange,
  onSelectPage,
  onResetFilters,
  onRetry,
}) => {
  const tableRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

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
      ? 'All Assets'
      : module === 'media'
      ? 'Media Library'
      : module === 'software'
      ? 'Software Inventory'
      : 'Services & Subscriptions';

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
    Boolean(searchQuery.trim()) ||
    page > 1;

  return (
    <main
      className="collection-workspace"
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        minWidth: 0,
        backgroundColor: 'var(--color-surface)',
      }}
    >
      {/* Header & Controls Toolbar */}
      <header
        style={{
          padding: '12px 16px',
          borderBottom: '1px solid var(--color-border)',
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
            <h2 style={{ fontSize: '15px', fontWeight: 600, color: 'var(--color-ink)' }}>
              {title}
            </h2>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
              {loading
                ? 'Loading assets...'
                : searchQuery.trim()
                ? `${totalItems} result${totalItems === 1 ? '' : 's'} for "${searchQuery.trim()}"`
                : data
                ? `${totalItems} asset${totalItems === 1 ? '' : 's'} recorded`
                : 'No assets'}
            </div>
          </div>

          {/* Search Input Bar */}
          <div
            style={{
              position: 'relative',
              display: 'flex',
              alignItems: 'center',
              flex: '1 1 220px',
              maxWidth: '360px',
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
              aria-label="Search library assets"
              placeholder="Search assets (Cmd+K)..."
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
                borderRadius: 'var(--radius-sm)',
                backgroundColor: 'var(--color-canvas)',
                color: 'var(--color-ink)',
                outline: 'none',
              }}
            />
            {searchQuery ? (
              <button
                onClick={() => onSearchChange('')}
                aria-label="Clear search"
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
                  backgroundColor: 'var(--color-surface)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  pointerEvents: 'none',
                }}
              >
                ⌘K
              </kbd>
            )}
          </div>

          {/* Quick Filter Controls */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
            {/* Lifecycle Selector */}
            <select
              aria-label="Filter by lifecycle"
              value={lifecycle}
              onChange={(e) => onSelectLifecycle(e.target.value as LifecycleOption)}
              style={{
                padding: '4px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
                cursor: 'pointer',
              }}
            >
              <option value="active">Active Only</option>
              <option value="active_or_archived">Active + Archived</option>
              <option value="all">All (incl. Merged)</option>
            </select>

            {/* Kind Selector */}
            {availableKinds.length > 0 && (
              <select
                aria-label="Filter by kind"
                value={selectedKind || ''}
                onChange={(e) => onSelectKind(e.target.value || null)}
                style={{
                  padding: '4px 8px',
                  fontSize: '12px',
                  borderRadius: 'var(--radius-sm)',
                  border: '1px solid var(--color-border)',
                  backgroundColor: 'var(--color-surface)',
                  color: 'var(--color-ink)',
                  cursor: 'pointer',
                }}
              >
                <option value="">All Kinds</option>
                {availableKinds.map((k) => (
                  <option key={k} value={k}>
                    {k}
                  </option>
                ))}
              </select>
            )}

            {/* Sort Selector */}
            <select
              aria-label="Sort assets"
              value={sort}
              onChange={(e) => onSelectSort(e.target.value as SortOption)}
              style={{
                padding: '4px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
                cursor: 'pointer',
              }}
            >
              <option value="updated_desc">Recently Updated</option>
              <option value="updated_asc">Oldest Updated</option>
              <option value="name_asc">Name (A-Z)</option>
              <option value="name_desc">Name (Z-A)</option>
              <option value="kind_asc">Asset Kind</option>
            </select>
          </div>
        </div>

        {/* Active Filters Summary Chips */}
        {(selectedTag || selectedKind || Boolean(searchQuery.trim()) || isFiltered) && (
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
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '11px',
                  color: 'var(--color-mesh)',
                  fontWeight: 500,
                }}
              >
                query: &quot;{searchQuery.trim()}&quot;
                <button
                  onClick={() => onSearchChange('')}
                  aria-label="Remove search filter"
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
                  backgroundColor: 'var(--color-canvas)',
                  border: '1px solid var(--color-border)',
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '11px',
                  color: 'var(--color-ink)',
                }}
              >
                tag: #{selectedTag}
                <button
                  onClick={() => onSelectTag(null)}
                  aria-label="Remove tag filter"
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
                  backgroundColor: 'var(--color-canvas)',
                  border: '1px solid var(--color-border)',
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '11px',
                  color: 'var(--color-ink)',
                }}
              >
                kind: {selectedKind}
                <button
                  onClick={() => onSelectKind(null)}
                  aria-label="Remove kind filter"
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
              >
                Reset filters
              </button>
            )}
          </div>
        )}
      </header>

      {/* Error state */}
      {error && (
        <div
          role="alert"
          style={{
            margin: '16px',
            padding: '16px',
            backgroundColor: 'var(--color-danger-bg)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-md)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
            <Badge variant="danger">{error.category}</Badge>
            <span style={{ fontWeight: 600, color: 'var(--color-danger)' }}>
              Query Execution Failed
            </span>
          </div>
          <p style={{ color: 'var(--color-ink)', fontSize: '12px', marginBottom: '12px' }}>
            {error.message}
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
          >
            Retry
          </button>
        </div>
      )}

      {/* Ledger Table / List */}
      <div
        ref={tableRef}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-label="Asset Ledger"
        style={{
          flex: 1,
          overflowY: 'auto',
          outline: 'none',
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
              {searchQuery.trim() ? 'No matching assets found' : 'No assets found'}
            </div>
            <p style={{ fontSize: '12px', marginBottom: '16px' }}>
              {searchQuery.trim()
                ? `No assets found matching "${searchQuery.trim()}". Try different keywords or reset filters.`
                : 'No assets match the current view and filter criteria.'}
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
              >
                Reset All Filters
              </button>
            )}
          </div>
        ) : (
          <div role="table" aria-label="Assets">
            {data?.items.map((asset) => {
              const isSelected = asset.id === selectedAssetId;
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
                    padding: '10px 16px',
                    borderBottom: '1px solid var(--color-border-subtle)',
                    backgroundColor: isSelected ? 'var(--color-surface-active)' : 'transparent',
                    borderLeft: isSelected ? '3px solid var(--color-mesh)' : '3px solid transparent',
                    cursor: 'pointer',
                    outline: 'none',
                    transition: 'background-color 0.1s ease',
                  }}
                  className="asset-row"
                >
                  <div style={{ flex: 1, minWidth: 0, paddingRight: '12px' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
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
                      <Badge variant="muted">{asset.kind}</Badge>
                      {asset.lifecycle !== 'active' && (
                        <Badge variant={asset.lifecycle === 'archived' ? 'attention' : 'danger'}>
                          {asset.lifecycle}
                        </Badge>
                      )}
                    </div>
                    {asset.subtitle && (
                      <div
                        style={{
                          fontSize: '12px',
                          color: 'var(--color-muted)',
                          marginTop: '2px',
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {asset.subtitle}
                      </div>
                    )}
                  </div>

                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px',
                      flexShrink: 0,
                    }}
                  >
                    {asset.tags.map((t) => (
                      <button
                        key={t}
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectTag(t);
                        }}
                        title={`Filter by #${t}`}
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
                        #{t}
                      </button>
                    ))}
                    <span
                      style={{
                        fontSize: '11px',
                        color: 'var(--color-muted)',
                        fontFamily: 'var(--font-mono)',
                        marginLeft: '8px',
                      }}
                    >
                      {asset.updated_at.slice(0, 10)}
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
            borderTop: '1px solid var(--color-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            backgroundColor: 'var(--color-canvas)',
            fontSize: '12px',
            color: 'var(--color-muted)',
          }}
        >
          <div>
            Page {page} of {totalPages}
            {totalItems > 0 && ` (${totalItems} total)`}
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button
              onClick={() => onSelectPage(page - 1)}
              disabled={!hasPrevPage}
              aria-label="Previous Page"
              style={{
                padding: '4px 10px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: hasPrevPage ? 'var(--color-ink)' : 'var(--color-muted)',
                cursor: hasPrevPage ? 'pointer' : 'not-allowed',
                opacity: hasPrevPage ? 1 : 0.5,
              }}
            >
              Previous
            </button>
            <button
              onClick={() => onSelectPage(page + 1)}
              disabled={!hasNextPage}
              aria-label="Next Page"
              style={{
                padding: '4px 10px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: hasNextPage ? 'var(--color-ink)' : 'var(--color-muted)',
                cursor: hasNextPage ? 'pointer' : 'not-allowed',
                opacity: hasNextPage ? 1 : 0.5,
              }}
            >
              Next
            </button>
          </div>
        </footer>
      )}
    </main>
  );
};
