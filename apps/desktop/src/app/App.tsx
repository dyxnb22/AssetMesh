import React, { useCallback, useEffect, useState } from 'react';
import { AssetInspector } from '../features/library/AssetInspector';
import { AssetLedger } from '../features/library/AssetLedger';
import { NavigationRail } from '../features/library/NavigationRail';
import { getTransport } from '../features/library/transport';
import type {
  AppCapabilities,
  AppStatus,
  AssetSummary,
  DesktopError,
  LibraryQuery,
  Page,
} from '../features/library/types';
import { useNavigation } from '../features/library/useNavigation';
import { Badge } from '../ui/Badge';
import '../ui/theme.css';

export const App: React.FC = () => {
  const [status, setStatus] = useState<AppStatus>({ status: 'loading' });
  const [capabilities, setCapabilities] = useState<AppCapabilities | null>(null);
  const [pageData, setPageData] = useState<Page<AssetSummary> | null>(null);
  const [loadingAssets, setLoadingAssets] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [mobileInspectorOpen, setMobileInspectorOpen] = useState(false);

  const {
    nav,
    setModule,
    setSection,
    setLifecycle,
    setSort,
    setKind,
    setTag,
    setPage,
    setSelectedAssetId,
    resetFilters,
  } = useNavigation();

  const transport = getTransport();

  // Load application capabilities and verify status
  const loadInitialState = useCallback(async () => {
    try {
      const appStatus = await transport.getStatus();
      setStatus(appStatus);

      if (appStatus.status === 'ready') {
        const caps = await transport.getCapabilities();
        setCapabilities(caps);
      }
    } catch (err: unknown) {
      setStatus({
        status: 'setup_failure',
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }, [transport]);

  useEffect(() => {
    loadInitialState();
  }, [loadInitialState]);

  const selectedAssetIdRef = React.useRef<string | null>(nav.selectedAssetId);
  selectedAssetIdRef.current = nav.selectedAssetId;

  // Load library page according to navigation / filter state
  const loadAssets = useCallback(async () => {
    if (status.status !== 'ready') return;

    setLoadingAssets(true);
    setError(null);

    const query: LibraryQuery = {
      lifecycle: nav.lifecycle,
      modules: nav.module === 'all' ? undefined : [nav.module],
      kinds: nav.kind ? [nav.kind] : undefined,
      tags: nav.tag ? [nav.tag] : undefined,
      sort: nav.sort,
      limit: nav.pageSize,
      offset: (nav.page - 1) * nav.pageSize,
    };

    try {
      const result = await transport.listAssets(query);
      setPageData(result);

      // Auto-select first asset if none selected or selected not in page
      if (result.items.length > 0) {
        const stillExists = result.items.some((i) => i.id === selectedAssetIdRef.current);
        if (!selectedAssetIdRef.current || !stillExists) {
          setSelectedAssetId(result.items[0].id);
        }
      } else if (selectedAssetIdRef.current !== null) {
        setSelectedAssetId(null);
      }
    } catch (err: unknown) {
      setError({
        category: 'LibraryQueryFailed',
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setLoadingAssets(false);
    }
  }, [
    status.status,
    nav.lifecycle,
    nav.module,
    nav.kind,
    nav.tag,
    nav.sort,
    nav.pageSize,
    nav.page,
    transport,
    setSelectedAssetId,
  ]);

  useEffect(() => {
    if (status.status === 'ready') {
      loadAssets();
    }
  }, [status.status, loadAssets]);

  if (status.status === 'loading') {
    return (
      <div
        role="status"
        aria-live="polite"
        style={{
          display: 'flex',
          height: '100vh',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: 'var(--color-canvas)',
          color: 'var(--color-muted)',
        }}
      >
        <div style={{ textAlign: 'center' }}>
          <div
            style={{
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--color-ink)',
              marginBottom: '8px',
            }}
          >
            AssetMesh
          </div>
          <div>Opening library and verifying state...</div>
        </div>
      </div>
    );
  }

  if (status.status === 'setup_failure') {
    return (
      <div
        role="alert"
        style={{
          display: 'flex',
          height: '100vh',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: 'var(--color-canvas)',
          padding: '24px',
        }}
      >
        <div
          style={{
            maxWidth: '480px',
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-lg)',
            padding: '24px',
            boxShadow: '0 4px 12px rgba(0,0,0,0.05)',
          }}
        >
          <Badge variant="attention" className="mb-2">
            Setup Required
          </Badge>
          <h2
            style={{
              fontSize: '16px',
              fontWeight: 600,
              margin: '8px 0 12px',
              color: 'var(--color-ink)',
            }}
          >
            Unable to initialize database
          </h2>
          <p
            style={{
              color: 'var(--color-muted)',
              marginBottom: '16px',
              wordBreak: 'break-word',
            }}
          >
            {status.message}
          </p>
          <button
            onClick={() => loadInitialState()}
            style={{
              padding: '6px 14px',
              backgroundColor: 'var(--color-mesh)',
              color: '#FFFFFF',
              border: 'none',
              borderRadius: 'var(--radius-sm)',
              cursor: 'pointer',
              fontWeight: 500,
            }}
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (status.status === 'corrupt_failure') {
    return (
      <div
        role="alert"
        style={{
          display: 'flex',
          height: '100vh',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: 'var(--color-canvas)',
          padding: '24px',
        }}
      >
        <div
          style={{
            maxWidth: '480px',
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-lg)',
            padding: '24px',
            boxShadow: '0 4px 12px rgba(0,0,0,0.05)',
          }}
        >
          <Badge variant="danger" className="mb-2">
            Corrupt Data
          </Badge>
          <h2
            style={{
              fontSize: '16px',
              fontWeight: 600,
              margin: '8px 0 12px',
              color: 'var(--color-danger)',
            }}
          >
            Database Integrity Verification Failed
          </h2>
          <p
            style={{
              color: 'var(--color-muted)',
              marginBottom: '16px',
              wordBreak: 'break-word',
            }}
          >
            {status.message}
          </p>
          <p style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
            AssetMesh refused to load this database because migrations or checksums do not match
            expected canonical definitions.
          </p>
        </div>
      </div>
    );
  }

  const selectedAsset =
    pageData?.items.find((i) => i.id === nav.selectedAssetId) || pageData?.items[0] || null;

  return (
    <div className="app-shell">
      {/* 1. Navigation Rail */}
      <NavigationRail
        currentSection={nav.section}
        currentModule={nav.module}
        capabilities={capabilities}
        onSelectModule={setModule}
        onSelectSection={setSection}
      />

      {/* 2. Collection Workspace (Ledger Rows) */}
      <AssetLedger
        module={nav.module}
        lifecycle={nav.lifecycle}
        sort={nav.sort}
        selectedKind={nav.kind}
        selectedTag={nav.tag}
        page={nav.page}
        pageSize={nav.pageSize}
        data={pageData}
        error={error}
        loading={loadingAssets}
        selectedAssetId={selectedAsset?.id || null}
        capabilities={capabilities}
        onSelectAsset={(id) => {
          setSelectedAssetId(id);
          setMobileInspectorOpen(true);
        }}
        onSelectLifecycle={setLifecycle}
        onSelectSort={setSort}
        onSelectKind={setKind}
        onSelectTag={setTag}
        onSelectPage={setPage}
        onResetFilters={resetFilters}
        onRetry={loadAssets}
      />

      {/* 3. Asset Inspector (Desktop column / responsive sheet) */}
      {(selectedAsset || mobileInspectorOpen) && (
        <AssetInspector
          asset={selectedAsset}
          onClose={() => setMobileInspectorOpen(false)}
          onSelectTag={(tag) => setTag(tag)}
        />
      )}
    </div>
  );
};
