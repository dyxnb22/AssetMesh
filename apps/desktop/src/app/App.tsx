import React, { useEffect, useState } from 'react';
import { getTransport } from '../features/library/transport';
import type { AppCapabilities, AppStatus, AssetSummary, Page } from '../features/library/types';
import { Badge } from '../ui/Badge';
import '../ui/theme.css';

export const App: React.FC = () => {
  const [status, setStatus] = useState<AppStatus>({ status: 'loading' });
  const [capabilities, setCapabilities] = useState<AppCapabilities | null>(null);
  const [page, setPage] = useState<Page<AssetSummary> | null>(null);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);

  const transport = getTransport();

  const loadData = async () => {
    try {
      const appStatus = await transport.getStatus();
      setStatus(appStatus);

      if (appStatus.status === 'ready') {
        const caps = await transport.getCapabilities();
        setCapabilities(caps);
        const assetsPage = await transport.listAssets();
        setPage(assetsPage);
        if (assetsPage.items.length > 0 && !selectedAssetId) {
          setSelectedAssetId(assetsPage.items[0].id);
        }
      }
    } catch (err: unknown) {
      setStatus({ status: 'setup_failure', message: err instanceof Error ? err.message : String(err) });
    }
  };

  useEffect(() => {
    loadData();
  }, []);

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
          <div style={{ fontSize: '18px', fontWeight: 600, color: 'var(--color-ink)', marginBottom: '8px' }}>
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
          <Badge variant="attention" className="mb-2">Setup Required</Badge>
          <h2 style={{ fontSize: '16px', fontWeight: 600, margin: '8px 0 12px', color: 'var(--color-ink)' }}>
            Unable to initialize database
          </h2>
          <p style={{ color: 'var(--color-muted)', marginBottom: '16px', wordBreak: 'break-word' }}>
            {status.message}
          </p>
          <button
            onClick={() => loadData()}
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
          <Badge variant="danger" className="mb-2">Corrupt Data</Badge>
          <h2 style={{ fontSize: '16px', fontWeight: 600, margin: '8px 0 12px', color: 'var(--color-danger)' }}>
            Database Integrity Verification Failed
          </h2>
          <p style={{ color: 'var(--color-muted)', marginBottom: '16px', wordBreak: 'break-word' }}>
            {status.message}
          </p>
          <p style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
            AssetMesh refused to load this database because migrations or checksums do not match expected canonical definitions.
          </p>
        </div>
      </div>
    );
  }

  const selectedAsset = page?.items.find((i) => i.id === selectedAssetId) || page?.items[0];

  return (
    <div
      style={{
        display: 'flex',
        height: '100vh',
        width: '100vw',
        overflow: 'hidden',
        backgroundColor: 'var(--color-canvas)',
      }}
    >
      {/* 1. Navigation Rail */}
      <nav
        aria-label="Library Navigation"
        style={{
          width: '200px',
          backgroundColor: 'var(--color-canvas)',
          borderRight: '1px solid var(--color-border)',
          display: 'flex',
          flexDirection: 'column',
          padding: '16px 8px',
          flexShrink: 0,
        }}
      >
        <div style={{ padding: '0 8px 16px', borderBottom: '1px solid var(--color-border)' }}>
          <h1 style={{ fontSize: '14px', fontWeight: 600, color: 'var(--color-ink)', letterSpacing: '-0.01em' }}>
            AssetMesh
          </h1>
          {capabilities && (
            <div style={{ fontSize: '11px', color: 'var(--color-muted)', marginTop: '2px' }}>
              v{capabilities.version}
            </div>
          )}
        </div>

        <div style={{ marginTop: '12px', display: 'flex', flexDirection: 'column', gap: '2px' }}>
          <div
            style={{
              padding: '6px 8px',
              borderRadius: 'var(--radius-sm)',
              backgroundColor: 'var(--color-surface)',
              color: 'var(--color-mesh)',
              fontWeight: 600,
              fontSize: '12px',
            }}
          >
            All Assets
          </div>
          <div style={{ padding: '6px 8px', color: 'var(--color-muted)', fontSize: '12px' }}>
            Media
          </div>
          <div style={{ padding: '6px 8px', color: 'var(--color-muted)', fontSize: '12px' }}>
            Software
          </div>
          <div style={{ padding: '6px 8px', color: 'var(--color-muted)', fontSize: '12px' }}>
            Services
          </div>
          <div style={{ height: '1px', backgroundColor: 'var(--color-border)', margin: '8px 0' }} />
          <div style={{ padding: '6px 8px', color: 'var(--color-muted)', fontSize: '12px' }}>
            Relations
          </div>
          <div style={{ padding: '6px 8px', color: 'var(--color-muted)', fontSize: '12px' }}>
            Activity
          </div>
        </div>
      </nav>

      {/* 2. Collection Workspace (Ledger Rows) */}
      <main
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          minWidth: 0,
          backgroundColor: 'var(--color-surface)',
        }}
      >
        {/* Workspace Toolbar */}
        <header
          style={{
            padding: '12px 16px',
            borderBottom: '1px solid var(--color-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div>
            <h2 style={{ fontSize: '15px', fontWeight: 600, color: 'var(--color-ink)' }}>All Assets</h2>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
              {page ? `${page.total ?? page.items.length} items recorded` : 'Loading...'}
            </div>
          </div>
        </header>

        {/* Ledger Table / List */}
        <div style={{ flex: 1, overflowY: 'auto' }} tabIndex={0} aria-label="Asset Ledger">
          {page?.items.length === 0 ? (
            <div style={{ padding: '32px 16px', textAlign: 'center', color: 'var(--color-muted)' }}>
              No assets in library.
            </div>
          ) : (
            <div role="table" aria-label="Assets">
              {page?.items.map((asset) => {
                const isSelected = asset.id === selectedAssetId;
                return (
                  <div
                    key={asset.id}
                    role="row"
                    tabIndex={0}
                    onClick={() => setSelectedAssetId(asset.id)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        setSelectedAssetId(asset.id);
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
                    }}
                    className="asset-row"
                  >
                    <div style={{ flex: 1, minWidth: 0, paddingRight: '12px' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        <span style={{ fontWeight: 600, color: 'var(--color-ink)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
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
                        <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginTop: '2px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {asset.subtitle}
                        </div>
                      )}
                    </div>

                    <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexShrink: 0 }}>
                      {asset.tags.map((t) => (
                        <span key={t} style={{ fontSize: '11px', color: 'var(--color-muted)', backgroundColor: 'var(--color-canvas)', padding: '2px 6px', borderRadius: 'var(--radius-sm)' }}>
                          #{t}
                        </span>
                      ))}
                      <span style={{ fontSize: '11px', color: 'var(--color-muted)', marginLeft: '8px' }}>
                        {asset.updated_at.slice(0, 10)}
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </main>

      {/* 3. Fixed-width Inspector */}
      <aside
        aria-label="Asset Inspector"
        style={{
          width: '360px',
          borderLeft: '1px solid var(--color-border)',
          backgroundColor: 'var(--color-canvas)',
          display: 'flex',
          flexDirection: 'column',
          overflowY: 'auto',
          padding: '16px',
          flexShrink: 0,
        }}
      >
        {selectedAsset ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div>
              <Badge variant="mesh" className="mb-1">{selectedAsset.kind}</Badge>
              <h3 style={{ fontSize: '18px', fontWeight: 600, color: 'var(--color-ink)', marginTop: '4px' }}>
                {selectedAsset.name}
              </h3>
              {selectedAsset.subtitle && (
                <p style={{ color: 'var(--color-muted)', fontSize: '13px', marginTop: '2px' }}>
                  {selectedAsset.subtitle}
                </p>
              )}
            </div>

            <div
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-md)',
                padding: '12px',
                fontSize: '12px',
              }}
            >
              <div style={{ color: 'var(--color-muted)', marginBottom: '4px' }}>Asset ID</div>
              <code style={{ fontSize: '11px', wordBreak: 'break-all' }}>{selectedAsset.id}</code>

              <div style={{ color: 'var(--color-muted)', marginTop: '8px', marginBottom: '4px' }}>Lifecycle</div>
              <div>{selectedAsset.lifecycle}</div>

              <div style={{ color: 'var(--color-muted)', marginTop: '8px', marginBottom: '4px' }}>Last Modified</div>
              <div>{selectedAsset.updated_at}</div>
            </div>

            {selectedAsset.tags.length > 0 && (
              <div>
                <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-ink)', marginBottom: '6px' }}>
                  Tags
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px' }}>
                  {selectedAsset.tags.map((t) => (
                    <Badge key={t} variant="muted">#{t}</Badge>
                  ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          <div style={{ color: 'var(--color-muted)', textAlign: 'center', marginTop: '48px' }}>
            Select an asset to inspect details.
          </div>
        )}
      </aside>
    </div>
  );
};
