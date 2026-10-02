import { WindowToolbar } from './WindowToolbar';
import React, { lazy, useEffect, useState } from 'react';
import { DeferredFeature } from './DeferredFeature';
import { FirstListTiming, useStartupStage } from './startup-timing';
import { AssetInspector } from '../features/library/AssetInspector';
import { AssetLedger } from '../features/library/AssetLedger';
import { MutationFeedback } from '../features/library/MutationFeedback';
import { useMediaStatusActions } from '../features/library/useMediaStatusActions';
import { useLibraryPage } from '../features/library/useLibraryPage';
import { TodoList } from '../features/library/TodoList';
import { NavigationRail } from '../features/library/NavigationRail';
import { getTransport } from '../features/library/transport';
import type {
  LibraryQuery,
} from '../features/library/types';
import { useNavigation } from '../features/library/useNavigation';
import { t } from '../i18n';
import { useDesktopSession } from './useDesktopSession';
import { StartupScreen } from './StartupScreen';
import type { SavedFilterState } from '../features/library/SavedFilters';
import '../ui/theme.css';

const AssetDetailView = lazy(() => import('../features/library/AssetDetailView').then((m) => ({ default: m.AssetDetailView })));
const CreateMediaModal = lazy(() => import('../features/library/CreateMediaModal').then((m) => ({ default: m.CreateMediaModal })));
const CreateSoftwareModal = lazy(() => import('../features/library/CreateSoftwareModal').then((m) => ({ default: m.CreateSoftwareModal })));
const SoftwareDiscoveryModal = lazy(() => import('../features/library/SoftwareDiscoveryModal').then((m) => ({ default: m.SoftwareDiscoveryModal })));
const CreateServiceModal = lazy(() => import('../features/library/CreateServiceModal').then((m) => ({ default: m.CreateServiceModal })));
const CreateLocalServiceModal = lazy(() => import('../features/library/CreateLocalServiceModal').then((m) => ({ default: m.CreateLocalServiceModal })));
const ServicesWorkspace = lazy(() => import('../features/library/ServicesWorkspace').then((m) => ({ default: m.ServicesWorkspace })));
const CreateInfoModal = lazy(() => import('../features/library/InfoEditor').then((m) => ({ default: m.CreateInfoModal })));
const InfoCsvImport = lazy(() => import('../features/library/InfoCsvImport').then((m) => ({ default: m.InfoCsvImport })));
const RelationExplorer = lazy(() => import('../features/library/RelationExplorer').then((m) => ({ default: m.RelationExplorer })));
const ActivityFeed = lazy(() => import('../features/library/ActivityFeed').then((m) => ({ default: m.ActivityFeed })));
const DuplicateReview = lazy(() => import('../features/library/DuplicateReview').then((m) => ({ default: m.DuplicateReview })));
const ImportExportView = lazy(() => import('../features/library/ImportExportView').then((m) => ({ default: m.ImportExportView })));
const SettingsView = lazy(() => import('../features/library/SettingsView').then((m) => ({ default: m.SettingsView })));

