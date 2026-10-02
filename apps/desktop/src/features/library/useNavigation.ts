import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  ActiveModule,
  ActiveSection,
  LifecycleOption,
  NavigationState,
  SortOption,
} from './types';
import type { SavedFilterState } from './SavedFilters';

const DEFAULT_STATE: NavigationState = {
  section: 'library',
  module: 'all',
  lifecycle: 'active',
  mediaStatus: null,
  sort: 'updated_desc',
  kind: null,
  tag: null,
  search: '',
  page: 1,
  pageSize: 25,
  selectedAssetId: null,
};

function parseHash(hash: string): NavigationState {
  if (!hash || !hash.startsWith('#/')) {
    return DEFAULT_STATE;
  }

  const raw = hash.slice(2);
  const [pathPart, queryPart] = raw.split('?');
  const params = new URLSearchParams(queryPart || '');

  let section: ActiveSection = 'library';
  if (pathPart === 'todos') section = 'todos';
  else if (pathPart === 'relations') section = 'relations';
  else if (pathPart === 'activity') section = 'activity';
  else if (pathPart === 'duplicates') section = 'duplicates';
  else if (pathPart === 'import-export') section = 'import-export';
  else if (pathPart === 'settings') section = 'settings';

  const rawModule = params.get('module');
  const module: ActiveModule =
    rawModule === 'media' ||
    rawModule === 'software' ||
    rawModule === 'services' ||
    rawModule === 'subscriptions' ||
    rawModule === 'info'
      ? rawModule
      : 'all';

  const rawLifecycle = params.get('lifecycle');
  const lifecycle: LifecycleOption =
    rawLifecycle === 'active_or_archived' || rawLifecycle === 'all'
      ? rawLifecycle
      : 'active';

  const rawSort = params.get('sort');
  const sort: SortOption =
    rawSort === 'updated_asc' ||
    rawSort === 'name_asc' ||
    rawSort === 'name_desc' ||
    rawSort === 'kind_asc'
      ? rawSort
      : 'updated_desc';

  const kind = params.get('kind') || null;
  const mediaStatus = params.get('status') || null;
  const tag = params.get('tag') || null;
  const search = params.get('q') || '';
  const page = Math.max(1, parseInt(params.get('page') || '1', 10) || 1);
  const selectedAssetId = params.get('asset') || null;

  return {
    section,
    module,
    lifecycle,
    mediaStatus,
    sort,
    kind,
    tag,
    search,
    page,
    pageSize: 25,
    selectedAssetId,
  };
}

function serializeHash(state: NavigationState): string {
  const params = new URLSearchParams();
  if (state.module !== 'all') params.set('module', state.module);
  if (state.lifecycle !== 'active') params.set('lifecycle', state.lifecycle);
  if (state.mediaStatus) params.set('status', state.mediaStatus);
  if (state.sort !== 'updated_desc') params.set('sort', state.sort);
  if (state.kind) params.set('kind', state.kind);
  if (state.tag) params.set('tag', state.tag);
  if (state.search.trim()) params.set('q', state.search.trim());
  if (state.page > 1) params.set('page', state.page.toString());
  if (state.selectedAssetId) params.set('asset', state.selectedAssetId);

  const queryStr = params.toString();
  const path = state.section === 'library' ? '' : state.section;
  return `#/${path}${queryStr ? '?' + queryStr : ''}`;
}

const STORAGE_KEY = 'assetmesh-navigation-v1';
const viewKey = (state: NavigationState) => state.section === 'library' ? `library:${state.module}` : state.section;

function readMemory(): { current?: string; views: Record<string, string> } {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}');
    const views: Record<string, string> = {};
    if (saved.views && typeof saved.views === 'object') {
      for (const value of Object.values(saved.views)) {
        if (typeof value === 'string' && value.startsWith('#/')) {
          const state = parseHash(value);
          views[viewKey(state)] = serializeHash(state);
        }
      }
    }
    return { current: typeof saved.current === 'string' && saved.current.startsWith('#/') ? saved.current : undefined, views };
  } catch { return { views: {} }; }
}

export function useNavigation() {
  const [initialMemory] = useState(readMemory);
  const memory = useRef(initialMemory);
  const [nav, setNav] = useState<NavigationState>(() => parseHash(window.location.hash || memory.current.current || ''));

  useEffect(() => {
    memory.current.views[viewKey(nav)] = serializeHash(nav);
    memory.current.current = serializeHash(nav);
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(memory.current)); } catch { /* Navigation still works without storage. */ }
  }, [nav]);

  useEffect(() => {
    const onHashChange = () => {
      setNav(parseHash(window.location.hash));
    };
    window.addEventListener('hashchange', onHashChange);
    return () => window.removeEventListener('hashchange', onHashChange);
  }, []);

  const updateNav = useCallback((updater: (prev: NavigationState) => NavigationState) => {
    setNav((prev) => {
      memory.current.views[viewKey(prev)] = serializeHash(prev);
      const next = updater(prev);
      if (
        prev.section === next.section &&
        prev.module === next.module &&
        prev.lifecycle === next.lifecycle &&
        prev.mediaStatus === next.mediaStatus &&
        prev.sort === next.sort &&
        prev.kind === next.kind &&
        prev.tag === next.tag &&
        prev.search === next.search &&
        prev.page === next.page &&
        prev.pageSize === next.pageSize &&
        prev.selectedAssetId === next.selectedAssetId
      ) {
        return prev;
      }
      const newHash = serializeHash(next);
      if (typeof window !== 'undefined' && window.location.hash !== newHash) {
        window.history.replaceState(null, '', newHash);
      }
      return next;
    });
  }, []);

  const setModule = useCallback((module: ActiveModule) => {
    updateNav((prev) => {
      if (prev.section === 'library' && prev.module === module) return prev;
      const saved = memory.current.views[`library:${module}`];
      return saved ? parseHash(saved) : { ...DEFAULT_STATE, module };
    });
  }, [updateNav]);

  const setSection = useCallback((section: ActiveSection) => {
    updateNav((prev) => {
      if (prev.section === section) return prev;
      if (section === 'relations') return { ...prev, section, page: 1 };
      const saved = memory.current.views[section];
      return saved ? parseHash(saved) : { ...prev, section, page: 1 };
    });
  }, [updateNav]);

  const setLifecycle = useCallback((lifecycle: LifecycleOption) => {
    updateNav((prev) => ({
      ...prev,
      lifecycle,
      page: 1,
    }));
  }, [updateNav]);

  const setMediaStatus = useCallback((mediaStatus: string | null) => {
    updateNav((prev) => ({
      ...prev,
      mediaStatus,
      page: 1,
    }));
  }, [updateNav]);

  const setSort = useCallback((sort: SortOption) => {
    updateNav((prev) => ({
      ...prev,
      sort,
      page: 1,
    }));
  }, [updateNav]);

  const setKind = useCallback((kind: string | null) => {
    updateNav((prev) => ({
      ...prev,
      kind,
      page: 1,
    }));
  }, [updateNav]);

  const setTag = useCallback((tag: string | null) => {
    updateNav((prev) => ({
      ...prev,
      tag,
      page: 1,
    }));
  }, [updateNav]);

  const setSearch = useCallback((search: string) => {
    updateNav((prev) => ({
      ...prev,
      search,
      page: 1,
    }));
  }, [updateNav]);

  const setPage = useCallback((page: number) => {
    updateNav((prev) => ({
      ...prev,
      page: Math.max(1, page),
    }));
  }, [updateNav]);

  const setSelectedAssetId = useCallback((selectedAssetId: string | null) => {
    updateNav((prev) => ({
      ...prev,
      selectedAssetId,
    }));
  }, [updateNav]);

  const resetFilters = useCallback(() => {
    updateNav((prev) => ({
      ...prev,
      lifecycle: 'active',
      mediaStatus: null,
      sort: 'updated_desc',
      kind: null,
      tag: null,
      search: '',
      page: 1,
    }));
  }, [updateNav]);

  const applySavedFilter = useCallback((filter: SavedFilterState) => {
    updateNav((prev) => ({ ...prev, ...filter, section: 'library', page: 1 }));
  }, [updateNav]);

  return {
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
  };
}