export const App: React.FC = () => {
  const transport = getTransport();
  const { status, capabilities, restorePending, setupBusy, retrySetup, chooseDatabaseFolder, theme, handleThemeChange } = useDesktopSession(transport);
  const [mobileInspectorOpen, setMobileInspectorOpen] = useState(() => window.innerWidth >= 760);
  const [dialog, setDialog] = useState<'media' | 'software' | 'softwareDiscovery' | 'subscription' | 'localService' | 'info' | 'infoImport' | null>(null);
  const [activeDetailId, setActiveDetailId] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(() => window.innerWidth >= 760);
  const [todoSearch, setTodoSearch] = useState('');
  const [todoQuery, setTodoQuery] = useState('');
  const [todoKind, setTodoKind] = useState<string | null>(null);
  const [todoSort, setTodoSort] = useState<LibraryQuery['sort']>('updated_desc');
  useEffect(() => {
    const openService = (event: Event) => {
      if (!(event instanceof CustomEvent)) return;
      const assetId: unknown = event.detail;
      if (typeof assetId !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(assetId)) return;
      setActiveDetailId(assetId);
    };
    const manageServices = () => setActiveDetailId(null);
    window.addEventListener('assetmesh-open-service', openService);
    window.addEventListener('assetmesh-manage-services', manageServices);
    return () => {
      window.removeEventListener('assetmesh-open-service', openService);
      window.removeEventListener('assetmesh-manage-services', manageServices);
    };
  }, []);

  const {
    nav,
    setModule,
    setSection,
    setLifecycle,
    setMediaStatus,
    setSort,
    setKind,
    setTag,
    setSearch,
    setPage,
    setSelectedAssetId,
    resetFilters,
    applySavedFilter,
  } = useNavigation();

  const dataViewKey = nav.section === 'todos' ? JSON.stringify(['todos', todoQuery, todoKind, todoSort]) : JSON.stringify([nav.section, nav.module, nav.lifecycle, nav.mediaStatus, nav.sort, nav.kind, nav.tag, nav.search]);
  const viewKey = `${dataViewKey}:${nav.page}:${nav.pageSize}`;

  const [searchInput, setSearchInput] = useState(nav.search);

  useEffect(() => {
    const timer = setTimeout(() => { setTodoQuery(todoSearch.trim()); }, 300);
    return () => clearTimeout(timer);
  }, [todoSearch]);

  useEffect(() => { setSearchInput(nav.search); }, [nav.section, nav.module, nav.search]);

  // Debounce search input changes (300ms) to update navigation state
  useEffect(() => {
    const timer = setTimeout(() => {
      if (searchInput !== nav.search) {
        setSearch(searchInput);
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [searchInput, nav.search, setSearch]);

  // Dismiss active asset detail view on Escape key
  useEffect(() => {
    if (!activeDetailId) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setActiveDetailId(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeDetailId]);

  // 服务 manages local projects; 订阅 keeps the billed services.
  const moduleScope = nav.module === 'all' ? {}
    : nav.module === 'services' ? { modules: ['services'], kinds: nav.kind ? [nav.kind] : ['service.local'] }
    : nav.module === 'subscriptions' ? { modules: ['services'], kinds: nav.kind ? [nav.kind] : ['service.saas', 'service.api', 'service.vps', 'service.domain'] }
    : { modules: [nav.module], kinds: nav.kind ? [nav.kind] : undefined };
  const query: LibraryQuery = nav.section === 'todos'
    ? { lifecycle: 'active', modules: ['media'], media_status: 'in_progress',
        kinds: todoKind ? [todoKind] : undefined, sort: todoSort,
        limit: nav.pageSize, offset: (nav.page - 1) * nav.pageSize }
    : { lifecycle: nav.module === 'services' ? 'active' : nav.lifecycle, ...moduleScope, tags: nav.tag ? [nav.tag] : undefined,
        media_status: nav.module === 'media' && nav.mediaStatus ? nav.mediaStatus : undefined,
        sort: nav.sort, limit: nav.pageSize, offset: (nav.page - 1) * nav.pageSize };
  const countsQuery: LibraryQuery | null = nav.section === 'library' && nav.module === 'media'
    ? { lifecycle: nav.lifecycle, modules: ['media'], kinds: nav.kind ? [nav.kind] : undefined,
        tags: nav.tag ? [nav.tag] : undefined } : null;
  const { data: currentPageData, statusCounts, loading: loadingAssets, error, refresh: loadAssets, updateMedia } = useLibraryPage(
    transport, status.status === 'ready' && ['library', 'todos', 'relations'].includes(nav.section),
    query, nav.section === 'todos' ? todoQuery : nav.search.trim(), countsQuery, nav.section,
  );
  useEffect(() => {
    if (!currentPageData) return;
    if (nav.section === 'todos' && currentPageData.total !== null && nav.page > 1 && currentPageData.offset >= currentPageData.total) {
      setPage(Math.max(1, Math.ceil(currentPageData.total / nav.pageSize)));
      return;
    }
    if (!currentPageData.items.some((item) => item.id === nav.selectedAssetId)) {
      setSelectedAssetId(currentPageData.items[0]?.id ?? null);
    }
  }, [currentPageData, nav.section, nav.page, nav.pageSize, nav.selectedAssetId, setPage, setSelectedAssetId]);

  const refreshAssetsRef = React.useRef(loadAssets);
  refreshAssetsRef.current = loadAssets;
  const mediaActions = useMediaStatusActions(updateMedia);
  const transitionMedia = mediaActions.transition;

  useStartupStage('shell_visible', status.status === 'ready');
  const initialStartupView = React.useRef(`${nav.section}:${nav.module}`);
  const startupListReady = status.status === 'ready'
    && initialStartupView.current === `${nav.section}:${nav.module}`
    && (nav.section === 'library' || nav.section === 'todos')
    && currentPageData !== null && !loadingAssets && !error;

  if (status.status !== 'ready' || restorePending) {
    return <StartupScreen status={status} restorePending={restorePending} setupBusy={setupBusy}
      retrySetup={retrySetup} chooseDatabaseFolder={chooseDatabaseFolder} />;
  }

  const selectedAsset =
    currentPageData?.items.find((i) => i.id === nav.selectedAssetId) || currentPageData?.items[0] || null;

  const inspector = selectedAsset && mobileInspectorOpen && (
    <AssetInspector
      asset={selectedAsset}
      mobileOpen={mobileInspectorOpen}
      onClose={() => setMobileInspectorOpen(false)}
      onSelectTag={(tag) => setTag(tag)}
      onOpenDetail={(id) => setActiveDetailId(id)}
      onOpenRelations={() => setSection('relations')}
      mutationPending={mediaActions.feedback.pending}
      onUpdateProgress={async (asset, progress) => { await mediaActions.updateProgress(asset, progress); }}
      onMediaStatusChange={async (asset, nextStatus) => {
        await transitionMedia(asset, nextStatus);
      }}
    />
  );

  const canInspect = nav.section === 'todos' || (nav.section === 'library' && nav.module !== 'services');
  const canSearch = canInspect;
  const createAsset = nav.section === 'todos' || nav.module === 'media' || nav.module === 'all'
    ? () => setDialog('media') : nav.module === 'software' ? () => setDialog('software')
    : nav.module === 'subscriptions' ? () => setDialog('subscription')
    : nav.module === 'services' ? () => setDialog('localService') : () => setDialog('info');
  const sectionTitle = nav.section === 'todos' ? t('To-do List') : nav.section === 'settings' ? t('Settings')
    : nav.section === 'relations' ? t('Relations') : nav.section === 'activity' ? t('Activity')
    : nav.section === 'duplicates' ? t('Duplicates') : nav.section === 'import-export' ? t('Portable Data')
    : nav.module === 'all' ? t('All Assets') : nav.module === 'media' ? t('Media')
    : nav.module === 'software' ? t('Software') : nav.module === 'services' ? t('Services')
    : nav.module === 'subscriptions' ? t('Subscriptions') : t('Information');

  return (
    <div className={`app-shell${nav.section === 'todos' ? ' app-shell-todos' : ''}${sidebarOpen ? '' : ' sidebar-collapsed'}`}>
      <WindowToolbar title={sectionTitle} sidebarOpen={sidebarOpen} inspectorOpen={mobileInspectorOpen}
        onToggleSidebar={() => setSidebarOpen((open) => !open)}
        onToggleInspector={canInspect ? () => setMobileInspectorOpen((open) => !open) : undefined}
        hideCreateButton={nav.section === 'library' && nav.module === 'services'}
        onCreate={nav.section === 'library' || nav.section === 'todos' ? createAsset : undefined}
        search={nav.section === 'todos' ? todoSearch : searchInput}
        searchLabel={nav.section === 'todos' ? t('Search media to-dos') : t('Search library assets')}
        onSearch={canSearch ? (value) => { if (nav.section === 'todos') { setTodoSearch(value); setPage(1); } else setSearchInput(value); } : undefined}
        onFilter={canSearch ? () => document.querySelector<HTMLSelectElement>('[aria-label="' + t('Filter by kind') + '"]')?.focus() : undefined}
      />
      <div className="app-body">
      {/* 1. Navigation Rail */}
      <NavigationRail
        currentSection={nav.section}
        currentModule={nav.module}
        capabilities={capabilities}
        onSelectModule={(module) => { setModule(module); if (window.innerWidth < 760) setSidebarOpen(false); }}
        onSelectSection={(section) => { setSection(section); if (window.innerWidth < 760) setSidebarOpen(false); }}
      />

      {/* Main Workspace Area */}
      <DeferredFeature key={`${nav.section}:${nav.module}`}>
      <FirstListTiming ready={startupListReady} />
      {nav.section === 'todos' ? (
        <>
          <TodoList
            data={currentPageData}
            viewKey={viewKey}
            feedback={mediaActions.feedback}
            loading={loadingAssets}
            error={error}
            page={nav.page}
            pageSize={nav.pageSize}
            selectedAssetId={selectedAsset?.id || null}
            onSelectAsset={(id) => {
              setSelectedAssetId(id);
              setMobileInspectorOpen(true);
            }}
            onSelectPage={setPage}
            kinds={capabilities?.asset_kinds.filter((kind) => kind.startsWith('media.')) ?? []}
            selectedKind={todoKind}
            sort={todoSort ?? 'updated_desc'}
            filtered={!!todoQuery || !!todoKind}
            onSelectKind={(kind) => { setTodoKind(kind); setPage(1); }}
            onSelectSort={(sort) => { setTodoSort(sort); setPage(1); }}
            onResetFilters={() => { setTodoSearch(''); setTodoQuery(''); setTodoKind(null); setPage(1); }}
            onOpenMedia={() => setModule('media')}
            onRetry={loadAssets}
            onTransition={transitionMedia}
          />
          {inspector}
        </>
      ) : nav.section === 'relations' ? (
        <main
          data-testid="relations-workspace"
          style={{
            flex: 1,
            padding: '24px',
            overflowY: 'auto',
            backgroundColor: 'var(--color-canvas)',
          }}
        >
          {selectedAsset ? (
            <RelationExplorer
              rootAsset={selectedAsset}
              capabilities={capabilities}
              onOpenAssetDetail={(id) => setActiveDetailId(id)}
              onAssetUpdated={() => refreshAssetsRef.current()}
            />
          ) : (
            <div
              style={{
                padding: '40px',
                textAlign: 'center',
                color: 'var(--color-muted)',
              }}
            >
              {t(
                'No asset selected. Select an asset from the library to explore relations and impact.'
              )}
            </div>
          )}
        </main>
      ) : nav.section === 'activity' ? (
        <main
          data-testid="activity-workspace"
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
            backgroundColor: 'var(--color-canvas)',
          }}
        >
          <ActivityFeed
            capabilities={capabilities}
            onOpenAssetDetail={(id) => setActiveDetailId(id)}
          />
        </main>
      ) : nav.section === 'duplicates' ? (
        <main
          data-testid="duplicates-workspace"
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
            backgroundColor: 'var(--color-canvas)',
          }}
        >
          <DuplicateReview
            capabilities={capabilities}
            onOpenAssetDetail={(id) => setActiveDetailId(id)}
            onAssetMerged={() => loadAssets()}
          />
        </main>
      ) : nav.section === 'import-export' ? (
        <main style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
          <ImportExportView />
        </main>
      ) : nav.section === 'settings' ? (
        <main style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
          <SettingsView currentTheme={theme} onThemeChange={handleThemeChange} />
        </main>
      ) : (
        nav.module === 'services' ? (
          <ServicesWorkspace
            data={currentPageData}
            error={error}
            loading={loadingAssets}
            searchQuery={searchInput}
            page={nav.page}
            pageSize={nav.pageSize}
            selectedAssetId={selectedAsset?.id || null}
            runtimeSupported={capabilities?.features.local_service_runtime ?? false}
            onNewAsset={() => setDialog('localService')}
            onSelectAsset={(id) => {
              setSelectedAssetId(id);
              setMobileInspectorOpen(true);
            }}
            onSearchChange={setSearchInput}
            onSelectPage={setPage}
            onResetFilters={() => {
              setSearchInput('');
              resetFilters();
            }}
            onRetry={loadAssets}
            onOpenDetail={(id) => setActiveDetailId(id)}
          />
        ) : (
        <>
          {/* 2. Collection Workspace (Ledger Rows) */}
          <div className="collection-workspace">
            <div style={{ padding: '0 16px' }}><MutationFeedback {...mediaActions.feedback} /></div>
            <AssetLedger
              viewKey={viewKey}
              module={nav.module}
              lifecycle={nav.lifecycle}
              sort={nav.sort}
              selectedKind={nav.kind}
              selectedTag={nav.tag}
              selectedMediaStatus={nav.module === 'media' ? nav.mediaStatus : null}
              statusCounts={statusCounts}
              searchQuery={searchInput}
              page={nav.page}
              pageSize={nav.pageSize}
              data={currentPageData}
              error={error}
              loading={loadingAssets}
              selectedAssetId={selectedAsset?.id || null}
              capabilities={capabilities}
              onNewAsset={createAsset}
              onImportInfo={() => setDialog('infoImport')}
              onApplySavedFilter={(filter: SavedFilterState) => {
                setSearchInput(filter.search);
                applySavedFilter(filter);
              }}
              onDiscoverSoftware={() => setDialog('softwareDiscovery')}
              onSelectAsset={(id) => {
                setSelectedAssetId(id);
                setMobileInspectorOpen(true);
              }}
              onOpenDetail={(id) => setActiveDetailId(id)}
              onSelectLifecycle={setLifecycle}
              onSelectMediaStatus={setMediaStatus}
              onSelectSort={setSort}
              onSelectKind={setKind}
              onSelectTag={setTag}
              onSearchChange={setSearchInput}
              onSelectPage={setPage}
              onResetFilters={() => {
                setSearchInput('');
                resetFilters();
              }}
              onRetry={loadAssets}
            />

          </div>
          {inspector}
        </>
        )
      )}
      </DeferredFeature>

      </div>

      {/* 4. Full Unified Asset Detail View */}
      {activeDetailId && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={t('Asset Detail View')}
          style={{
            position: 'fixed',
            inset: 0,
            backgroundColor: 'rgba(0, 0, 0, 0.45)',
            backdropFilter: 'blur(2px)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 100,
            padding: '24px',
          }}
          onClick={(e) => {
            if (e.target === e.currentTarget) {
              setActiveDetailId(null);
            }
          }}
        >
          <div
            style={{
              width: '100%',
              maxWidth: '680px',
              maxHeight: '88vh',
              backgroundColor: 'var(--color-surface)',
              borderRadius: 'var(--radius-lg)',
              border: '1px solid var(--color-border)',
              boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
              display: 'flex',
              flexDirection: 'column',
              overflow: 'hidden',
            }}
          >
            <DeferredFeature key={activeDetailId} onClose={() => setActiveDetailId(null)}><AssetDetailView
              assetId={activeDetailId}
              capabilities={capabilities}
              onOpenAssetDetail={(id) => setActiveDetailId(id)}
              onClose={() => setActiveDetailId(null)}
              onFollowRedirect={(survivorId) => {
                setSelectedAssetId(survivorId);
                setActiveDetailId(survivorId);
              }}
              onSelectTag={(tag) => {
                setTag(tag);
                setActiveDetailId(null);
              }}
              onAssetUpdated={(receipt, fresh) => { void updateMedia(receipt, fresh); }}
            /></DeferredFeature>
          </div>
        </div>
      )}

      {dialog === 'info' && <DeferredFeature dialog onClose={() => setDialog(null)}><CreateInfoModal onClose={() => setDialog(null)} onCreated={(id) => {
        setDialog(null);
        setSelectedAssetId(id);
        loadAssets();
        setActiveDetailId(id);
      }} /></DeferredFeature>}
      {dialog === 'infoImport' && <DeferredFeature dialog onClose={() => setDialog(null)}><InfoCsvImport onClose={() => setDialog(null)} onImported={() => {
        setDialog(null);
        if (nav.page === 1) loadAssets();
        else setPage(1);
      }} /></DeferredFeature>}

      {/* 5. Create Media Modal */}
      {dialog === 'media' && <DeferredFeature dialog onClose={() => setDialog(null)}><CreateMediaModal
        isOpen={dialog === 'media'}
        onClose={() => setDialog(null)}
        onCreated={(newId) => {
          loadAssets();
          setSelectedAssetId(newId);
          setActiveDetailId(newId);
        }}
      /></DeferredFeature>}

      {/* 6. Create Software Modal */}
      {dialog === 'software' && <DeferredFeature dialog onClose={() => setDialog(null)}><CreateSoftwareModal
        isOpen={dialog === 'software'}
        onClose={() => setDialog(null)}
        onCreated={(newId) => {
          loadAssets();
          setSelectedAssetId(newId);
          setActiveDetailId(newId);
        }}
      /></DeferredFeature>}

      {/* 7. Software Discovery & Adoption Modal */}
      {dialog === 'softwareDiscovery' && <DeferredFeature dialog onClose={() => setDialog(null)}><SoftwareDiscoveryModal
        isOpen={dialog === 'softwareDiscovery'}
        onClose={() => setDialog(null)}
        onAdopted={(adoptedId) => {
          loadAssets();
          setSelectedAssetId(adoptedId);
          setActiveDetailId(adoptedId);
        }}
      /></DeferredFeature>}

      {/* 8. Create Service Modal (订阅 page) */}
      {dialog === 'subscription' && <DeferredFeature dialog onClose={() => setDialog(null)}><CreateServiceModal
        isOpen={dialog === 'subscription'}
        onClose={() => setDialog(null)}
        onCreated={(newId) => {
          loadAssets();
          setSelectedAssetId(newId);
          setActiveDetailId(newId);
        }}
      /></DeferredFeature>}

      {/* 9. Create Local Service Modal (服务 page) */}
      {dialog === 'localService' && <DeferredFeature dialog onClose={() => setDialog(null)}><CreateLocalServiceModal
        isOpen={dialog === 'localService'}
        onClose={() => setDialog(null)}
        onCreated={(newId) => {
          loadAssets();
          setSelectedAssetId(newId);
          setActiveDetailId(newId);
        }}
      /></DeferredFeature>}
    </div>
  );
};
